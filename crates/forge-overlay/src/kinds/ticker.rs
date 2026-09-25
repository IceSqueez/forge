use crate::assets::PageAssets;
use crate::config;
use crate::descriptor::{
    DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use crate::metrics;
use crate::preview::{PreviewComposition, PreviewShape, compose};

pub const KIND_ID: &str = "overlay.ticker";

const TICKER_DISPLAY_SECS: i64 = 8;

pub struct TickerOverlayKind;

impl OverlayKindDescriptor for TickerOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> &str {
        "Ticker"
    }

    fn summary(&self) -> &str {
        "Runs a full-width strip carrying the latest line an action sends"
    }

    fn icon_name(&self) -> &str {
        "arrow-badge-right"
    }

    fn delivery_disposition(&self) -> DeliveryDisposition {
        DeliveryDisposition::Transient
    }

    fn order_sensitive(&self) -> bool {
        false
    }

    fn config_schema_version(&self) -> u32 {
        1
    }

    fn look_defaults(&self) -> OverlayConfig {
        let mut defaults = config::shared_style_defaults(
            "yellow",
            "Bebas Neue",
            "bottom",
            "slide-left",
            metrics::TICKER_SIZING,
        );
        defaults.insert(
            config::HEADLINE.to_owned(),
            config::text("Latest cheer: %bits_amount% bits"),
        );
        defaults.insert(
            config::SUBLINE.to_owned(),
            config::text("\"%message_text%\""),
        );
        defaults
    }

    fn default_display_secs(&self) -> i64 {
        TICKER_DISPLAY_SECS
    }

    fn look_fields(&self) -> Vec<SectionedField> {
        config::shared_fields(metrics::TICKER_SIZING)
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
