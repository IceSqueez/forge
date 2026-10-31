use std::path::Path;

use crate::error::SoundboardError;

pub(crate) fn probe_clip_duration_secs(path: &Path) -> Result<f32, SoundboardError> {
    forge_audio::probe_duration_secs(path).map_err(SoundboardError::Audio)
}
