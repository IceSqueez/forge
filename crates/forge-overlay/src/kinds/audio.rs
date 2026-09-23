use forge_types::Variant;

use crate::assets::PageAssets;
use crate::config;
use crate::descriptor::{
    DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use crate::preview::{PreviewComposition, PreviewShape, compose};

pub const KIND_ID: &str = "overlay.audio";

const STOP: &str = "stop";
const PAUSE: &str = "pause";
const RESUME: &str = "resume";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCommand {
    Stop,
    Pause,
    Resume,
}

impl AudioCommand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stop => STOP,
            Self::Pause => PAUSE,
            Self::Resume => RESUME,
        }
    }
}

/// `clip_id` addresses later commands and is never echoed back by the page; the capability lives
/// in `clip_path` and `report_path` and is the only thing that authorizes either request.
pub struct AudioAnnouncement<'a> {
    pub clip_id: &'a str,
    pub clip_path: &'a str,
    pub report_path: &'a str,
    pub media_type: &'a str,
    pub duration_ms: u64,
}

pub fn announcement_content(announcement: &AudioAnnouncement<'_>) -> OverlayConfig {
    OverlayConfig::from([
        (
            config::CLIP_ID.to_owned(),
            Variant::String(announcement.clip_id.to_owned()),
        ),
        (
            config::CLIP_PATH.to_owned(),
            Variant::String(announcement.clip_path.to_owned()),
        ),
        (
            config::REPORT_PATH.to_owned(),
            Variant::String(announcement.report_path.to_owned()),
        ),
        (
            config::CLIP_MEDIA_TYPE.to_owned(),
            Variant::String(announcement.media_type.to_owned()),
        ),
        (
            config::CLIP_DURATION_MS.to_owned(),
            Variant::Int(
                i64::try_from(announcement.duration_ms)
                    .unwrap_or(config::CLIP_DURATION_MAX_MS)
                    .clamp(config::CLIP_DURATION_MIN_MS, config::CLIP_DURATION_MAX_MS),
            ),
        ),
    ])
}

/// A `None` clip reaches every clip the page still holds.
pub fn command_content(command: AudioCommand, clip_id: Option<&str>) -> OverlayConfig {
    OverlayConfig::from([
        (
            config::COMMAND.to_owned(),
            Variant::String(command.as_str().to_owned()),
        ),
        (
            config::CLIP_ID.to_owned(),
            Variant::String(clip_id.unwrap_or_default().to_owned()),
        ),
    ])
}

pub struct AudioOverlayKind;

impl OverlayKindDescriptor for AudioOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> &str {
        "Audio"
    }

    fn summary(&self) -> &str {
        "Plays what forge says and the clips it fires, drawing nothing on the canvas"
    }

    fn icon_name(&self) -> &str {
        "volume"
    }

    /// An announcement spends its capability when the page fetches it, so replaying the last one
    /// on reconnect would announce a clip that no longer exists.
    fn delivery_disposition(&self) -> DeliveryDisposition {
        DeliveryDisposition::Transient
    }

    /// A command that overtakes the announcement it addresses reaches a clip the page has not met.
    fn order_sensitive(&self) -> bool {
        true
    }

    fn config_schema_version(&self) -> u32 {
        1
    }

    fn default_config(&self) -> OverlayConfig {
        config::audio_content_defaults()
    }

    fn config_fields(&self) -> Vec<SectionedField> {
        config::audio_content_fields()
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/audio/index.html"),
            style: include_str!("../../assets/audio/overlay.css"),
            behavior: include_str!("../../assets/audio/overlay.js"),
        }
    }

    fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
        compose(PreviewShape::AudioPlayer, config)
    }

    fn has_visual_page(&self) -> bool {
        false
    }

    fn content_is_machine_filled(&self) -> bool {
        true
    }
}
