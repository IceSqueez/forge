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
pub struct ObsTriggerHandle {
    cancel: Arc<AtomicBool>,
}

impl ObsTriggerHandle {
    pub fn shutdown(self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub struct ObsTriggerEvaluator {
    dp: Arc<dyn DataProvider>,
    scheduler: QueueSchedulerHandle,
    subscription: EventSubscription,
}

impl ObsTriggerEvaluator {
    pub fn spawn(
        bus: Arc<EventBus>,
        dp: Arc<dyn DataProvider>,
        scheduler: QueueSchedulerHandle,
    ) -> ObsTriggerHandle {
        let subscription = bus.subscribe();
        let evaluator = Self {
            dp,
            scheduler,
            subscription,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel);
        tokio::spawn(async move { evaluator.run(cancel_clone).await });
        ObsTriggerHandle { cancel }
    }

    async fn run(mut self, cancel: Arc<AtomicBool>) {
        while !cancel.load(Ordering::Relaxed) {
            match self.subscription.recv().await {
                Ok(event) if event.source == EventSource::Obs && event.kind == "scene.changed" => {
                    self.handle_scene_changed(event).await;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    }

    async fn handle_scene_changed(&mut self, event: Event) {
        let scene = match event.payload.get("scene").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => return,
        };
        let event_id = event.id;

        let actions = match self.dp.action_repo().list().await {
            Ok(a) => a,
            Err(e) => {
                warn!("obs_trigger: action_repo.list failed: {e}");
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
                    warn!("obs_trigger: trigger_repo.list_for_action failed: {e}");
                    continue;
                }
            };

            for trigger in &triggers {
                let TriggerKind::ObsSceneChanged { scene: ref filter } = trigger.kind else {
                    continue;
                };
                let matches = match filter {
                    None => true,
                    Some(f) => f == &scene,
                };
                if !matches {
                    continue;
                }
                let args = ArgStack::new().set("scene".to_string(), Variant::String(scene.clone()));
                let req = SchedulerRequest {
                    queue_id: action.queue_id,
                    action_id: action.id,
                    trigger_event_id: event_id,
                    initial_args: args,
                    bypass_pause: action.bypass_pause,
                };
                if let Err(e) = self.scheduler.dispatch(req).await {
                    warn!("obs_trigger: scheduler dispatch failed: {e}");
                }
            }
        }
    }
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
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![SubActionSpec::Log {
                level: LogLevel::Info,
                message: "ok".to_string(),
            }],
        }
    }

    fn obs_trigger(action_id: ActionId, scene_filter: Option<String>) -> Trigger {
        Trigger {
            id: TriggerId::new(),
            action_id,
            kind: TriggerKind::ObsSceneChanged {
                scene: scene_filter,
            },
            config: BTreeMap::new(),
        }
    }

    fn scene_changed_event(scene: &str) -> Event {
        Event::new(EventSource::Obs, "scene.changed", json!({ "scene": scene }))
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
    async fn scene_filter_matches_exact_scene() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let trigger = obs_trigger(a_id, Some("Main".to_string()));

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
        let _handle = ObsTriggerEvaluator::spawn(Arc::clone(&bus), Arc::clone(&dp), sched);

        tokio::time::sleep(Duration::from_millis(10)).await;
        bus.publish(scene_changed_event("Main"));

        let done = collect_kind(&mut sub, "action.done", 30).await;
        assert!(done.is_some(), "action.done expected for matching scene");
    }

    #[tokio::test]
    async fn scene_filter_rejects_wrong_scene() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let trigger = obs_trigger(a_id, Some("Main".to_string()));

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
        let _handle = ObsTriggerEvaluator::spawn(Arc::clone(&bus), Arc::clone(&dp), sched);

        tokio::time::sleep(Duration::from_millis(10)).await;
        bus.publish(scene_changed_event("Gaming"));

        let fired = drain_no_kind(&mut sub, "action.done", 300).await;
        assert!(!fired, "action.done must not fire for non-matching scene");
    }

    #[tokio::test]
    async fn none_filter_matches_any_scene() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let trigger = obs_trigger(a_id, None);

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
        let _handle = ObsTriggerEvaluator::spawn(Arc::clone(&bus), Arc::clone(&dp), sched);

        tokio::time::sleep(Duration::from_millis(10)).await;
        bus.publish(scene_changed_event("AnyRandomScene"));

        let done = collect_kind(&mut sub, "action.done", 30).await;
        assert!(
            done.is_some(),
            "action.done expected when scene filter is None"
        );
    }

    #[tokio::test]
    async fn replay_and_publish_dispatches_action_and_sets_replay_flag() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = Queue {
            id: q_id,
            name: "default".into(),
            blocking: false,
        };
        let action = log_action(a_id, q_id);
        let trigger = obs_trigger(a_id, None);

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
        let _handle = ObsTriggerEvaluator::spawn(Arc::clone(&bus), Arc::clone(&dp), sched);
        tokio::time::sleep(Duration::from_millis(10)).await;

        let original = scene_changed_event("TestScene");
        let original_id = original.id;
        bus.publish(original);
        let _ = collect_kind(&mut sub, "action.done", 30).await;

        bus.replay_and_publish(original_id).await.unwrap();

        let replayed = collect_kind(&mut sub, "scene.changed", 30).await.unwrap();
        assert!(
            replayed.replay,
            "replayed scene.changed must have replay=true"
        );

        let done = collect_kind(&mut sub, "action.done", 30).await;
        assert!(
            done.is_some(),
            "action.done must fire when evaluator processes replayed event"
        );
    }
}
