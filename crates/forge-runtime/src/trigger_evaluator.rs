use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource, EventsError};
use forge_registry::{
    CancelSignal, ChatTriggerFamily, TriggerKindDescriptor, TriggerRegistry, effective_config,
};
use forge_storage::{ActionRepo, TriggerInstanceRepo};
use forge_types::{
    ArgStack, ChatPayload, EventId, PermissionRung, SynthesisHint, TriggerConfig, TriggerInstance,
    TriggerInstanceId, Variant,
};
use serde::Deserialize;
use serde_json::json;
use tracing::{Level, debug, enabled, trace, warn};

use crate::cooldown::CooldownMap;
use crate::event_log_bridge::identity_digest;
use crate::{Config, EventBus, EventSubscription, QueueSchedulerHandle, SchedulerRequest};

/// Sibling of `forge::event`, so a reproduction can raise the evaluator's decisions alone.
const DECISION_TARGET: &str = "forge::trigger";

/// The one target carrying viewer-authored text; TRACE-only, so it is raised on its own or not at all.
pub const COMMAND_LINE_TARGET: &str = "forge::command";

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
        self.drain_backlog().await;
    }

    /// Events published before the cancel still dispatch; only what arrives after it is dropped.
    async fn drain_backlog(&mut self) {
        loop {
            match self.subscription.try_recv() {
                Ok(Some(event)) => self.handle(event).await,
                Ok(None) => return,
                Err(EventsError::LaggingReceiver) => continue,
                Err(_) => return,
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

        let chat_family = descriptor.chat_trigger_family();
        let is_command = chat_family == Some(ChatTriggerFamily::Command);

        if !scope_matches(instance, event) {
            log_rejected(instance, event, is_command, Rejection::Scope);
            return None;
        }

        let effective = effective_config(&descriptor.default_config(), &instance.overrides);
        if !descriptor.matches_trigger(&effective, event) {
            log_rejected(instance, event, is_command, Rejection::Phrase);
            return None;
        }

        let args = descriptor.build_arg_stack(event);

        if is_command {
            self.bus.publish(Event::caused_by(
                EventSource::Core,
                "command.matched",
                json!({
                    "command": command_phrase(&effective),
                    "kind_id": instance.kind_id,
                }),
                event.id,
            ));
            log_command_match(instance, event, descriptor, &effective, &args);
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
                log_rejected(instance, event, is_command, Rejection::Permission);
                return None;
            }
        }

        if let Some(remaining) = self.cooldown_remaining(instance, &args, event.id) {
            self.publish_blocked(instance, event.id, BlockReason::Cooldown { remaining });
            log_rejected(instance, event, is_command, Rejection::Cooldown);
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

enum Rejection {
    Scope,
    Phrase,
    Permission,
    Cooldown,
}

impl Rejection {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Scope => "scope",
            Self::Phrase => "phrase",
            Self::Permission => "permission",
            Self::Cooldown => "cooldown",
        }
    }
}

fn log_rejected(instance: &TriggerInstance, event: &Event, is_command: bool, reason: Rejection) {
    if !is_command {
        return;
    }
    debug!(
        target: DECISION_TARGET,
        event = %event.id,
        instance = %instance.id,
        kind = %instance.kind_id,
        reason = reason.as_str(),
        "command trigger did not fire"
    );
}

/// The match verdict is the carve-out's gate: everything here is reached only once the evaluator
/// has decided this message is an invocation of the broadcaster's own configured phrase.
fn log_command_match(
    instance: &TriggerInstance,
    event: &Event,
    descriptor: &dyn TriggerKindDescriptor,
    effective: &TriggerConfig,
    args: &ArgStack,
) {
    let debug_wanted = enabled!(target: DECISION_TARGET, Level::DEBUG);
    let line_wanted = enabled!(target: COMMAND_LINE_TARGET, Level::TRACE);
    if !debug_wanted && !line_wanted {
        return;
    }

    let phrase = command_phrase(effective);
    let line = matched_line(descriptor, args);

    if debug_wanted {
        let tail = argument_tail(&line, &phrase);
        let viewer = viewer_digest(event);
        debug!(
            target: DECISION_TARGET,
            event = %event.id,
            instance = %instance.id,
            kind = %instance.kind_id,
            phrase = %phrase,
            viewer = viewer.as_deref(),
            arg_count = tail.split_whitespace().count(),
            arg_len = tail.chars().count(),
            "command matched"
        );
    }

    if line_wanted {
        trace!(
            target: COMMAND_LINE_TARGET,
            event = %event.id,
            instance = %instance.id,
            line = %line,
            "matched command line"
        );
    }
}

/// The descriptor declares which of its arg-stack variables carries the message, so the evaluator
/// reads the matched line without knowing any platform's payload shape.
fn matched_line(descriptor: &dyn TriggerKindDescriptor, args: &ArgStack) -> String {
    descriptor
        .output_schema()
        .into_iter()
        .flat_map(|schema| schema.variables)
        .find(|variable| variable.synthesis == Some(SynthesisHint::Message))
        .and_then(|variable| match args.get(&variable.name) {
            Some(Variant::String(s)) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn argument_tail<'a>(line: &'a str, phrase: &str) -> &'a str {
    let offset = line
        .char_indices()
        .nth(phrase.chars().count())
        .map_or(line.len(), |(index, _)| index);
    line[offset..].trim_start()
}

fn viewer_digest(event: &Event) -> Option<String> {
    event
        .payload
        .get(ChatPayload::KEY)
        .and_then(|v| ChatPayload::deserialize(v).ok())
        .filter(|chat| !chat.author.is_empty())
        .map(|chat| identity_digest(&chat.author))
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use forge_events::{Event, EventSource};
    use forge_registry::{
        EventFilter, FormField, KindPlatformContract, SubActionRegistry, TriggerCategory,
        TriggerRegistry,
    };
    use forge_storage::{
        ActionRepo, DataProvider, GlobalsRepo, SettingsRepo, TriggerInstanceRepo, UserGlobalsRepo,
    };
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{
        Action, ActionId, ChatSegment, DeclaredVariable, ModerationMarks, PlatformId,
        PlatformScope, Queue, QueueId, SubActionStep, TriggerInstance, TriggerInstanceId,
        UserBadge, VariableSchema, VariantKind,
    };
    use serde_json::json;
    use tracing::field::{Field, Visit};
    use tracing::span;
    use tracing::subscriber::Interest;

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

    /// One `custom.my_event` trigger instance wired to one logging action on one queue.
    struct EvaluatorFixture {
        bus: Arc<EventBus>,
        registry: Arc<TriggerRegistry>,
        actions: Arc<dyn ActionRepo>,
        trigger_instances: Arc<dyn TriggerInstanceRepo>,
        scheduler: QueueSchedulerHandle,
        _backend: Arc<SqliteBackend>,
    }

    async fn fixture(bus: Arc<EventBus>) -> EvaluatorFixture {
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

        let engine = crate::action_engine::spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            sub_reg,
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        let scheduler = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);

        EvaluatorFixture {
            bus,
            registry: trig_reg,
            actions: dp.action_repo(),
            trigger_instances: dp.trigger_instance_repo(),
            scheduler,
            _backend: backend,
        }
    }

    impl EvaluatorFixture {
        fn spawn_evaluator(&self) -> TriggerEvaluatorHandle {
            spawn_trigger_evaluator(
                Arc::clone(&self.bus),
                Arc::clone(&self.registry),
                Arc::clone(&self.actions),
                Arc::clone(&self.trigger_instances),
                self.scheduler.clone(),
                Config::default(),
            )
        }

        /// Builds the evaluator without spawning it, so a test can drive `run` to completion.
        fn evaluator(&self, subscription: EventSubscription) -> TriggerEvaluator {
            TriggerEvaluator {
                bus: Arc::clone(&self.bus),
                registry: Arc::clone(&self.registry),
                actions: Arc::clone(&self.actions),
                trigger_instances: Arc::clone(&self.trigger_instances),
                scheduler: self.scheduler.clone(),
                subscription,
                cooldowns: CooldownMap::new(Config::default().max_cooldown_entries),
            }
        }
    }

    #[tokio::test]
    async fn matching_custom_event_dispatches_action() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let fixture = fixture(Arc::clone(&bus)).await;
        let mut sub = bus.subscribe();
        let _handle = fixture.spawn_evaluator();

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
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let fixture = fixture(Arc::clone(&bus)).await;
        let mut sub = bus.subscribe();
        let _handle = fixture.spawn_evaluator();

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

    #[tokio::test]
    async fn run_dispatches_events_that_reached_the_bus_before_the_cancel() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let fixture = fixture(Arc::clone(&bus)).await;
        let mut sub = bus.subscribe();
        let evaluator = fixture.evaluator(bus.subscribe());

        // A hold's synthesized release is published, and only then does shutdown cancel intake.
        // The run loop re-reads the flag at the top, so the event is already past the gate.
        bus.publish(Event::new(
            EventSource::Server,
            "custom.my_event",
            json!({ "user": "alice" }),
        ));
        let cancel = CancelSignal::new();
        cancel.cancel();

        let finished = tokio::time::timeout(Duration::from_secs(5), evaluator.run(cancel)).await;
        assert!(
            finished.is_ok(),
            "run must return once the backlog is drained"
        );

        let done = collect_kind(&mut sub, "action.done", 30).await;
        assert!(
            done.is_some(),
            "an event published before the cancel must still reach the scheduler"
        );
    }

    #[tokio::test]
    async fn run_returns_when_the_backlog_overflowed_the_subscription() {
        let bus = EventBus::with_caps(Arc::new(NullEventLogRepo), 2, 64);
        let fixture = fixture(Arc::clone(&bus)).await;
        let evaluator = fixture.evaluator(bus.subscribe());

        for _ in 0..8 {
            bus.publish(Event::new(
                EventSource::Server,
                "custom.my_event",
                json!({}),
            ));
        }
        let cancel = CancelSignal::new();
        cancel.cancel();

        let finished = tokio::time::timeout(Duration::from_secs(5), evaluator.run(cancel)).await;
        assert!(
            finished.is_ok(),
            "a lagged subscription must advance the cursor, not spin the drain forever"
        );
    }

    const COMMAND_KIND: &str = "test.chat.command";
    const PHRASE: &str = "!go";
    const LINE: &str = "!go secretword";
    const TAIL: &str = "secretword";

    #[derive(Clone)]
    struct CapturedLine {
        target: String,
        level: Level,
        fields: BTreeMap<String, String>,
    }

    impl CapturedLine {
        fn field(&self, name: &str) -> &str {
            self.fields
                .get(name)
                .map(String::as_str)
                .unwrap_or_default()
        }

        fn mentions(&self, needle: &str) -> bool {
            self.fields.values().any(|value| value.contains(needle))
        }
    }

    #[derive(Default)]
    struct FieldCollector(BTreeMap<String, String>);

    impl Visit for FieldCollector {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.0.insert(field.name().to_owned(), format!("{value:?}"));
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            self.0.insert(field.name().to_owned(), value.to_owned());
        }
    }

    // Why: `register_callsite` answers `sometimes` on purpose: a cached `always` from another
    // capture running in parallel would hand a TRACE line to a DEBUG-only assertion.
    struct CaptureSubscriber {
        lines: Arc<Mutex<Vec<CapturedLine>>>,
        max: Level,
    }

    impl tracing::Subscriber for CaptureSubscriber {
        fn register_callsite(&self, _: &'static tracing::Metadata<'static>) -> Interest {
            Interest::sometimes()
        }

        fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
            *metadata.level() <= self.max
        }

        fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
            span::Id::from_u64(1)
        }

        fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

        fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

        fn event(&self, event: &tracing::Event<'_>) {
            let mut collector = FieldCollector::default();
            event.record(&mut collector);
            self.lines.lock().unwrap().push(CapturedLine {
                target: event.metadata().target().to_owned(),
                level: *event.metadata().level(),
                fields: collector.0,
            });
        }

        fn enter(&self, _: &span::Id) {}

        fn exit(&self, _: &span::Id) {}
    }

    // Why: the callsite interest cache is process-global while a capture subscriber is
    // thread-local. Ordinary tests in this binary reach the same production callsites with no
    // subscriber installed, which registers those callsites as `Interest::never()` - and `never`
    // short-circuits the event before `enabled()` is ever consulted, so a capture running in
    // parallel silently records nothing. This floor is installed once as the process-wide global
    // default and answers `sometimes` for every callsite, so the union can never collapse to
    // `never` and every event reaches whatever thread-local subscriber `with_default` installed.
    // It captures nothing itself.
    struct InterestFloor;

    impl tracing::Subscriber for InterestFloor {
        fn register_callsite(&self, _: &'static tracing::Metadata<'static>) -> Interest {
            Interest::sometimes()
        }

        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            false
        }

        fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
            span::Id::from_u64(1)
        }

        fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

        fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

        fn event(&self, _: &tracing::Event<'_>) {}

        fn enter(&self, _: &span::Id) {}

        fn exit(&self, _: &span::Id) {}
    }

    fn install_interest_floor() {
        static INSTALLED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        INSTALLED.get_or_init(|| {
            // A binary that already set a global default keeps it; the rebuild below still
            // clears any `never` cached before this point.
            let _ = tracing::subscriber::set_global_default(InterestFloor);
            tracing::callsite::rebuild_interest_cache();
        });
    }

    // Runs `body` on this thread with everything up to `max` captured. The decision
    // is driven synchronously precisely so the thread-local subscriber sees it.
    fn capture(max: Level, body: impl FnOnce()) -> Vec<CapturedLine> {
        let lines = Arc::new(Mutex::new(Vec::new()));
        install_interest_floor();
        tracing::subscriber::with_default(
            CaptureSubscriber {
                lines: Arc::clone(&lines),
                max,
            },
            || {
                tracing::callsite::rebuild_interest_cache();
                body();
            },
        );
        lines.lock().unwrap().clone()
    }

    fn on_target<'a>(lines: &'a [CapturedLine], target: &str) -> Vec<&'a CapturedLine> {
        lines.iter().filter(|line| line.target == target).collect()
    }

    fn message_variable(name: &str) -> DeclaredVariable {
        DeclaredVariable {
            name: name.to_owned(),
            kind: VariantKind::String,
            label: String::new(),
            synthesis: Some(SynthesisHint::Message),
        }
    }

    struct CommandDescriptor {
        family: Option<ChatTriggerFamily>,
        matches: bool,
        schema: Option<VariableSchema>,
    }

    impl CommandDescriptor {
        fn command() -> Self {
            Self {
                family: Some(ChatTriggerFamily::Command),
                matches: true,
                schema: Some(VariableSchema {
                    variables: vec![message_variable("message_text")],
                }),
            }
        }
    }

    impl TriggerKindDescriptor for CommandDescriptor {
        fn id(&self) -> &str {
            COMMAND_KIND
        }
        fn category(&self) -> TriggerCategory {
            TriggerCategory::Chat
        }
        fn label(&self) -> &str {
            "fake"
        }
        fn summary(&self) -> &str {
            ""
        }
        fn search_text(&self) -> &str {
            ""
        }
        fn icon_name(&self) -> &str {
            ""
        }
        fn platform_contract(&self) -> KindPlatformContract {
            KindPlatformContract::Universal
        }
        fn default_config(&self) -> TriggerConfig {
            let mut config = TriggerConfig::new();
            config.insert("phrase".to_owned(), Variant::String(PHRASE.to_owned()));
            config
        }
        fn config_fields(&self) -> Vec<FormField> {
            Vec::new()
        }
        fn condition_display(&self, _: &TriggerConfig) -> String {
            String::new()
        }
        fn event_filter(&self) -> EventFilter {
            EventFilter {
                source: Some(EventSource::Twitch),
                kind_prefix: Some("test.chat".to_owned()),
            }
        }
        fn matches_trigger(&self, _: &TriggerConfig, _: &Event) -> bool {
            self.matches
        }
        fn build_arg_stack(&self, event: &Event) -> ArgStack {
            let line = event
                .payload
                .get("line")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            ArgStack::new()
                .set("message_text".to_owned(), Variant::String(line.to_owned()))
                .set("user".to_owned(), Variant::String("alice".to_owned()))
        }
        fn output_schema(&self) -> Option<VariableSchema> {
            self.schema.clone()
        }
        fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
            self.family
        }
    }

    fn chat_payload(author: &str, line: &str, badges: Vec<UserBadge>) -> ChatPayload {
        ChatPayload {
            platform_msg_id: "m-1".to_owned(),
            author: author.to_owned(),
            author_color: None,
            segments: vec![ChatSegment::Text {
                text: line.to_owned(),
            }],
            badges,
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        }
    }

    fn command_event(line: &str, author: &str, badges: Vec<UserBadge>) -> Event {
        let chat = chat_payload(author, line, badges);
        Event::new(
            EventSource::Twitch,
            "test.chat.message",
            json!({
                "line": line,
                (ChatPayload::KEY): serde_json::to_value(&chat).unwrap(),
            }),
        )
    }

    fn command_instance(
        scope: PlatformScope,
        rung: PermissionRung,
        cooldown_secs: u32,
    ) -> TriggerInstance {
        TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: COMMAND_KIND.to_owned(),
            name: "cmd".to_owned(),
            overrides: BTreeMap::new(),
            enabled: true,
            user_defined: true,
            platform_scope: scope,
            cooldown_secs,
            cooldown_global: true,
            permission_rung: rung,
        }
    }

    // The fixture supplies the repos and scheduler `TriggerEvaluator` needs to exist; only the
    // registry, the bus and the cooldown map take part in a decision.
    async fn decide_harness(descriptor: CommandDescriptor) -> (EvaluatorFixture, TriggerEvaluator) {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let fixture = fixture(Arc::clone(&bus)).await;
        let mut registry = TriggerRegistry::new();
        registry.register(Box::new(descriptor)).unwrap();
        let evaluator = TriggerEvaluator {
            bus: Arc::clone(&bus),
            registry: Arc::new(registry),
            actions: Arc::clone(&fixture.actions),
            trigger_instances: Arc::clone(&fixture.trigger_instances),
            scheduler: fixture.scheduler.clone(),
            subscription: bus.subscribe(),
            cooldowns: CooldownMap::new(Config::default().max_cooldown_entries),
        };
        (fixture, evaluator)
    }

    #[tokio::test]
    async fn the_debug_command_summary_reports_the_phrase_viewer_and_argument_metadata() {
        let (_fixture, mut evaluator) = decide_harness(CommandDescriptor::command()).await;
        let instance = command_instance(PlatformScope::Any, PermissionRung::Everyone, 0);
        let event = command_event(LINE, "alice", vec![]);

        let lines = capture(Level::DEBUG, || {
            evaluator.decide(&instance, &event);
        });

        let matched = on_target(&lines, DECISION_TARGET)
            .into_iter()
            .find(|line| line.field("message") == "command matched")
            .expect("a matched command must record its DEBUG summary");
        assert_eq!(matched.field("phrase"), PHRASE);
        assert_eq!(matched.field("viewer"), identity_digest("alice"));
        assert_eq!(matched.field("arg_count"), "1");
        assert_eq!(matched.field("arg_len"), TAIL.chars().count().to_string());
    }

    #[tokio::test]
    async fn the_argument_tail_never_reaches_a_debug_field() {
        let (_fixture, mut evaluator) = decide_harness(CommandDescriptor::command()).await;
        let instance = command_instance(PlatformScope::Any, PermissionRung::Everyone, 0);
        let event = command_event(LINE, "alice", vec![]);

        let lines = capture(Level::DEBUG, || {
            evaluator.decide(&instance, &event);
        });

        assert!(
            !lines.iter().any(|line| line.mentions(TAIL)),
            "DEBUG carries argument metadata only - the tail is the RFC's explicit prohibition"
        );
        assert!(
            on_target(&lines, COMMAND_LINE_TARGET).is_empty(),
            "the content target must stay silent below TRACE"
        );
    }

    #[tokio::test]
    async fn the_full_command_line_is_raised_only_on_the_command_target_at_trace() {
        let (_fixture, mut evaluator) = decide_harness(CommandDescriptor::command()).await;
        let instance = command_instance(PlatformScope::Any, PermissionRung::Everyone, 0);
        let event = command_event(LINE, "alice", vec![]);

        let lines = capture(Level::TRACE, || {
            evaluator.decide(&instance, &event);
        });

        let raised = on_target(&lines, COMMAND_LINE_TARGET);
        assert_eq!(
            raised.len(),
            1,
            "the content line is single-sited so a bundle without the directive provably lacks it"
        );
        assert_eq!(raised[0].level, Level::TRACE);
        assert_eq!(raised[0].field("line"), LINE);
        assert!(
            !on_target(&lines, DECISION_TARGET)
                .iter()
                .any(|line| line.mentions(TAIL)),
            "raising the content target must not spill the tail onto the decision target"
        );
    }

    #[tokio::test]
    async fn a_refused_command_names_its_decision_point_and_stays_content_free() {
        struct Case {
            reason: &'static str,
            matches: bool,
            scope: PlatformScope,
            rung: PermissionRung,
            cooldown_secs: u32,
            invocations: usize,
        }

        let kick_only = PlatformScope::only(BTreeSet::from([PlatformId::Kick])).unwrap();
        for case in [
            Case {
                reason: "scope",
                matches: true,
                scope: kick_only,
                rung: PermissionRung::Everyone,
                cooldown_secs: 0,
                invocations: 1,
            },
            Case {
                reason: "phrase",
                matches: false,
                scope: PlatformScope::Any,
                rung: PermissionRung::Everyone,
                cooldown_secs: 0,
                invocations: 1,
            },
            Case {
                reason: "permission",
                matches: true,
                scope: PlatformScope::Any,
                rung: PermissionRung::Moderator,
                cooldown_secs: 0,
                invocations: 1,
            },
            Case {
                reason: "cooldown",
                matches: true,
                scope: PlatformScope::Any,
                rung: PermissionRung::Everyone,
                cooldown_secs: 30,
                invocations: 2,
            },
        ] {
            let descriptor = CommandDescriptor {
                matches: case.matches,
                ..CommandDescriptor::command()
            };
            let (_fixture, mut evaluator) = decide_harness(descriptor).await;
            let instance = command_instance(case.scope, case.rung, case.cooldown_secs);
            for _ in 1..case.invocations {
                evaluator.decide(&instance, &command_event(LINE, "alice", vec![]));
            }

            let event = command_event(LINE, "alice", vec![]);
            let lines = capture(Level::DEBUG, || {
                evaluator.decide(&instance, &event);
            });

            let refusal = on_target(&lines, DECISION_TARGET)
                .into_iter()
                .find(|line| line.field("message") == "command trigger did not fire")
                .unwrap_or_else(|| panic!("no decision line for a {} refusal", case.reason));
            assert_eq!(refusal.field("reason"), case.reason);
            assert_eq!(refusal.field("kind"), COMMAND_KIND);
            assert!(
                !lines.iter().any(|line| line.mentions(TAIL)),
                "the \"my command didn't fire\" trail is served content-free ({})",
                case.reason
            );
        }
    }

    #[tokio::test]
    async fn a_trigger_outside_the_command_family_records_no_decision_line() {
        for family in [Some(ChatTriggerFamily::Message), None] {
            let descriptor = CommandDescriptor {
                family,
                matches: false,
                ..CommandDescriptor::command()
            };
            let (_fixture, mut evaluator) = decide_harness(descriptor).await;
            let instance = command_instance(PlatformScope::Any, PermissionRung::Everyone, 0);
            let event = command_event(LINE, "alice", vec![]);

            let lines = capture(Level::TRACE, || {
                evaluator.decide(&instance, &event);
            });

            assert!(
                lines.iter().all(
                    |line| line.target != DECISION_TARGET && line.target != COMMAND_LINE_TARGET
                ),
                "the carve-out is scoped by the match verdict, so {family:?} must stay silent"
            );
        }
    }

    #[test]
    fn matched_line_takes_the_first_message_hint_when_the_schema_declares_two() {
        // Why: kick's command descriptor declares `content` (the whole line) and then `args`,
        // both hinted Message. First-wins is what keeps the logged line whole.
        let descriptor = CommandDescriptor {
            schema: Some(VariableSchema {
                variables: vec![message_variable("content"), message_variable("args")],
            }),
            ..CommandDescriptor::command()
        };
        let args = ArgStack::new()
            .set("content".to_owned(), Variant::String(LINE.to_owned()))
            .set("args".to_owned(), Variant::String(TAIL.to_owned()));

        assert_eq!(matched_line(&descriptor, &args), LINE);
    }

    #[test]
    fn matched_line_is_empty_when_no_declared_message_variable_supplies_a_string() {
        let username_only = VariableSchema {
            variables: vec![DeclaredVariable {
                name: "user".to_owned(),
                kind: VariantKind::String,
                label: String::new(),
                synthesis: Some(SynthesisHint::Username),
            }],
        };
        let args = ArgStack::new()
            .set("user".to_owned(), Variant::String("alice".to_owned()))
            .set("count".to_owned(), Variant::Int(3));

        for (label, schema) in [
            ("no output schema", None),
            ("no message hint declared", Some(username_only)),
            (
                "hinted variable absent from the stack",
                Some(VariableSchema {
                    variables: vec![message_variable("message_text")],
                }),
            ),
            (
                "hinted variable is not a string",
                Some(VariableSchema {
                    variables: vec![message_variable("count")],
                }),
            ),
        ] {
            let descriptor = CommandDescriptor {
                schema,
                ..CommandDescriptor::command()
            };
            assert_eq!(matched_line(&descriptor, &args), "", "{label}");
        }
    }

    #[test]
    fn argument_tail_drops_a_phrase_length_prefix_then_the_leading_space() {
        for (line, phrase, expected) in [
            ("!go now", "!go", "now"),
            ("!go   two  words", "!go", "two  words"),
            ("!go", "!go", ""),
            ("!go", "!go-longer-than-the-line", ""),
            ("!go now", "", "!go now"),
            ("", "!go", ""),
        ] {
            assert_eq!(
                argument_tail(line, phrase),
                expected,
                "{line:?} / {phrase:?}"
            );
        }
    }

    #[test]
    fn argument_tail_cuts_a_multibyte_phrase_on_a_character_boundary() {
        // A byte-count prefix would land inside the trailing Cyrillic character and panic.
        assert_eq!(argument_tail("!привіт світ", "!привіт"), "світ");
        assert_eq!(argument_tail("!привіт", "!привіт"), "");
    }

    #[test]
    fn viewer_digest_matches_the_bridge_digest_for_the_same_author() {
        // Why: R1's correlation property. One salt per run keeps a viewer joinable across the
        // bus bridge's trail and the evaluator's; a second salt would silently break it.
        let event = command_event(LINE, "alice", vec![]);
        assert_eq!(viewer_digest(&event), Some(identity_digest("alice")));
    }

    #[test]
    fn viewer_digest_is_absent_without_a_usable_chat_author() {
        for (label, payload) in [
            ("no chat envelope", json!({ "line": LINE })),
            ("malformed chat envelope", json!({ (ChatPayload::KEY): 42 })),
            (
                "empty author",
                json!({
                    (ChatPayload::KEY): serde_json::to_value(chat_payload("", LINE, vec![])).unwrap(),
                }),
            ),
        ] {
            let event = Event::new(EventSource::Twitch, "test.chat.message", payload);
            assert!(viewer_digest(&event).is_none(), "{label}");
        }
    }
}
