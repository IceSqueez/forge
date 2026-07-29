use crate::descriptor::OverlayConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayInstance {
    pub id: String,
    pub display_name: String,
    pub kind_id: String,
    /// Sparse as persisted; kind defaults are merged in during materialization.
    pub config: OverlayConfig,
    /// File names from [`crate::assets::OVERRIDABLE_FILES`]; names outside that set are ignored.
    pub source_overrides: Vec<String>,
    /// Emitted top-level in the config document, never merged into `config`.
    pub credential: Option<String>,
}
