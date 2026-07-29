use forge_registry::FormField;

use crate::assets::PageAssets;
use crate::config;
use crate::descriptor::{OverlayConfig, OverlayKindDescriptor};
use crate::preview::{PreviewComposition, PreviewShape, compose};

pub const KIND_ID: &str = "overlay.ticker";

pub struct TickerOverlayKind;

impl OverlayKindDescriptor for TickerOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> &str {
        "Ticker"
    }

    fn summary(&self) -> &str {
        "Runs a full-width strip carrying the latest event"
    }

    fn icon_name(&self) -> &str {
        "arrow-badge-right"
    }

    fn config_schema_version(&self) -> u32 {
        1
    }

    fn default_config(&self) -> OverlayConfig {
        let mut defaults = config::shared_defaults("yellow", "Bebas Neue", "bottom", "slide-left");
        defaults.insert(
            config::HEADLINE.to_owned(),
            config::text("Last tip: %user% - $%amount%"),
        );
        defaults.insert(config::SUBLINE.to_owned(), config::text("\"%message%\""));
        defaults.insert(config::DURATION.to_owned(), forge_types::Variant::Int(8));
        defaults
    }

    fn config_fields(&self) -> Vec<FormField> {
        let mut fields = config::shared_fields();
        fields.push(config::duration_field());
        fields.push(config::sound_field());
        fields
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/ticker/index.html"),
            style: include_str!("../../assets/ticker/overlay.css"),
            behavior: include_str!("../../assets/ticker/overlay.js"),
        }
    }

    fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
        compose(PreviewShape::Strip, config)
    }
}
