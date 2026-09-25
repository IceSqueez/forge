use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource, EventsError};
use forge_hotkey::HotkeyCombo;
use forge_runtime::EventSubscription;
use forge_soundboard::SoundboardPlayer;
use forge_storage::{SoundboardClipsRepo, StorageError, StoredClip};
use forge_types::ClipId;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::hotkey_bindings::HOTKEY_PRESSED_KIND;
use crate::hotkey_sync::HotkeyReconciler;

pub const CLIP_COMBO_FIELD: &str = "hotkey";
const PRESSED_COMBO_KEY: &str = "combo";

/// Several clips on one combo is a collision already in the data; every one of them answers the press.
pub type ClipBindings = BTreeMap<String, Vec<ClipId>>;

pub fn clip_bindings_of(clips: &[StoredClip]) -> ClipBindings {
    let mut bindings = ClipBindings::new();
    for clip in clips {
        let Some(raw) = clip.hotkey.as_deref().map(str::trim) else {
            continue;
        };
        if raw.is_empty() {
            continue;
        }
        let combo = HotkeyCombo::parse(raw)
            .map(|parsed| parsed.as_str().to_owned())
            .unwrap_or_else(|_| raw.to_owned());
        bindings.entry(combo).or_default().push(clip.id);
    }
    bindings
}

/// A blank combo is stored as no binding; an unparseable one is refused on the binding field.
pub fn canonical_clip(clip: &StoredClip) -> Result<StoredClip, StorageError> {
    let mut canonical = clip.clone();
    canonical.hotkey = match clip.hotkey.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => {
            let combo = HotkeyCombo::parse(raw).map_err(|e| StorageError::ValidationFailed {
                field: CLIP_COMBO_FIELD.to_owned(),
                reason: e.to_string(),
            })?;
            Some(combo.as_str().to_owned())
        }
    };
    Ok(canonical)
}

/// Every clip write, whichever screen or service made it, ends in a reconcile run.
pub struct HotkeySyncedClipsRepo {
    inner: Arc<dyn SoundboardClipsRepo>,
    reconciler: Option<Arc<HotkeyReconciler>>,
}

impl HotkeySyncedClipsRepo {
    pub fn wrap(
        inner: Arc<dyn SoundboardClipsRepo>,
        reconciler: Option<Arc<HotkeyReconciler>>,
    ) -> Arc<dyn SoundboardClipsRepo> {
        Arc::new(Self { inner, reconciler })
    }

    async fn sync(&self) {
        if let Some(reconciler) = &self.reconciler {
            reconciler.reconcile().await;
        }
    }
}

#[async_trait]
impl SoundboardClipsRepo for HotkeySyncedClipsRepo {
    async fn list(&self) -> Result<Vec<StoredClip>, StorageError> {
        self.inner.list().await
    }

    async fn get(&self, id: ClipId) -> Result<Option<StoredClip>, StorageError> {
        self.inner.get(id).await
    }

    async fn save(&self, clip: &StoredClip) -> Result<(), StorageError> {
        let canonical = canonical_clip(clip)?;
        self.inner.save(&canonical).await?;
        self.sync().await;
        Ok(())
    }

    async fn delete(&self, id: ClipId) -> Result<bool, StorageError> {
        let deleted = self.inner.delete(id).await?;
        self.sync().await;
        Ok(deleted)
    }
}

pub fn pressed_combo(event: &Event) -> Option<&str> {
    if event.source != EventSource::Hotkey || event.kind != HOTKEY_PRESSED_KIND {
        return None;
    }
    event.payload.get(PRESSED_COMBO_KEY)?.as_str()
}

pub fn clips_bound_to(bindings: &ClipBindings, combo: &str) -> Vec<ClipId> {
    bindings.get(combo).cloned().unwrap_or_default()
}

/// Press edges only; the hotkey engine already drops auto-repeat and presses while it is off.
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

