use crate::assets::PageAssets;
use crate::config;
use crate::descriptor::{
    DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use crate::preview::{PreviewComposition, PreviewShape, compose};

pub const KIND_ID: &str = "overlay.chat";

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
        1
    }

    fn default_config(&self) -> OverlayConfig {
        let mut defaults = config::shared_style_defaults("sky", "Inter", "bottom", "slide-up");
        defaults.insert(
            config::AUTHOR.to_owned(),
            config::text("%user.display_name%"),
        );
        defaults.insert(config::AUTHOR_COLOR.to_owned(), config::text("#89dceb"));
        defaults.insert(config::BADGES.to_owned(), config::text(""));
        defaults.insert(config::MESSAGE.to_owned(), config::text("%message%"));
        defaults
    }

    fn config_fields(&self) -> Vec<SectionedField> {
        let mut fields = vec![
            config::author_field(),
            config::author_color_field(),
            config::badges_field(),
            config::message_field(),
        ];
        fields.extend(config::shared_style_fields());
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
        compose(PreviewShape::Strip, config)
    }
}
