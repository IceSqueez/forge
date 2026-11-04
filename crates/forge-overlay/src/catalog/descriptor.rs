use std::collections::BTreeMap;

use forge_registry::FormField;
use forge_types::{ArgStack, MessageCode, NamesLanguage, Variant};

use crate::catalog::source_box::{ContentMargins, DesignSize};
use crate::config;
use crate::files::assets::PageAssets;
use crate::metrics::ElementSizing;
use crate::motion::{self, MotionProfile};
use crate::render::base;
use crate::render::preview::PreviewComposition;

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
pub enum ContentFeed {
    Deliveries,
    LatestSlot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSection {
    Content,
    Text,
    Card,
    Placement,
    Motion,
    Chat,
    Audio,
    Timing,
}

#[derive(Debug, Clone)]
pub struct SectionedField {
    pub section: ConfigSection,
    pub delivered: bool,
    pub field: FormField,
}

pub trait OverlayKindDescriptor: Send + Sync {
    fn id(&self) -> &str;
    fn label(&self) -> MessageCode;
    fn summary(&self) -> MessageCode;
    fn icon_name(&self) -> &str;
    fn delivery_disposition(&self) -> DeliveryDisposition;
    fn order_sensitive(&self) -> bool;
    fn content_feed(&self) -> ContentFeed {
        ContentFeed::Deliveries
    }
    fn config_schema_version(&self) -> u32;
    fn look_defaults(&self) -> OverlayConfig {
        OverlayConfig::new()
    }
    fn look_fields(&self) -> Vec<SectionedField> {
        Vec::new()
    }
    fn implied_content(&self, _args: &ArgStack) -> OverlayConfig {
        OverlayConfig::new()
    }
    fn opening_content(&self, _stored: &OverlayConfig) -> Option<OverlayConfig> {
        None
    }
    fn panel_replaces_content(&self, _before: &OverlayConfig, _after: &OverlayConfig) -> bool {
        false
    }
    fn withholds(
        &self,
        _stored: &OverlayConfig,
        _delivered: &OverlayConfig,
        _args: &ArgStack,
    ) -> bool {
        false
    }
    fn default_display_secs(&self) -> i64 {
        base::DEFAULT_DISPLAY_SECS
    }
    fn default_design_size(&self) -> DesignSize {
        DesignSize::BROWSER_SOURCE_DEFAULT
    }
    fn default_margins(&self) -> ContentMargins {
        ContentMargins::NONE
    }
    fn motion(&self) -> MotionProfile {
        MotionProfile::STILL
    }
    fn default_config(&self) -> OverlayConfig {
        let mut defaults = base::base_defaults(
            self.delivery_disposition(),
            self.default_display_secs(),
            self.default_design_size(),
            self.default_margins(),
        );
        defaults.extend(self.look_defaults());
        defaults.extend(motion::motion_defaults(self.motion()));
        if !self.speaks() {
            for key in config::SPEECH_KEYS {
                defaults.remove(key);
            }
        }
        defaults
    }
    fn config_fields(&self) -> Vec<SectionedField> {
        let mut fields = self.look_fields();
        fields.extend(motion::motion_fields(self.motion().axes));
        fields.extend(
            base::base_fields(self.delivery_disposition())
                .into_iter()
                .filter(|sectioned| {
                    self.speaks() || !config::SPEECH_KEYS.contains(&sectioned.field.key())
                }),
        );
        fields
    }
    fn speaks(&self) -> bool {
        true
    }
    fn sizing(&self) -> Option<ElementSizing> {
        None
    }
    fn style_declarations(&self) -> Vec<String> {
        Vec::new()
    }
    fn page_assets(&self) -> PageAssets;
    fn preview(&self, config: &OverlayConfig, language: NamesLanguage) -> PreviewComposition;
    fn has_visual_page(&self) -> bool {
        true
    }
}
