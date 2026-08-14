use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_registry::{CancelSignal, ChatTriggerFamily, TriggerRegistry, effective_config};
use forge_storage::{ActionRepo, TriggerInstanceRepo};
use forge_types::{
    ArgStack, ChatPayload, EventId, PermissionRung, TriggerConfig, TriggerInstance,
    TriggerInstanceId, Variant,
};
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

use crate::cooldown::CooldownMap;
use crate::{Config, EventBus, EventSubscription, QueueSchedulerHandle, SchedulerRequest};

#[derive(Clone)]
pub struct TriggerEvaluatorHandle {
    cancel: CancelSignal,
}

impl TriggerEvaluatorHandle {
    pub fn shutdown(self) {
        self.cancel.cancel();
    }
}

pub struct TriggerEvaluator {
    bus: Arc<EventBus>,
    registry: Arc<TriggerRegistry>,
    actions: Arc<dyn ActionRepo>,
    trigger_instances: Arc<dyn TriggerInstanceRepo>,
    scheduler: QueueSchedulerHandle,
    subscription: EventSubscription,
    cooldowns: CooldownMap,
}

impl TriggerEvaluator {
    pub fn spawn(
        bus: Arc<EventBus>,
        registry: Arc<TriggerRegistry>,
        actions: Arc<dyn ActionRepo>,
        trigger_instances: Arc<dyn TriggerInstanceRepo>,
        scheduler: QueueSchedulerHandle,
        config: Config,
    ) -> TriggerEvaluatorHandle {
        let subscription = bus.subscribe();
        let evaluator = Self {
            bus,
            registry,
            actions,
            trigger_instances,
            scheduler,
            subscription,
            cooldowns: CooldownMap::new(config.max_cooldown_entries),
        };
        let cancel = CancelSignal::new();
        let cancel_clone = cancel.clone();
        tokio::spawn(async move { evaluator.run(cancel_clone).await });
        TriggerEvaluatorHandle { cancel }
    }

    async fn run(mut self, cancel: CancelSignal) {
        while !cancel.is_cancelled() {
            match self.subscription.recv().await {
                Ok(event) => self.handle(event).await,
                Err(_) => break,
            }
        }
    }

    async fn handle(&mut self, event: forge_events::Event) {
        let descriptors: Vec<_> = self
            .registry
            .all()
            .filter(|d| {
                let filter = d.event_filter();
                let source_ok = filter.source.is_none_or(|s| s == event.source);
                let prefix_ok = filter
                    .kind_prefix
                    .as_deref()
                    .is_none_or(|p| event.kind.starts_with(p));
                source_ok && prefix_ok
            })
            .collect();

        if descriptors.is_empty() {
            return;
        }

        let actions = match self.actions.list().await {
            Ok(a) => a,
            Err(e) => {
                warn!("trigger_evaluator: action_repo.list failed: {e}");
                return;
            }
        };

        let mut decided: HashMap<TriggerInstanceId, Option<ArgStack>> = HashMap::new();

        for action in &actions {
            if !action.enabled {
                continue;
            }

            let instances = match self.trigger_instances.list_for_action(action.id).await {
                Ok(t) => t,
                Err(e) => {
                    warn!("trigger_evaluator: trigger_instance_repo.list_for_action failed: {e}");
                    continue;
                }
            };

            for instance in &instances {
                if !instance.enabled {
                    continue;
                }

                let decision = match decided.get(&instance.id) {
                    Some(cached) => cached.clone(),
                    None => {
                        let fresh = self.decide(instance, &event);
                        decided.insert(instance.id, fresh.clone());
                        fresh
                    }
                };

                let Some(args) = decision else {
                    continue;
                };

                let req = SchedulerRequest {
                    queue_id: action.queue_id,
                    action_id: action.id,
                    trigger_event_id: event.id,
                    trigger_kind: Some(instance.kind_id.clone()),
                    initial_args: args,
                    bypass_pause: action.bypass_pause,
                };
                if let Err(e) = self.scheduler.dispatch(req).await {
                    warn!("trigger_evaluator: scheduler dispatch failed: {e}");
                }
            }
        }
    }

    /// Publishes the decision record, so callers must invoke it once per (event, instance) pair.
    fn decide(&mut self, instance: &TriggerInstance, event: &Event) -> Option<ArgStack> {
        let descriptor = match self.registry.get(&instance.kind_id) {
            Some(d) => d,
            None => {
                warn!(
                    "unknown trigger kind_id: {} - trigger will never fire",
                    instance.kind_id
                );
                return None;
            }
        };

        let filter = descriptor.event_filter();
        let source_ok = filter.source.is_none_or(|s| s == event.source);
        let prefix_ok = filter
            .kind_prefix
            .as_deref()
            .is_none_or(|p| event.kind.starts_with(p));

        if !source_ok || !prefix_ok {
            return None;
        }

        if !scope_matches(instance, event) {
            return None;
        }

        let effective = effective_config(&descriptor.default_config(), &instance.overrides);
        if !descriptor.matches_trigger(&effective, event) {
            return None;
        }

        let args = descriptor.build_arg_stack(event);
        let chat_family = descriptor.chat_trigger_family();

        if chat_family == Some(ChatTriggerFamily::Command) {
            self.bus.publish(Event::caused_by(
                EventSource::Core,
                "command.matched",
                json!({
                    "command": command_phrase(&effective),
                    "kind_id": instance.kind_id,
                }),
                event.id,
            ));
        }

        if chat_family.is_some() {
            let resolved = resolve_rung(event);
            if resolved < instance.permission_rung {
                self.publish_blocked(
                    instance,
                    event.id,
                    BlockReason::Permission {
                        required: instance.permission_rung,
                        resolved,
                    },
                );
                return None;
            }
        }

        if let Some(remaining) = self.cooldown_remaining(instance, &args, event.id) {
            self.publish_blocked(instance, event.id, BlockReason::Cooldown { remaining });
            return None;
        }

        Some(args)
    }

    fn publish_blocked(&self, instance: &TriggerInstance, cause: EventId, reason: BlockReason) {
        self.bus.publish(Event::caused_by(
            EventSource::Core,
            "trigger.blocked",
            reason.into_payload(instance),
            cause,
        ));
    }

    fn cooldown_remaining(
        &mut self,
        instance: &TriggerInstance,
        args: &ArgStack,
        event_id: EventId,
    ) -> Option<Duration> {
        if instance.cooldown_secs == 0 {
            return None;
        }

        let key = if instance.cooldown_global {
            (instance.id, None)
        } else {
            (instance.id, Some(arg_stack_user(args)?))
        };

        let window = Duration::from_secs(instance.cooldown_secs as u64);
        self.cooldowns.remaining_or_stamp(key, window, event_id)
    }
}

enum BlockReason {
    Permission {
        required: PermissionRung,
        resolved: PermissionRung,
    },
    Cooldown {
        remaining: Duration,
    },
}

impl BlockReason {
    fn into_payload(self, instance: &TriggerInstance) -> serde_json::Value {
        match self {
            Self::Permission { required, resolved } => json!({
                "instance_id": instance.id,
                "kind_id": instance.kind_id,
                "reason": "permission",
                "rung_required": required.as_str(),
                "rung_resolved": resolved.as_str(),
            }),
            Self::Cooldown { remaining } => json!({
                "instance_id": instance.id,
                "kind_id": instance.kind_id,
                "reason": "cooldown",
                "remaining_ms": remaining.as_millis() as u64,
            }),
        }
    }
}

fn resolve_rung(event: &Event) -> PermissionRung {
    // The flat platform badge strings Twitch alone also puts on the event are not an authorization
    // source: reading them would make the gate silently correct for one platform and blind on the rest.
    event
        .payload
        .get(ChatPayload::KEY)
        .and_then(|v| ChatPayload::deserialize(v).ok())
        .map(|chat| PermissionRung::from_badges(&chat.badges))
        .unwrap_or_default()
}

fn arg_stack_user(args: &ArgStack) -> Option<String> {
    for key in ["user_id", "user_login", "user"] {
        if let Some(Variant::String(s)) = args.get(key)
            && !s.is_empty()
        {
            return Some(s.clone());
        }
    }
    None
}

fn command_phrase(config: &TriggerConfig) -> String {
    match config.get("phrase") {
        Some(Variant::String(s)) => s.clone(),
        _ => String::new(),
    }
}

fn scope_matches(instance: &forge_types::TriggerInstance, event: &forge_events::Event) -> bool {
    instance
        .platform_scope
        .matches(event.source.to_platform_id())
}

pub fn spawn_trigger_evaluator(
    bus: Arc<EventBus>,
    registry: Arc<TriggerRegistry>,
    actions: Arc<dyn ActionRepo>,
    trigger_instances: Arc<dyn TriggerInstanceRepo>,
    scheduler: QueueSchedulerHandle,
    config: Config,
) -> TriggerEvaluatorHandle {
    TriggerEvaluator::spawn(bus, registry, actions, trigger_instances, scheduler, config)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_events::{Event, EventSource};
    use forge_registry::{SubActionRegistry, TriggerRegistry};
    use forge_storage::{
        ActionRepo, DataProvider, GlobalsRepo, SettingsRepo, TriggerInstanceRepo, UserGlobalsRepo,
    };
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{
        Action, ActionId, PlatformId, PlatformScope, Queue, QueueId, SubActionStep,
        TriggerInstance, TriggerInstanceId,
    };
    use serde_json::json;

    use super::*;
    use crate::{
        EventBus, EventSubscription, NullEventLogRepo, QueueScheduler, ScriptRegistry,
        sub_action_runners::register_core_sub_actions, triggers::register_core_triggers,
    };

    async fn make_backend() -> Arc<SqliteBackend> {
        Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        )
    }

    fn log_action(id: ActionId, queue_id: QueueId) -> Action {
        Action {
            id,
            name: "test-action".to_string(),
            group: None,
            queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![SubActionStep {
                kind_id: "core.log.write".to_owned(),
                config: {
                    let mut c = std::collections::BTreeMap::new();
                    c.insert(
                        "message".to_owned(),
                        forge_types::Variant::String("ok".to_owned()),
                    );
                    c
                },
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            }],
        }
    }

    fn custom_event_instance(event_name: &str) -> TriggerInstance {
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert(
            "event_name".to_owned(),
            forge_types::Variant::String(event_name.to_owned()),
        );
        TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: "script.event.custom".to_owned(),
            name: "custom".to_owned(),
            overrides,
            enabled: true,
            user_defined: true,
            platform_scope: Default::default(),
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: forge_types::PermissionRung::Everyone,
        }
    }

    async fn collect_kind(
        sub: &mut EventSubscription,
        target: &str,
        attempts: usize,
    ) -> Option<Event> {
        for _ in 0..attempts {
            match tokio::time::timeout(Duration::from_millis(300), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == target => return Some(ev),
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        None
    }

    async fn drain_no_kind(sub: &mut EventSubscription, forbidden: &str, wait_ms: u64) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(wait_ms);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(50), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == forbidden => return true,
                Ok(Ok(_)) => {}
                _ => {}
            }
        }
        false
    }

    fn build_registries(
        globals: Arc<dyn GlobalsRepo>,
        user_globals: Arc<dyn UserGlobalsRepo>,
        settings: Arc<dyn SettingsRepo>,
        trigger_instances: Arc<dyn TriggerInstanceRepo>,
        actions: Arc<dyn ActionRepo>,
    ) -> (Arc<SubActionRegistry>, Arc<TriggerRegistry>) {
        let registry = Arc::new(ScriptRegistry::new());
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let publisher: Arc<dyn forge_events::EventPublisher> =
            Arc::clone(&bus) as Arc<dyn forge_events::EventPublisher>;

        let script_repo = {
            let mut m = forge_storage::script::MockScriptRepo::new();
            m.expect_record_execution().returning(|_, _, _, _| Ok(()));
            Arc::new(m) as Arc<dyn forge_storage::ScriptRepo>
        };
        let mut sub_reg = SubActionRegistry::new();
        register_core_sub_actions(
            &mut sub_reg,
            globals,
            user_globals,
            registry,
            publisher,
            settings,
            crate::SchedulerCell::new(),
            trigger_instances,
            actions,
            script_repo,
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
            crate::OverlayServiceCell::new(),
            crate::Config::default(),
        )
        .unwrap();

        let mut trig_reg = TriggerRegistry::new();
        register_core_triggers(&mut trig_reg).unwrap();

        (Arc::new(sub_reg), Arc::new(trig_reg))
    }

    #[tokio::test]
    async fn matching_custom_event_dispatches_action() {
        let backend = make_backend().await;
        let dp: Arc<dyn DataProvider> = Arc::clone(&backend) as Arc<dyn DataProvider>;

        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            description: String::new(),
            concurrency: 8,
        };
        let action = log_action(a_id, q_id);
        let instance = custom_event_instance("my_event");

        dp.queue_repo().save(&queue).await.unwrap();
        dp.action_repo().save(&action).await.unwrap();
        dp.trigger_instance_repo().save(&instance).await.unwrap();
        backend
            .insert_action_trigger_instance_for_test(a_id, instance.id, 0)
            .await
            .unwrap();

        let (sub_reg, trig_reg) = build_registries(
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>,
            Arc::clone(&dp) as Arc<dyn SettingsRepo>,
            dp.trigger_instance_repo(),
            dp.action_repo(),
        );

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();

        let engine = crate::action_engine::spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            sub_reg,
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let _handle = spawn_trigger_evaluator(
            Arc::clone(&bus),
            trig_reg,
            dp.action_repo(),
            dp.trigger_instance_repo(),
            sched,
            Config::default(),
        );

        tokio::time::sleep(Duration::from_millis(10)).await;
        bus.publish(Event::new(
            EventSource::Server,
            "custom.my_event",
            json!({ "user": "alice" }),
        ));

        let done = collect_kind(&mut sub, "action.done", 30).await;
        assert!(
            done.is_some(),
            "action.done expected for matching custom event"
        );
    }

    #[tokio::test]
    async fn non_matching_custom_event_does_not_dispatch() {
        let backend = make_backend().await;
        let dp: Arc<dyn DataProvider> = Arc::clone(&backend) as Arc<dyn DataProvider>;

        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            description: String::new(),
            concurrency: 8,
        };
        let action = log_action(a_id, q_id);
        let instance = custom_event_instance("my_event");

        dp.queue_repo().save(&queue).await.unwrap();
        dp.action_repo().save(&action).await.unwrap();
        dp.trigger_instance_repo().save(&instance).await.unwrap();
        backend
            .insert_action_trigger_instance_for_test(a_id, instance.id, 0)
            .await
            .unwrap();

        let (sub_reg, trig_reg) = build_registries(
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>,
            Arc::clone(&dp) as Arc<dyn SettingsRepo>,
            dp.trigger_instance_repo(),
            dp.action_repo(),
        );

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();

        let engine = crate::action_engine::spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            sub_reg,
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let _handle = spawn_trigger_evaluator(
            Arc::clone(&bus),
            trig_reg,
            dp.action_repo(),
            dp.trigger_instance_repo(),
            sched,
            Config::default(),
        );

        tokio::time::sleep(Duration::from_millis(10)).await;
        bus.publish(Event::new(
            EventSource::Server,
            "custom.other_event",
            json!({}),
        ));

        let fired = drain_no_kind(&mut sub, "action.done", 300).await;
        assert!(
            !fired,
            "action.done must not fire for non-matching event name"
        );
    }

    fn scoped_instance(scope: PlatformScope) -> TriggerInstance {
        TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: "script.event.custom".to_owned(),
            name: "scoped".to_owned(),
            overrides: std::collections::BTreeMap::new(),
            enabled: true,
            user_defined: true,
            platform_scope: scope,
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: forge_types::PermissionRung::Everyone,
        }
    }

    #[test]
    fn any_scope_matches_every_source() {
        let instance = scoped_instance(PlatformScope::Any);
        for src in [EventSource::Twitch, EventSource::Core, EventSource::YouTube] {
            let event = Event::new(src, "x", json!({}));
            assert!(scope_matches(&instance, &event), "Any failed for {src:?}");
        }
    }

    #[test]
    fn only_scope_matches_listed_and_rejects_others() {
        let mut set = std::collections::BTreeSet::new();
        set.insert(PlatformId::Twitch);
        set.insert(PlatformId::YouTube);
        let instance = scoped_instance(PlatformScope::only(set).unwrap());

        for src in [EventSource::Twitch, EventSource::YouTube] {
            assert!(scope_matches(&instance, &Event::new(src, "x", json!({}))));
        }
        for src in [EventSource::Kick, EventSource::Core] {
            assert!(!scope_matches(&instance, &Event::new(src, "x", json!({}))));
        }
    }
}
