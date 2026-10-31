use std::collections::BTreeMap;
use std::sync::Arc;

use forge_storage::{SoundboardClipsRepo, StorageError, StoredClip};
use forge_types::ClipId;
use tokio::sync::watch;

use crate::combo::HotkeyCombo;
use crate::error::HotkeyError;

pub const CLIP_COMBO_FIELD: &str = "hotkey";

pub type ClipBindings = BTreeMap<String, Vec<ClipId>>;

pub trait ClipBindingsSource: Send + 'static {
    fn current(&self) -> Arc<ClipBindings>;
}

#[derive(Debug, Clone)]
pub struct ClipBindingsWatch(watch::Receiver<Arc<ClipBindings>>);

impl ClipBindingsWatch {
    pub(crate) fn new(receiver: watch::Receiver<Arc<ClipBindings>>) -> Self {
        Self(receiver)
    }
}

impl ClipBindingsSource for ClipBindingsWatch {
    fn current(&self) -> Arc<ClipBindings> {
        Arc::clone(&self.0.borrow())
    }
}

pub fn clip_bindings_of(clips: &[StoredClip]) -> ClipBindings {
    let mut bindings = ClipBindings::new();
    for clip in clips {
        if let Some(combo) = stored_combo(clip) {
            bindings.entry(combo).or_default().push(clip.id);
        }
    }
    bindings
}

pub fn stored_combo(clip: &StoredClip) -> Option<String> {
    let raw = clip.hotkey.as_deref().map(str::trim)?;
    if raw.is_empty() {
        return None;
    }
    Some(
        HotkeyCombo::parse(raw)
            .map(|parsed| parsed.as_str().to_owned())
            .unwrap_or_else(|_| raw.to_owned()),
    )
}

pub async fn clear_clip_combo(
    repo: Arc<dyn SoundboardClipsRepo>,
    id: ClipId,
) -> Result<(), HotkeyError> {
    let Some(mut clip) = repo.get(id).await? else {
        return Ok(());
    };
    clip.hotkey = None;
    repo.save(&clip).await.map_err(HotkeyError::from)
}

pub(crate) fn canonical_clip(clip: &StoredClip) -> Result<StoredClip, StorageError> {
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

#[cfg(test)]
mod tests {
    use forge_types::OutputDevice;
    use time::OffsetDateTime;

    use super::*;

    fn clip(hotkey: Option<&str>) -> StoredClip {
        StoredClip {
            id: ClipId::new(),
            name: "airhorn".to_owned(),
            file_path: "/clips/airhorn.wav".into(),
            volume: 1.0,
            output_device: OutputDevice::Default,
            hotkey: hotkey.map(str::to_owned),
            created_at: OffsetDateTime::now_utc(),
            category: String::new(),
            loop_playback: false,
            duration_secs: None,
            builtin_id: None,
        }
    }

    #[test]
    fn clip_bindings_group_every_clip_on_a_combo_under_its_canonical_spelling() {
        let typed = clip(Some("f9"));
        let canonical = clip(Some("F9"));
        let clips = [
            typed.clone(),
            canonical.clone(),
            clip(None),
            clip(Some("  ")),
        ];

        let bindings = clip_bindings_of(&clips);

        assert_eq!(
            bindings,
            ClipBindings::from([("F9".to_owned(), vec![typed.id, canonical.id])])
        );
    }
}
