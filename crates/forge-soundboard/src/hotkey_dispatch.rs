use std::sync::Arc;

use forge_events::{Event, EventSource, EventsError};
use forge_hotkey::payload_fields::COMBO;
use forge_hotkey::{ClipBindings, event_kinds};
use forge_runtime::EventSubscription;
use forge_types::ClipId;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::player::SoundboardPlayer;

pub fn pressed_combo(event: &Event) -> Option<&str> {
    if event.source != EventSource::Hotkey || event.kind != event_kinds::GLOBAL_PRESSED {
        return None;
    }
    event.payload.get(COMBO)?.as_str()
}

pub fn clips_bound_to(bindings: &ClipBindings, combo: &str) -> Vec<ClipId> {
    bindings.get(combo).cloned().unwrap_or_default()
}

pub fn spawn_clip_hotkey_dispatcher(
    mut events: EventSubscription,
    bindings: watch::Receiver<Arc<ClipBindings>>,
    player: Arc<SoundboardPlayer>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                Err(EventsError::LaggingReceiver) => {
                    tracing::warn!("clip hotkey dispatcher lagged; some presses were dropped");
                    continue;
                }
                Err(_) => break,
            };
            let Some(combo) = pressed_combo(&event) else {
                continue;
            };
            let clips = clips_bound_to(&bindings.borrow(), combo);
            for clip_id in clips {
                let player = Arc::clone(&player);
                tokio::spawn(async move {
                    if let Err(e) = player.toggle(clip_id, None).await {
                        tracing::warn!(clip_id = %clip_id, error = %e, "clip hotkey could not toggle the clip");
                    }
                });
            }
        }
    })
}
