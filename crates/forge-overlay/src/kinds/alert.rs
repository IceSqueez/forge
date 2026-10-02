use crate::assets::PageAssets;
use crate::config;
use crate::descriptor::{
    DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use crate::metrics;
use crate::preview::{PreviewComposition, PreviewShape, compose};
use crate::source_box::{ContentMargins, DesignSize};

pub const KIND_ID: &str = "overlay.alert";

pub const BOX_CONTRACT_SCHEMA_VERSION: u32 = 3;

const DESIGN_WIDTH_PX: u32 = 800;
const DESIGN_HEIGHT_PX: u32 = 600;
const MARGIN_PERCENT: u32 = 10;

pub struct AlertOverlayKind;

impl OverlayKindDescriptor for AlertOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> &str {
        "Alert"
    }

    fn summary(&self) -> &str {
        "Shows a banner for a few seconds each time an action sends one"
    }

    fn icon_name(&self) -> &str {
        "bell"
    }

    fn delivery_disposition(&self) -> DeliveryDisposition {
        DeliveryDisposition::Transient
    }

    fn order_sensitive(&self) -> bool {
        false
    }

    fn config_schema_version(&self) -> u32 {
        BOX_CONTRACT_SCHEMA_VERSION
    }

    fn default_design_size(&self) -> DesignSize {
        DesignSize {
            width: DESIGN_WIDTH_PX,
            height: DESIGN_HEIGHT_PX,
        }
    }

    fn default_margins(&self) -> ContentMargins {
        ContentMargins::even(MARGIN_PERCENT)
    }

    fn look_defaults(&self) -> OverlayConfig {
        let mut defaults =
            config::shared_style_defaults("mauve", "Rubik", "slide-up", metrics::ALERT_SIZING);
        defaults.insert(
            config::HEADLINE.to_owned(),
            config::text("Thanks for the sub!"),
        );
        defaults.insert(
            config::SUBLINE.to_owned(),
            config::text("%sub_cumulative_months% months subscribed"),
        );
        defaults.insert(config::ICON.to_owned(), config::text(config::DEFAULT_ICON));
        defaults
    }

    fn look_fields(&self) -> Vec<SectionedField> {
        let mut fields = config::shared_fields();
        fields.push(config::icon_field());
        fields
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/alert/index.html"),
            style: include_str!("../../assets/alert/overlay.css"),
            behavior: include_str!("../../assets/alert/overlay.js"),
        }
    }

    fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
        compose(PreviewShape::BadgeBanner, config)
    }
}
