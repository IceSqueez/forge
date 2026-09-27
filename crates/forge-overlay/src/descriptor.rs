use std::collections::BTreeMap;

use forge_registry::FormField;
use forge_types::Variant;

use crate::assets::PageAssets;
use crate::base;
use crate::preview::PreviewComposition;

pub type OverlayConfig = BTreeMap<String, Variant>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryDisposition {
    Replace,
    Transient,
    Append,
}

impl DeliveryDisposition {
    pub fn retains_last_content(self) -> bool {
        matches!(self, Self::Replace)
    }
}

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
    fn order_sensitive(&self) -> bool;
    fn config_schema_version(&self) -> u32;
    fn look_defaults(&self) -> OverlayConfig {
        OverlayConfig::new()
    }
    fn look_fields(&self) -> Vec<SectionedField> {
        Vec::new()
    }
    fn default_display_secs(&self) -> i64 {
        base::DEFAULT_DISPLAY_SECS
    }
    fn default_config(&self) -> OverlayConfig {
        let mut defaults =
            base::base_defaults(self.delivery_disposition(), self.default_display_secs());
        defaults.extend(self.look_defaults());
        defaults
    }
    fn config_fields(&self) -> Vec<SectionedField> {
        let mut fields = self.look_fields();
        fields.extend(base::base_fields(self.delivery_disposition()));
        fields
    }
    fn page_assets(&self) -> PageAssets;
    fn preview(&self, config: &OverlayConfig) -> PreviewComposition;
    fn has_visual_page(&self) -> bool {
        true
    }
}
