use std::collections::BTreeMap;

use forge_registry::FormField;
use forge_types::Variant;

use crate::preview::PreviewComposition;

pub type OverlayConfig = BTreeMap<String, Variant>;

pub trait OverlayKindDescriptor: Send + Sync {
    fn id(&self) -> &str;
    fn label(&self) -> &str;
    fn summary(&self) -> &str;
    fn icon_name(&self) -> &str;
    /// Bumped only when a stored config needs a Rust-side rewrite pass to stay readable.
    fn config_schema_version(&self) -> u32;
    fn default_config(&self) -> OverlayConfig;
    fn config_fields(&self) -> Vec<FormField>;
    /// Takes the effective config, never the sparse stored one.
    fn preview(&self, config: &OverlayConfig) -> PreviewComposition;
}
