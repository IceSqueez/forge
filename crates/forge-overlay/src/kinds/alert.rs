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

pub const KIND_ID: &str = "overlay.alert";

pub const CONFIG_SCHEMA_VERSION: u32 = 7;

const DESIGN_WIDTH_PX: u32 = 800;
const DESIGN_HEIGHT_PX: u32 = 600;
const MARGIN_PERCENT: u32 = 10;

pub struct AlertOverlayKind;

impl OverlayKindDescriptor for AlertOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> MessageCode {
        messages::ALERT_LABEL
    }

    fn summary(&self) -> MessageCode {
        messages::ALERT_SUMMARY
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
        CONFIG_SCHEMA_VERSION
    }

    fn motion(&self) -> MotionProfile {
        MotionProfile {
            axes: MotionAxes::ALL,
            entrance: motion::SLIDE_UP,
            entrance_ms: motion::DEFAULT_ENTRANCE_MS,
            exit: motion::SLIDE_DOWN,
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
        ContentMargins::even(MARGIN_PERCENT)
    }

    fn look_defaults(&self) -> OverlayConfig {
        let mut defaults = config::shared_style_defaults(
            config::ACCENT_MAUVE,
            config::FONT_RUBIK,
            metrics::ALERT_SIZING,
        );
        defaults.insert(
            config::HEADLINE.to_owned(),
            config::text("Thanks for the sub!"),
        );
        defaults.insert(
            config::SUBLINE.to_owned(),
            config::text("%sub_cumulative_months% months subscribed"),
        );
        defaults.insert(config::ICON.to_owned(), config::text(config::DEFAULT_ICON));
        defaults.extend([
            (
                config::BACKGROUND_COLOR.to_owned(),
                config::text(config::DEFAULT_BACKGROUND_COLOR),
            ),
            (
                config::BACKGROUND_OPACITY.to_owned(),
                Variant::Int(config::DEFAULT_BACKGROUND_OPACITY_PERCENT),
            ),
            (config::IMAGE.to_owned(), config::text("")),
            (
                config::IMAGE_POSITION.to_owned(),
                config::text(config::IMAGE_POSITION_ABOVE),
            ),
            (
                config::IMAGE_HEIGHT.to_owned(),
                Variant::Int(config::DEFAULT_IMAGE_HEIGHT_PX),
            ),
        ]);
        defaults.extend(config::card_size_defaults());
        defaults
    }

    fn look_fields(&self) -> Vec<SectionedField> {
        let mut fields = config::shared_fields();
        fields.push(config::icon_field());
        fields.extend(config::background_fields());
        fields.extend(config::image_fields());
        fields.extend(config::card_size_fields());
        fields
    }

    fn sizing(&self) -> Option<ElementSizing> {
        Some(metrics::ALERT_SIZING)
    }

    fn style_declarations(&self) -> Vec<String> {
        metrics::ALERT.declarations()
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/alert/index.html"),
            style: include_str!("../../assets/alert/overlay.css"),
            behavior: include_str!("../../assets/alert/overlay.js"),
        }
    }

    fn preview(&self, config: &OverlayConfig, _: forge_types::NamesLanguage) -> PreviewComposition {
        compose(PreviewShape::BadgeBanner, self.sizing(), config)
    }
}
