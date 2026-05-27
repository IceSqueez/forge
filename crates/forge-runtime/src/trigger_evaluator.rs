use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use forge_registry::TriggerRegistry;
use forge_storage::{ActionRepo, TriggerRepo};
use tracing::warn;

use crate::{EventBus, EventSubscription, QueueSchedulerHandle, SchedulerRequest};

#[derive(Clone)]
pub struct TriggerEvaluatorHandle {
    cancel: Arc<AtomicBool>,
}

impl TriggerEvaluatorHandle {
    pub fn shutdown(self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub struct TriggerEvaluator {
    registry: Arc<TriggerRegistry>,
    actions: Arc<dyn ActionRepo>,
    triggers: Arc<dyn TriggerRepo>,
    scheduler: QueueSchedulerHandle,
    subscription: EventSubscription,
}

impl TriggerEvaluator {
    pub fn spawn(
        bus: Arc<EventBus>,
        registry: Arc<TriggerRegistry>,
        actions: Arc<dyn ActionRepo>,
        triggers: Arc<dyn TriggerRepo>,
        scheduler: QueueSchedulerHandle,
    ) -> TriggerEvaluatorHandle {
        let subscription = bus.subscribe();
        let evaluator = Self {
            registry,
            actions,
            triggers,
            scheduler,
            subscription,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel);
        tokio::spawn(async move { evaluator.run(cancel_clone).await });
        TriggerEvaluatorHandle { cancel }
    }

    async fn run(mut self, cancel: Arc<AtomicBool>) {
        while !cancel.load(Ordering::Relaxed) {
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

        for action in &actions {
            if !action.enabled {
                continue;
            }

            let trigger_rows = match self.triggers.list_for_action(action.id).await {
                Ok(t) => t,
                Err(e) => {
                    warn!("trigger_evaluator: trigger_repo.list_for_action failed: {e}");
                    continue;
                }
            };

            for trigger in &trigger_rows {
                let descriptor = match self.registry.get(&trigger.kind_id) {
                    Some(d) => d,
                    None => {
                        warn!(
                            "unknown trigger kind_id: {} — trigger will never fire",
                            trigger.kind_id
                        );
                        continue;
                    }
                };

                let filter = descriptor.event_filter();
                let source_ok = filter.source.is_none_or(|s| s == event.source);
                let prefix_ok = filter
                    .kind_prefix
                    .as_deref()
                    .is_none_or(|p| event.kind.starts_with(p));

                if !source_ok || !prefix_ok {
                    continue;
                }

                if !descriptor.matches_trigger(trigger, &event) {
                    continue;
                }

                let args = descriptor.build_arg_stack(&event);
                let req = SchedulerRequest {
                    queue_id: action.queue_id,
                    action_id: action.id,
                    trigger_event_id: event.id,
                    initial_args: args,
                    bypass_pause: action.bypass_pause,
                };
                if let Err(e) = self.scheduler.dispatch(req).await {
                    warn!("trigger_evaluator: scheduler dispatch failed: {e}");
                }
            }
        }
    }
}

pub fn spawn_trigger_evaluator(
    bus: Arc<EventBus>,
    registry: Arc<TriggerRegistry>,
    actions: Arc<dyn ActionRepo>,
    triggers: Arc<dyn TriggerRepo>,
    scheduler: QueueSchedulerHandle,
) -> TriggerEvaluatorHandle {
    TriggerEvaluator::spawn(bus, registry, actions, triggers, scheduler)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_events::{Event, EventSource};
    use forge_registry::{SubActionRegistry, TriggerRegistry};
    use forge_storage::{DataProvider, GlobalsRepo};
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{Action, ActionId, Queue, QueueId, SubActionStep, Trigger, TriggerId};
    use serde_json::json;

    use super::*;
    use crate::{
        EventBus, EventSubscription, NullEventLogRepo, QueueScheduler, ScriptRegistry,
        sub_action_runners::register_core_sub_actions, triggers::register_core_triggers,
    };

    async fn make_dp() -> Arc<dyn DataProvider> {
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
                label: None,
            }],
        }
    }

    fn custom_event_trigger(action_id: ActionId, event_name: &str) -> Trigger {
        let mut config = std::collections::BTreeMap::new();
        config.insert(
            "event_name".to_owned(),
            forge_types::Variant::String(event_name.to_owned()),
        );
        Trigger {
            id: TriggerId::new(),
            action_id,
            kind_id: "script.event.custom".to_owned(),
            config,
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
    ) -> (Arc<SubActionRegistry>, Arc<TriggerRegistry>) {
        let registry = Arc::new(ScriptRegistry::new());
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let publisher: Arc<dyn forge_events::EventPublisher> =
            Arc::clone(&bus) as Arc<dyn forge_events::EventPublisher>;

        let mut sub_reg = SubActionRegistry::new();
        register_core_sub_actions(&mut sub_reg, globals, registry, publisher).unwrap();

        let mut trig_reg = TriggerRegistry::new();
        register_core_triggers(&mut trig_reg).unwrap();

        (Arc::new(sub_reg), Arc::new(trig_reg))
    }

    #[tokio::test]
    async fn matching_custom_event_dispatches_action() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let trigger = custom_event_trigger(a_id, "my_event");

        dp.queue_repo().save(&queue).await.unwrap();
        dp.action_repo().save(&action).await.unwrap();
        dp.trigger_repo().save(&trigger).await.unwrap();

        let (sub_reg, trig_reg) = build_registries(Arc::clone(&dp) as Arc<dyn GlobalsRepo>);

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();

        let engine = crate::action_engine::spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            sub_reg,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let _handle = spawn_trigger_evaluator(
            Arc::clone(&bus),
            trig_reg,
            dp.action_repo(),
            dp.trigger_repo(),
            sched,
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
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let trigger = custom_event_trigger(a_id, "my_event");

        dp.queue_repo().save(&queue).await.unwrap();
        dp.action_repo().save(&action).await.unwrap();
        dp.trigger_repo().save(&trigger).await.unwrap();

        let (sub_reg, trig_reg) = build_registries(Arc::clone(&dp) as Arc<dyn GlobalsRepo>);

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();

        let engine = crate::action_engine::spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            sub_reg,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let _handle = spawn_trigger_evaluator(
            Arc::clone(&bus),
            trig_reg,
            dp.action_repo(),
            dp.trigger_repo(),
            sched,
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
}
