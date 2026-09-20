pub mod assets;
pub mod browser_preview;
pub mod config;
pub mod content;
pub mod descriptor;
pub mod document;
pub mod error;
pub mod instance;
pub mod kinds;
pub mod materialize;
pub mod metrics;
pub mod preview;
pub mod registry;
pub mod sample;
pub mod source;

pub use assets::{
    BEHAVIOR_FILE, CONFIG_FILE, MARKUP_FILE, OVERRIDABLE_FILES, PageAssets, RESERVED_DIRECTORY,
    RUNTIME_ASSET, RUNTIME_SOURCE, SAMPLE_FILE, STYLE_FILE,
};
pub use browser_preview::{PREVIEW_PARAM, PREVIEW_VALUE, preview_page_url};
pub use config::{effective_overlay_config, validate_overlay_config};
pub use content::delivered_content;
pub use descriptor::{
    ConfigSection, DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
pub use document::{DOCUMENT_VERSION, config_document, sample_document};
pub use error::OverlayError;
pub use instance::OverlayInstance;
pub use kinds::register_builtin_kinds;
pub use materialize::{
    GENERATOR_VERSION, MaterializeReport, ensure_shared_directory, materialize_overlay,
    remove_overlay_directory,
};
pub use metrics::{StyleGuard, style_guards};
pub use preview::{
    PreviewAccent, PreviewCanvas, PreviewComposition, PreviewFont, PreviewLine, PreviewLineRole,
    PreviewPosition, PreviewShape,
};
pub use registry::OverlayKindRegistry;
pub use sample::{sample_content, sample_payload};
pub use source::{read_overlay_source, write_overlay_source};
