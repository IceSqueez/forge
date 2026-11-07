use std::sync::Arc;

use forge_events::{EventSource, EventsError};
use forge_obs::vocabulary::event_kinds::STREAMING_STARTED;
use forge_platform_youtube::BroadcastLookupWake;
use forge_runtime::EventBus;
use tokio::task::JoinHandle;

pub(crate) fn spawn_obs_stream_wake(
    bus: &Arc<EventBus>,
    wake: BroadcastLookupWake,
) -> JoinHandle<()> {
    let mut events = bus.subscribe();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event)
                    if event.source == EventSource::Obs
                        && event.kind == STREAMING_STARTED
                        && !event.replay =>
                {
                    wake.wake();
                }
                Ok(_) | Err(EventsError::LaggingReceiver) => {}
                Err(EventsError::BusClosed) => break,
            }
        }
    })
}
