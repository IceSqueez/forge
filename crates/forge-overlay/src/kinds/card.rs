use forge_types::{ArgStack, MessageCode};

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
use crate::render::content::delivered_content;
use crate::render::preview::{PreviewComposition, PreviewShape, compose};

pub const KIND_ID: &str = "overlay.card";

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

const DESIGN_WIDTH_PX: u32 = 640;
const DESIGN_HEIGHT_PX: u32 = 160;

const PANEL_CONTENT_KEYS: [&str; 4] = [
    config::HEADLINE,
    config::SUBLINE,
    config::ICON,
    config::IMAGE,
];

pub struct CardOverlayKind;

impl OverlayKindDescriptor for CardOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> MessageCode {
        messages::CARD_LABEL
    }

    fn summary(&self) -> MessageCode {
        messages::CARD_SUMMARY
    }

    fn icon_name(&self) -> &str {
        "pin"
    }

    fn delivery_disposition(&self) -> DeliveryDisposition {
        DeliveryDisposition::Replace
    }

    fn order_sensitive(&self) -> bool {
        false
    }

    fn opening_content(&self, stored: &OverlayConfig) -> Option<OverlayConfig> {
        let mut content = delivered_content(self, stored, &OverlayConfig::new(), &ArgStack::new());
        content.remove(config::SPEECH);
        Some(content)
    }

    fn panel_replaces_content(&self, before: &OverlayConfig, after: &OverlayConfig) -> bool {
        let before = config::effective_overlay_config(self, before);
        let after = config::effective_overlay_config(self, after);
        PANEL_CONTENT_KEYS
            .iter()
            .any(|key| before.get(*key) != after.get(*key))
    }

    fn config_schema_version(&self) -> u32 {
        CONFIG_SCHEMA_VERSION
    }

    fn motion(&self) -> MotionProfile {
        MotionProfile {
            axes: MotionAxes::ENTRANCE_AND_TEXT,
            entrance: motion::SLIDE_UP,
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
        config::card_look_defaults(
            config::ACCENT_MAUVE,
            config::FONT_RUBIK,
            "Welcome to the stream",
            "Say hi in chat",
        )
    }

    fn look_fields(&self) -> Vec<SectionedField> {
        config::card_look_fields()
    }

    fn sizing(&self) -> Option<ElementSizing> {
        Some(metrics::ALERT_SIZING)
    }

    fn style_declarations(&self) -> Vec<String> {
        metrics::ALERT.declarations()
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/card/index.html"),
            style: include_str!("../../assets/card-look/look.css"),
            behavior: concat!(
                include_str!("../../assets/card-look/look.js"),
                include_str!("../../assets/card/overlay.js")
            ),
        }
    }

    fn preview(&self, config: &OverlayConfig, _: forge_types::NamesLanguage) -> PreviewComposition {
        compose(PreviewShape::Card, self.sizing(), config)
    }
}
