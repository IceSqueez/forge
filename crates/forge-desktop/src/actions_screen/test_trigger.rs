use forge_events::{EventSource, ManualTestRun, TypedEvent};
use forge_runtime::{EventBus, QueueAdmission, QueueSchedulerHandle, SchedulerRequest};
use forge_types::{ActionId, ArgStack, QueueId};

pub(super) async fn dispatch_test_run(
    scheduler: &QueueSchedulerHandle,
    bus: &EventBus,
    action_id: ActionId,
    queue_id: QueueId,
    admission: QueueAdmission,
    trigger_kind: Option<String>,
    initial_args: ArgStack,
) -> Result<(), String> {
    let root = ManualTestRun { action_id }.into_event(EventSource::Core, None);
    let trigger_event_id = root.id;
    bus.record(root);
    scheduler
        .dispatch(SchedulerRequest {
            queue_id,
            action_id,
            trigger_event_id,
            trigger_kind,
            manual_test: true,
            initial_args,
            admission,
        })
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_registry::SubActionRegistry;
    use forge_runtime::{ActionCancelRegistry, Catalog, QueueScheduler, spawn_action_engine};

    use super::*;
    use crate::test_support::{StubEventLog, runtime, sandboxed_provider, seed_action};

    const RUN_SETTLE: Duration = Duration::from_secs(30);

    #[test]
    fn test_run_dispatch_marks_the_finished_run_as_a_manual_test() {
        let rt = runtime();
        let manual_test = rt.block_on(async {
            let provider = sandboxed_provider().await;
            let action_id = seed_action(&provider, "Scene", Vec::new()).await;
            let queue = provider
                .queue_repo()
                .get(forge_types::DEFAULT_QUEUE_ID)
                .await
                .unwrap()
                .expect("migrations seed the default queue");
            let queue_id = queue.id;
            let bus = EventBus::new(Arc::new(StubEventLog));
            let engine = spawn_action_engine(
                Arc::clone(&bus),
                Catalog::from_provider(provider.as_ref()),
                provider.action_repo(),
                provider.history_repo(),
                Arc::new(SubActionRegistry::new()),
                Arc::new(ActionCancelRegistry::new()),
            );
            let scheduler = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
            let mut sub = bus.subscribe();

            dispatch_test_run(
                &scheduler,
                &bus,
                action_id,
                queue_id,
                QueueAdmission::default(),
                None,
                ArgStack::new(),
            )
            .await
            .unwrap();

            tokio::time::timeout(RUN_SETTLE, async {
                loop {
                    let event = sub.recv().await.expect("the bus stays open");
                    if event.kind == "action.done" {
                        return event.payload["manual_test"].clone();
                    }
                }
            })
            .await
            .expect("the engine finishes the test run")
        });

        assert_eq!(manual_test, serde_json::Value::Bool(true));
    }
}
