use crate::assets::PageAssets;
use crate::config;
use crate::descriptor::{
    DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use crate::metrics;
use crate::motion::{self, MotionAxes, MotionProfile};
use crate::preview::{PreviewComposition, PreviewShape, compose};
use crate::source_box::{ContentMargins, DesignSize};

pub const KIND_ID: &str = "overlay.ticker";

pub const BOX_CONTRACT_SCHEMA_VERSION: u32 = 2;

pub const MOTION_SCHEMA_VERSION: u32 = 3;

const TICKER_DISPLAY_SECS: i64 = 8;
const DESIGN_WIDTH_PX: u32 = 1920;
const DESIGN_HEIGHT_PX: u32 = 96;

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
        MOTION_SCHEMA_VERSION
    }

    fn motion(&self) -> MotionProfile {
        MotionProfile {
            axes: MotionAxes::ALL,
            entrance: motion::SLIDE_LEFT,
            entrance_ms: motion::DEFAULT_ENTRANCE_MS,
            exit: motion::SLIDE_RIGHT,
            exit_ms: motion::DEFAULT_EXIT_MS,
            reveal_target_id: motion::STAGE_ID,
        }
    }

    fn default_design_size(&self) -> DesignSize {
        DesignSize {
            width: DESIGN_WIDTH_PX,
            height: DESIGN_HEIGHT_PX,
        }
    }

    fn default_margins(&self) -> ContentMargins {
        ContentMargins::NONE
    }

    fn look_defaults(&self) -> OverlayConfig {
        let mut defaults =
            config::shared_style_defaults("yellow", "Bebas Neue", metrics::TICKER_SIZING);
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
        config::shared_fields()
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/ticker/index.html"),
            style: include_str!("../../assets/ticker/overlay.css"),
            behavior: include_str!("../../assets/ticker/overlay.js"),
        }
    }

    fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
        compose(
            PreviewShape::Strip,
            metrics::element_sizing(KIND_ID),
            config,
        )
    }
}
