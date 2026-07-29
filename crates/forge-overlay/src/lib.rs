pub mod assets;
pub mod config;
pub mod descriptor;
pub mod document;
pub mod error;
pub mod instance;
pub mod kinds;
pub mod materialize;
pub mod preview;
pub mod registry;
pub mod sample;

pub use assets::{
    BEHAVIOR_FILE, CONFIG_FILE, MARKUP_FILE, OVERRIDABLE_FILES, PageAssets, RESERVED_DIRECTORY,
    RUNTIME_ASSET, RUNTIME_SOURCE, STYLE_FILE,
};
pub use config::{effective_overlay_config, validate_overlay_config};
pub use descriptor::{OverlayConfig, OverlayKindDescriptor};
pub use document::{DOCUMENT_VERSION, config_document};
pub use error::OverlayError;
pub use instance::OverlayInstance;
pub use kinds::register_builtin_kinds;
pub use materialize::{
    GENERATOR_VERSION, MaterializeReport, ensure_shared_directory, materialize_overlay,
    remove_overlay_directory,
};
pub use preview::{
    PreviewAccent, PreviewComposition, PreviewFont, PreviewLine, PreviewLineRole, PreviewPosition,
    PreviewShape,
};
pub use registry::OverlayKindRegistry;
pub use sample::sample_payload;
