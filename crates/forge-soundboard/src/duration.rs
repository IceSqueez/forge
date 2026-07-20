use std::path::Path;

use crate::error::SoundboardError;

/// Duration of a clip file, in seconds. Blocking (reads and parses the file
/// header) - callers on an async runtime must wrap this in `spawn_blocking`.
pub fn probe_clip_duration_secs(path: &Path) -> Result<f32, SoundboardError> {
    forge_audio::probe_duration_secs(path).map_err(SoundboardError::Audio)
}
