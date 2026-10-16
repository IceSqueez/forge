use std::collections::BTreeMap;
use std::sync::Arc;

use forge_storage::{SoundboardClipsRepo, StorageError, StoredClip};
use forge_types::ClipId;

use crate::combo::HotkeyCombo;

pub const CLIP_COMBO_FIELD: &str = "hotkey";

pub type ClipBindings = BTreeMap<String, Vec<ClipId>>;

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
) -> Result<(), String> {
    let Some(mut clip) = repo.get(id).await.map_err(|e| e.to_string())? else {
        return Ok(());
    };
    clip.hotkey = None;
    repo.save(&clip).await.map_err(|e| e.to_string())
}

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
