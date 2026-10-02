use crate::assets::PageAssets;
use crate::config;
use crate::descriptor::{
    DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use crate::metrics;
use crate::preview::{PreviewComposition, PreviewShape, compose};
use crate::source_box::{ContentMargins, DesignSize};

pub const KIND_ID: &str = "overlay.goal";

pub const BOX_CONTRACT_SCHEMA_VERSION: u32 = 2;

const DESIGN_WIDTH_PX: u32 = 640;
const DESIGN_HEIGHT_PX: u32 = 160;

pub struct GoalOverlayKind;

impl OverlayKindDescriptor for GoalOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> &str {
        "Goal"
    }

    fn summary(&self) -> &str {
        "Tracks progress toward a target an action keeps updating"
    }

    fn icon_name(&self) -> &str {
        "target-arrow"
    }

    fn delivery_disposition(&self) -> DeliveryDisposition {
        DeliveryDisposition::Replace
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
        ContentMargins::NONE
    }

    fn look_defaults(&self) -> OverlayConfig {
        let mut defaults =
            config::shared_style_defaults("green", "Inter", "fade", metrics::GOAL_SIZING);
        defaults.insert(config::LABEL.to_owned(), config::text("Sub goal"));
        defaults.insert(config::VALUE.to_owned(), config::text("42"));
        defaults.insert(config::TARGET.to_owned(), config::text("100"));
        defaults
    }

    fn look_fields(&self) -> Vec<SectionedField> {
        let mut fields = vec![
            config::label_field(),
            config::value_field(),
            config::target_field(),
        ];
        fields.extend(config::shared_style_fields());
        fields
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/goal/index.html"),
            style: include_str!("../../assets/goal/overlay.css"),
            behavior: include_str!("../../assets/goal/overlay.js"),
        }
    }

    fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
        compose(PreviewShape::ProgressBar, config)
    }
}
