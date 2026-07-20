use std::path::PathBuf;

use forge_types::{ClipId, OutputDevice};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Volume scalar in `0.0..=1.5` range. Above 1.0 is post-gain - clipping is the
/// caller's responsibility. The newtype prevents accidental swapping with raw
/// percentages.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClipVolume(f32);

impl ClipVolume {
    pub const SILENT: Self = Self(0.0);
    pub const UNITY: Self = Self(1.0);
    pub const MAX: Self = Self(1.5);

    pub fn new(v: f32) -> Self {
        Self(v.clamp(0.0, Self::MAX.0))
    }

    pub fn get(self) -> f32 {
        self.0
    }
}

impl Default for ClipVolume {
    fn default() -> Self {
        Self::UNITY
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundboardClip {
    pub id: ClipId,
    pub name: String,
    pub file_path: PathBuf,
    pub volume: ClipVolume,
    pub output_device: OutputDevice,
    pub hotkey: Option<String>,
    pub created_at: OffsetDateTime,
    /// Free-form grouping key (e.g. `"memes"`, `"alerts"`, `"music"`, `"voice"`).
    /// Empty string means uncategorized.
    pub category: String,
    pub loop_playback: bool,
    /// Probed once and cached; `None` until a probe has run for this clip.
    pub duration_secs: Option<f32>,
    /// Identifies a builtin-library slot this clip was imported from; `None` for
    /// user-added clips.
    pub builtin_id: Option<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn clip_volume_clamps_above_max() {
        let v = ClipVolume::new(3.5);
        assert_eq!(v.get(), 1.5);
    }

    #[test]
    fn clip_volume_clamps_below_zero() {
        let v = ClipVolume::new(-1.0);
        assert_eq!(v.get(), 0.0);
    }

    #[test]
    fn clip_volume_serde_roundtrip() {
        let v = ClipVolume::new(0.7);
        let json = serde_json::to_string(&v).unwrap();
        let back: ClipVolume = serde_json::from_str(&json).unwrap();
        assert!((back.get() - 0.7).abs() < 1e-6);
    }

    #[test]
    fn clip_serde_roundtrip() {
        let clip = SoundboardClip {
            id: ClipId::new(),
            name: "Air horn".to_string(),
            file_path: PathBuf::from("/tmp/airhorn.wav"),
            volume: ClipVolume::new(0.8),
            output_device: OutputDevice::Default,
            hotkey: Some("Ctrl+1".to_string()),
            created_at: OffsetDateTime::now_utc(),
            category: "memes".to_string(),
            loop_playback: false,
            duration_secs: Some(1.2),
            builtin_id: None,
        };
        let json = serde_json::to_string(&clip).unwrap();
        let back: SoundboardClip = serde_json::from_str(&json).unwrap();
        assert_eq!(clip, back);
    }
}
