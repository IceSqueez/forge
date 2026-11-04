use forge_types::{MessageCode, Variant};

use crate::catalog::descriptor::{
    DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use crate::catalog::source_box::{ContentMargins, DesignSize};
use crate::config;
use crate::files::assets::PageAssets;
use crate::messages;
use crate::metrics;
use crate::metrics::ElementSizing;
use crate::motion::{self, MotionAxes, MotionProfile};
use crate::render::preview::{PreviewComposition, PreviewShape, compose};

pub const KIND_ID: &str = "overlay.frame";

pub const CONFIG_SCHEMA_VERSION: u32 = 6;

const DESIGN_WIDTH_PX: u32 = 1280;
const DESIGN_HEIGHT_PX: u32 = 720;

pub struct FrameOverlayKind;

impl OverlayKindDescriptor for FrameOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> MessageCode {
        messages::FRAME_LABEL
    }

    fn summary(&self) -> MessageCode {
        messages::FRAME_SUMMARY
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
        CONFIG_SCHEMA_VERSION
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
        let mut defaults = config::shared_style_defaults(
            config::ACCENT_PEACH,
            config::FONT_INTER,
            metrics::FRAME_SIZING,
        );
        defaults.insert(
            config::POSITION.to_owned(),
            config::text(config::POSITION_BOTTOM),
        );
        defaults.insert(config::HEADLINE.to_owned(), config::text(""));
        defaults.insert(config::SUBLINE.to_owned(), config::text("LIVE"));
        defaults.extend([
            (
                config::LABEL_SIDE.to_owned(),
                config::text(config::LABEL_SIDE_LEFT),
            ),
            (config::LABEL_OFFSET_X.to_owned(), Variant::Int(0)),
            (config::LABEL_OFFSET_Y.to_owned(), Variant::Int(0)),
        ]);
        defaults
    }

    fn look_fields(&self) -> Vec<SectionedField> {
        let mut fields = config::shared_fields();
        fields.push(config::position_field("Caption edge"));
        fields.extend(config::label_placement_fields());
        fields
    }

    fn sizing(&self) -> Option<ElementSizing> {
        Some(metrics::FRAME_SIZING)
    }

    fn style_declarations(&self) -> Vec<String> {
        metrics::FRAME.declarations()
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/frame/index.html"),
            style: include_str!("../../assets/frame/overlay.css"),
            behavior: include_str!("../../assets/frame/overlay.js"),
        }
    }

    fn preview(&self, config: &OverlayConfig, _: forge_types::NamesLanguage) -> PreviewComposition {
        compose(PreviewShape::BorderedFrame, self.sizing(), config)
    }
}
