use forge_types::Variant;

use crate::config;
use crate::descriptor::OverlayConfig;

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

/// Tags an announcement or a show with the token that joins a show to its speech on the page.
pub fn joined_to_show(mut content: OverlayConfig, show: &str) -> OverlayConfig {
    content.insert(config::SHOW.to_owned(), Variant::String(show.to_owned()));
    content
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
