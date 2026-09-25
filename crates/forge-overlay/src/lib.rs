pub mod assets;
pub mod audio_transport;
pub mod browser_preview;
pub mod config;
pub mod content;
pub mod descriptor;
pub mod document;
pub mod error;
pub mod icons;
pub mod instance;
pub mod kinds;
pub mod materialize;
pub mod media;
pub mod metrics;
pub mod preview;
pub mod registry;
pub mod sample;
pub mod source;
pub mod wiring;

pub use assets::{
    BEHAVIOR_FILE, CONFIG_FILE, MARKUP_FILE, OVERRIDABLE_FILES, PageAssets, RESERVED_DIRECTORY,
    RUNTIME_ASSET, RUNTIME_SOURCE, SAMPLE_FILE, STYLE_FILE,
};
pub use audio_transport::{AudioAnnouncement, AudioCommand, announcement_content, command_content};
pub use browser_preview::{
    PREVIEW_CONNECTION_FIELD, PREVIEW_PARAM, PREVIEW_VALUE, preview_page_url,
};
pub use config::{effective_overlay_config, validate_overlay_config};
pub use content::delivered_content;
pub use descriptor::{
    ConfigSection, DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
pub use document::{
    DOCUMENT_VERSION, ICON_FILE_FIELD, ICON_TINTABLE_FIELD, config_document, sample_document,
};
pub use error::OverlayError;
pub use icons::{CURATED_ICONS, CuratedIcon, IconCategory, curated_icon};
pub use instance::OverlayInstance;
pub use kinds::register_builtin_kinds;
pub use materialize::{
    GENERATOR_VERSION, MaterializeReport, ensure_shared_directory, materialize_overlay,
    remove_overlay_directory,
};
pub use media::{
    CLIP_REFERENCE_PREFIX, ClipReference, EmittedIcon, GENERATED_MEDIA_DIRECTORY,
    IMAGE_REFERENCE_PREFIX, IconValue, ImageReference, MEDIA_KEYS, MediaIssue, MediaSlot,
    MediaValue, OverlayMedia, ResolvedMedia, clip_reference, clip_references, emitted_icon,
    emitted_media_value, glyph_media, image_reference, image_references, key_holds_media,
    media_slot, read_icon_value, read_media_value,
};
pub use metrics::{
    AxisBound, AxisFallback, ElementAxis, ElementSizing, StyleGuard, element_sizing, style_guards,
};
pub use preview::{
    CANVAS_HEIGHT_PX, CANVAS_WIDTH_PX, PreviewAccent, PreviewCanvas, PreviewComposition,
    PreviewElement, PreviewFont, PreviewLine, PreviewLineRole, PreviewPosition, PreviewShape,
};
pub use registry::OverlayKindRegistry;
pub use sample::{SampleContext, SampleTrigger, sample_content, sample_context};
pub use source::{read_overlay_source, write_overlay_source};
pub use wiring::{
    EventWiringTrigger, accepts_event_wiring, is_curated, order_curated_first, suggested_content,
};
