pub mod config;
pub mod descriptor;
pub mod error;
pub mod kinds;
pub mod preview;
pub mod registry;

pub use config::{effective_overlay_config, validate_overlay_config};
pub use descriptor::{OverlayConfig, OverlayKindDescriptor};
pub use error::OverlayError;
pub use kinds::register_builtin_kinds;
pub use preview::{
    PreviewAccent, PreviewComposition, PreviewFont, PreviewLine, PreviewLineRole, PreviewPosition,
    PreviewShape,
};
pub use registry::OverlayKindRegistry;
