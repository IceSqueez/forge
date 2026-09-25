use std::collections::BTreeMap;

use forge_registry::FormField;
use forge_types::Variant;

use crate::assets::PageAssets;
use crate::base;
use crate::preview::PreviewComposition;

pub type OverlayConfig = BTreeMap<String, Variant>;

/// How delivered content composes with what the page is already showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryDisposition {
    Replace,
    Transient,
    Append,
}

impl DeliveryDisposition {
    /// Replace content is the display itself; transient and append content is gone once shown.
    pub fn retains_last_content(self) -> bool {
        matches!(self, Self::Replace)
    }
}

/// Content is the group a step may supply per delivery; style and behavior belong to the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSection {
    Content,
    Style,
    Behavior,
}

#[derive(Debug, Clone)]
pub struct SectionedField {
    pub section: ConfigSection,
    pub field: FormField,
}

pub trait OverlayKindDescriptor: Send + Sync {
    fn id(&self) -> &str;
    fn label(&self) -> &str;
    fn summary(&self) -> &str;
    fn icon_name(&self) -> &str;
    fn delivery_disposition(&self) -> DeliveryDisposition;
    /// Delivery to one page must preserve the order it received when true.
    fn order_sensitive(&self) -> bool;
    /// Bumped only when a stored config needs a Rust-side rewrite pass to stay readable.
    fn config_schema_version(&self) -> u32;
    fn look_defaults(&self) -> OverlayConfig {
        OverlayConfig::new()
    }
    fn look_fields(&self) -> Vec<SectionedField> {
        Vec::new()
    }
    /// Seconds a transient show stays up when neither the overlay nor the step names a duration.
    fn default_display_secs(&self) -> i64 {
        base::DEFAULT_DISPLAY_SECS
    }
    /// The base's defaults with the look's laid over them; a look implements `look_defaults`.
    fn default_config(&self) -> OverlayConfig {
        let mut defaults =
            base::base_defaults(self.delivery_disposition(), self.default_display_secs());
        defaults.extend(self.look_defaults());
        defaults
    }
    /// The look's fields followed by the base's; a look implements `look_fields`.
    fn config_fields(&self) -> Vec<SectionedField> {
        let mut fields = self.look_fields();
        fields.extend(base::base_fields(self.delivery_disposition()));
        fields
    }
    /// Carries no config value; the page binds against its config document at runtime.
    fn page_assets(&self) -> PageAssets;
    /// Takes the effective config, never the sparse stored one.
    fn preview(&self, config: &OverlayConfig) -> PreviewComposition;
    fn has_visual_page(&self) -> bool {
        true
    }
}
