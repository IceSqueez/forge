use crate::assets::PageAssets;
use crate::config;
use crate::descriptor::{
    DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use crate::metrics;
use crate::preview::{PreviewComposition, PreviewShape, compose};
use crate::source_box::{ContentMargins, DesignSize};

pub const KIND_ID: &str = "overlay.chat";

pub const BOX_CONTRACT_SCHEMA_VERSION: u32 = 2;

const DESIGN_WIDTH_PX: u32 = 400;
const DESIGN_HEIGHT_PX: u32 = 600;

pub struct ChatOverlayKind;

impl OverlayKindDescriptor for ChatOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> &str {
        "Chat"
    }

    fn summary(&self) -> &str {
        "Lists the latest chat rows an action sends, oldest falling off the ring"
    }

    fn icon_name(&self) -> &str {
        "message-circle"
    }

    fn delivery_disposition(&self) -> DeliveryDisposition {
        DeliveryDisposition::Append
    }

    fn order_sensitive(&self) -> bool {
        true
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
            config::shared_style_defaults("sky", "Inter", "slide-up", metrics::CHAT_SIZING);
        defaults.insert(
            config::POSITION.to_owned(),
            config::text(config::POSITION_BOTTOM),
        );
        defaults.insert(config::AUTHOR.to_owned(), config::text("%user_login%"));
        defaults.insert(config::AUTHOR_COLOR.to_owned(), config::text("#89dceb"));
        defaults.insert(config::BADGES.to_owned(), config::text(""));
        defaults.insert(config::MESSAGE.to_owned(), config::text("%message_text%"));
        defaults
    }

    fn look_fields(&self) -> Vec<SectionedField> {
        let mut fields = vec![
            config::author_field(),
            config::author_color_field(),
            config::badges_field(),
            config::message_field(),
        ];
        fields.extend(config::shared_style_fields());
        fields.push(config::position_field("Newest message at"));
        fields
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/chat/index.html"),
            style: include_str!("../../assets/chat/overlay.css"),
            behavior: include_str!("../../assets/chat/overlay.js"),
        }
    }

    fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
        compose(PreviewShape::MessageFeed, config)
    }
}
