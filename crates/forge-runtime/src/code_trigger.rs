use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use forge_events::{Event, EventSource};
use forge_storage::DataProvider;
use forge_types::{ArgStack, TriggerKind, Variant};
use tracing::warn;

use crate::{EventBus, EventSubscription, QueueSchedulerHandle, SchedulerRequest};

#[derive(Clone)]
pub struct CodeTriggerHandle {
    cancel: Arc<AtomicBool>,
}

impl CodeTriggerHandle {
    pub fn shutdown(self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub struct CodeTriggerEvaluator {
    dp: Arc<dyn DataProvider>,
    scheduler: QueueSchedulerHandle,
    subscription: EventSubscription,
}

impl CodeTriggerEvaluator {
    pub fn spawn(
        bus: Arc<EventBus>,
        dp: Arc<dyn DataProvider>,
        scheduler: QueueSchedulerHandle,
    ) -> CodeTriggerHandle {
        let subscription = bus.subscribe();
        let evaluator = Self {
            dp,
            scheduler,
            subscription,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel);
        tokio::spawn(async move { evaluator.run(cancel_clone).await });
        CodeTriggerHandle { cancel }
    }

    async fn run(mut self, cancel: Arc<AtomicBool>) {
        while !cancel.load(Ordering::Relaxed) {
            match self.subscription.recv().await {
                Ok(event)
                    if event.source == EventSource::Server && event.kind.starts_with("custom.") =>
                {
                    self.handle_code_event(event).await;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    }

    async fn handle_code_event(&mut self, event: Event) {
        let name = &event.kind["custom.".len()..];
        let event_id = event.id;

        let actions = match self.dp.action_repo().list().await {
            Ok(a) => a,
            Err(e) => {
                warn!("code_trigger: action_repo.list failed: {e}");
                return;
            }
        };

        for action in &actions {
            if !action.enabled {
                continue;
            }

            let triggers = match self.dp.trigger_repo().list_for_action(action.id).await {
                Ok(t) => t,
                Err(e) => {
                    warn!("code_trigger: trigger_repo.list_for_action failed: {e}");
                    continue;
                }
            };

            for trigger in &triggers {
                let TriggerKind::CodeEvent {
                    name: ref trigger_name,
                } = trigger.kind
                else {
                    continue;
                };
                if trigger_name != name {
                    continue;
                }
                let args = build_args_from_payload(&event.payload);
                let req = SchedulerRequest {
                    queue_id: action.queue_id,
                    action_id: action.id,
                    trigger_event_id: event_id,
                    initial_args: args,
                    bypass_pause: action.bypass_pause,
                };
                if let Err(e) = self.scheduler.dispatch(req).await {
                    warn!("code_trigger: scheduler dispatch failed: {e}");
                }
            }
        }
    }
}

fn build_args_from_payload(payload: &serde_json::Value) -> ArgStack {
    let Some(obj) = payload.as_object() else {
        return ArgStack::new();
    };
    obj.iter()
        .filter_map(|(k, v)| Variant::from_json(v.clone()).ok().map(|vv| (k.clone(), vv)))
        .fold(ArgStack::new(), |stack, (k, v)| stack.set(k, v))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    use forge_events::{Event, EventSource};
    use forge_storage::DataProvider;
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{
        Action, ActionId, LogLevel, Queue, QueueId, SubActionSpec, Trigger, TriggerId, TriggerKind,
    };
    use serde_json::json;

    use super::*;
    use crate::{
        EventBus, EventSubscription, NullEventLogRepo, QueueScheduler, ScriptRegistry,
        spawn_action_engine,
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
            description: None,
            sub_actions: vec![SubActionSpec::Log {
                level: LogLevel::Info,
                message: "ok".to_string(),
            }],
        }
    }

    fn code_event_trigger(action_id: ActionId, name: &str) -> Trigger {
        Trigger {
            id: TriggerId::new(),
            action_id,
            kind: TriggerKind::CodeEvent {
                name: name.to_string(),
            },
            config: BTreeMap::new(),
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

    #[tokio::test]
    async fn matching_name_dispatches_action() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let trigger = code_event_trigger(a_id, "my_event");

        dp.queue_repo().save(&queue).await.unwrap();
        dp.action_repo().save(&action).await.unwrap();
        dp.trigger_repo().save(&trigger).await.unwrap();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&dp),
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let _handle = CodeTriggerEvaluator::spawn(Arc::clone(&bus), Arc::clone(&dp), sched);

        tokio::time::sleep(Duration::from_millis(10)).await;
        bus.publish(Event::new(
            EventSource::Server,
            "custom.my_event",
            json!({ "user": "alice" }),
        ));

        let done = collect_kind(&mut sub, "action.done", 30).await;
        assert!(
            done.is_some(),
            "action.done expected for matching code event"
        );
    }

    #[tokio::test]
    async fn non_matching_name_does_not_dispatch() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let trigger = code_event_trigger(a_id, "my_event");

        dp.queue_repo().save(&queue).await.unwrap();
        dp.action_repo().save(&action).await.unwrap();
        dp.trigger_repo().save(&trigger).await.unwrap();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&dp),
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let _handle = CodeTriggerEvaluator::spawn(Arc::clone(&bus), Arc::clone(&dp), sched);

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
    async fn non_server_source_ignored() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let trigger = code_event_trigger(a_id, "my_event");

        dp.queue_repo().save(&queue).await.unwrap();
        dp.action_repo().save(&action).await.unwrap();
        dp.trigger_repo().save(&trigger).await.unwrap();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&dp),
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let _handle = CodeTriggerEvaluator::spawn(Arc::clone(&bus), Arc::clone(&dp), sched);

        tokio::time::sleep(Duration::from_millis(10)).await;
        bus.publish(Event::new(EventSource::Rhai, "custom.my_event", json!({})));

        let fired = drain_no_kind(&mut sub, "action.done", 300).await;
        assert!(!fired, "action.done must not fire for non-Server source");
    }
}
