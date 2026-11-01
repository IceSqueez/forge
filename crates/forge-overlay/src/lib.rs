mod catalog;
pub mod config;
mod error;
mod files;
pub mod kinds;
mod messages;
pub mod metrics;
pub mod motion;
mod render;

pub use catalog::descriptor::{
    ConfigSection, ContentFeed, DeliveryDisposition, OverlayConfig, OverlayKindDescriptor,
    SectionedField,
};
pub use catalog::instance::OverlayInstance;
pub use catalog::latest::{
    LatestBinding, SLOT_OPTIONS, latest_binding, latest_content, sample_latest_value,
};
pub use catalog::registry::OverlayKindRegistry;
pub use catalog::source::{read_overlay_source, write_overlay_source};
pub use catalog::source_box::{ContentMargins, DesignSize};
pub use catalog::wiring::{
    EventWiringTrigger, accepts_event_wiring, is_curated, order_curated_first, suggested_content,
};
pub use config::{effective_overlay_config, hex_color_channels, validate_overlay_config};
pub use error::OverlayError;
pub use files::assets::{
    BEHAVIOR_FILE, CONFIG_FILE, MARKUP_FILE, MOTION_ASSET, MOTION_SOURCE, OVERRIDABLE_FILES,
    PageAssets, RESERVED_DIRECTORY, RUNTIME_ASSET, RUNTIME_SOURCE, SAMPLE_FILE, STYLE_FILE,
};
pub use files::audio_transport::{
    AudioAnnouncement, AudioCommand, announcement_content, command_content, joined_to_show,
    silent_reveal_content,
};
pub use files::icons::{CURATED_ICONS, CuratedIcon, IconCategory, curated_icon};
pub use files::media::{
    CLIP_REFERENCE_PREFIX, ClipReference, EmittedIcon, GENERATED_MEDIA_DIRECTORY,
    IMAGE_REFERENCE_PREFIX, IconValue, ImageReference, MEDIA_KEYS, MediaIssue, MediaSlot,
    MediaValue, OverlayMedia, ResolvedMedia, clip_reference, clip_references, emitted_icon,
    emitted_media_value, glyph_media, image_reference, image_references, media_slot,
    read_icon_value, read_media_value,
};
pub use kinds::register_builtin_kinds;
pub use messages::OVERLAY_MESSAGES;
pub use metrics::{AxisBound, ElementSizing};
pub use render::base::{
    DEFAULT_DISPLAY_SECS, ShowTiming, SpeechProgram, display_window, exit_tail, show_timing,
    take_speech,
};
pub use render::browser_preview::{
    PREVIEW_CONNECTION_FIELD, PREVIEW_PARAM, PREVIEW_VALUE, preview_page_url,
};
pub use render::content::delivered_content;
pub use render::document::{
    ICON_FILE_FIELD, ICON_TINTABLE_FIELD, config_document, sample_document,
};
pub use render::materialize::{
    GENERATOR_VERSION, MaterializeReport, ensure_shared_directory, materialize_overlay,
    refresh_config_credential, remove_overlay_directory,
};
pub use render::motion_scan::{MotionIssue, OverriddenSources, motion_issues};
pub use render::page_contract::{
    LookConfigKey, LookContract, PAGE_CONTRACT, PageContract, PageFunction, look_contract,
};
pub use render::preview::PreviewLine;
pub use render::preview::{
    PreviewAccent, PreviewCanvas, PreviewComposition, PreviewElement, PreviewFont, PreviewImage,
    PreviewImageSide, PreviewLabel, PreviewLabelSide, PreviewLineRole, PreviewMark,
    PreviewPosition, PreviewShape, PreviewSurface, TALLY_SEPARATOR,
};
pub use render::sample::{SampleContext, SampleTrigger, sample_content, sample_context};

pub use render::page_contract::BindRule;
pub use render::page_contract::RuntimeAttribute;
pub use render::page_contract::RuntimeProperty;
pub use render::page_contract::{CLEAR_FRAME, CONTENT_FRAME, RELOAD_FRAME};
pub use render::page_contract::{
    CONTENT_FUNCTION, READY_FUNCTION, SET_FUNCTION, SHOW_FUNCTION, SOUND_FUNCTION,
};
