use crate::assets::PageAssets;
use crate::config;
use crate::descriptor::{
    DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use crate::metrics;
use crate::motion::{self, MotionAxes, MotionProfile};
use crate::preview::{PreviewComposition, PreviewShape, compose};
use crate::source_box::{ContentMargins, DesignSize};

pub const KIND_ID: &str = "overlay.frame";

pub const BOX_CONTRACT_SCHEMA_VERSION: u32 = 2;

pub const MOTION_SCHEMA_VERSION: u32 = 3;

const DESIGN_WIDTH_PX: u32 = 1280;
const DESIGN_HEIGHT_PX: u32 = 720;

pub struct FrameOverlayKind;

impl OverlayKindDescriptor for FrameOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> &str {
        "Frame"
    }

    fn summary(&self) -> &str {
        "Borders a camera or capture area and labels its corner"
    }

    fn icon_name(&self) -> &str {
        "frame"
    }

    fn delivery_disposition(&self) -> DeliveryDisposition {
        DeliveryDisposition::Replace
    }

    fn order_sensitive(&self) -> bool {
        false
    }

    fn config_schema_version(&self) -> u32 {
        MOTION_SCHEMA_VERSION
    }

    fn motion(&self) -> MotionProfile {
        MotionProfile {
            axes: MotionAxes::ENTRANCE_ONLY,
            entrance: motion::FADE,
            entrance_ms: motion::DEFAULT_ENTRANCE_MS,
            exit: motion::NO_MOTION,
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
        let mut defaults = config::shared_style_defaults("peach", "Inter", metrics::FRAME_SIZING);
        defaults.insert(
            config::POSITION.to_owned(),
            config::text(config::POSITION_BOTTOM),
        );
        defaults.insert(config::HEADLINE.to_owned(), config::text(""));
        defaults.insert(config::SUBLINE.to_owned(), config::text("LIVE"));
        defaults
    }

    fn look_fields(&self) -> Vec<SectionedField> {
        let mut fields = config::shared_fields();
        fields.push(config::position_field("Caption edge"));
        fields
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/frame/index.html"),
            style: include_str!("../../assets/frame/overlay.css"),
            behavior: include_str!("../../assets/frame/overlay.js"),
        }
    }

    fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
        compose(PreviewShape::BorderedFrame, config)
    }
}
