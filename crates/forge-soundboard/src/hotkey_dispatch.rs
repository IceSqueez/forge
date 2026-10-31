use std::sync::Arc;

use forge_events::{Event, EventObserver, EventSource, EventsError};
use forge_hotkey::vocabulary::event_kinds;
use forge_hotkey::vocabulary::fields::COMBO;
use forge_hotkey::{ClipBindings, ClipBindingsSource};
use forge_types::ClipId;
use tokio::task::JoinHandle;

use crate::player::SoundboardPlayer;

const CLIP_HOTKEY_DISPATCHER: &str = "clip_hotkey_dispatcher";

pub fn pressed_combo(event: &Event) -> Option<&str> {
    if event.source != EventSource::Hotkey || event.kind != event_kinds::GLOBAL_PRESSED {
        return None;
    }
    event.payload.get(COMBO)?.as_str()
}

pub(crate) fn clips_bound_to(bindings: &ClipBindings, combo: &str) -> Vec<ClipId> {
    bindings.get(combo).cloned().unwrap_or_default()
}

pub fn spawn_clip_hotkey_dispatcher(
    observer: &dyn EventObserver,
    bindings: impl ClipBindingsSource,
    player: Arc<SoundboardPlayer>,
) -> JoinHandle<()> {
    let mut events = observer.observe(CLIP_HOTKEY_DISPATCHER);
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
            let clips = clips_bound_to(&bindings.current(), combo);
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
