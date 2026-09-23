use forge_events::{Event, EventSource};
use forge_runtime::{EventBus, QueueSchedulerHandle, SchedulerRequest};
use forge_types::{ActionId, ArgStack, QueueId};
use serde_json::json;

pub(super) async fn dispatch_test_run(
    scheduler: &QueueSchedulerHandle,
    bus: &EventBus,
    action_id: ActionId,
    queue_id: QueueId,
    bypass_pause: bool,
    trigger_kind: Option<String>,
    initial_args: ArgStack,
) -> Result<(), String> {
    let root = Event::new(
        EventSource::Core,
        "test.run",
        json!({ "action_id": action_id.to_string() }),
    );
    let trigger_event_id = root.id;
    bus.record(root);
    scheduler
        .dispatch(SchedulerRequest {
            queue_id,
            action_id,
            trigger_event_id,
            trigger_kind,
            initial_args,
            bypass_pause,
        })
        .await
        .map_err(|e| e.to_string())
}
