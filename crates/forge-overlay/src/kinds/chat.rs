use forge_registry::{ActorRole, ActorSlot, CanonicalVariable};
use forge_types::{ArgStack, PlatformId, Variant};

use forge_types::MessageCode;

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
use crate::render::row_filter::ChatRowFilter;

pub const KIND_ID: &str = "overlay.chat";

pub const CONFIG_SCHEMA_VERSION: u32 = 5;

const ROW_ENTRANCE_MS: i64 = 300;
const ROW_TEMPLATE_ID: &str = "row-template";

const DESIGN_WIDTH_PX: u32 = 400;
const DESIGN_HEIGHT_PX: u32 = 600;

pub struct ChatOverlayKind;

impl OverlayKindDescriptor for ChatOverlayKind {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn label(&self) -> MessageCode {
        messages::CHAT_LABEL
    }

    fn summary(&self) -> MessageCode {
        messages::CHAT_SUMMARY
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
        CONFIG_SCHEMA_VERSION
    }

    fn motion(&self) -> MotionProfile {
        MotionProfile {
            axes: MotionAxes::ENTRANCE_ONLY,
            entrance: motion::SLIDE_UP,
            entrance_ms: ROW_ENTRANCE_MS,
            exit: motion::NO_MOTION,
            exit_ms: motion::DEFAULT_EXIT_MS,
            reveal_target_id: ROW_TEMPLATE_ID,
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
            config::ACCENT_SKY,
            config::FONT_INTER,
            metrics::CHAT_SIZING,
        );
        defaults.insert(
            config::POSITION.to_owned(),
            config::text(config::POSITION_BOTTOM),
        );
        defaults.insert(config::AUTHOR.to_owned(), config::text("%user_login%"));
        defaults.insert(config::AUTHOR_COLOR.to_owned(), config::text("#89dceb"));
        defaults.insert(config::BADGES.to_owned(), config::text(""));
        defaults.insert(config::MESSAGE.to_owned(), config::text("%message_text%"));
        defaults.insert(config::PLATFORM.to_owned(), config::text(""));
        defaults.insert(config::MESSAGE_ID.to_owned(), config::text(""));
        defaults.insert(config::USER_ID.to_owned(), config::text(""));
        defaults.extend(config::chat_row_defaults());
        defaults.extend(config::chat_filter_defaults());
        defaults
    }

    fn look_fields(&self) -> Vec<SectionedField> {
        let mut fields = vec![
            config::author_field(),
            config::author_color_field(),
            config::badges_field(),
            config::message_field(),
            config::chat_platform_field(),
            config::chat_message_id_field(),
            config::chat_user_id_field(),
        ];
        fields.extend(config::shared_style_fields());
        fields.push(config::position_field("Newest message at"));
        fields.extend(config::chat_row_fields());
        fields.extend(config::chat_filter_fields());
        fields
    }

    fn implied_content(&self, args: &ArgStack) -> OverlayConfig {
        let sender = |slot| CanonicalVariable::actor(ActorRole::Principal, slot);
        let implied = |name: &str| {
            args.get(name)
                .and_then(Variant::as_str)
                .map(str::trim)
                .map(str::to_owned)
        };
        let platform = implied(sender(ActorSlot::Platform).name())
            .and_then(|raw| PlatformId::from_wire(&raw))
            .map(|platform| platform.as_str().to_owned());
        [
            (config::PLATFORM, platform),
            (
                config::MESSAGE_ID,
                implied(CanonicalVariable::MessageId.name()),
            ),
            (config::USER_ID, implied(sender(ActorSlot::Id).name())),
        ]
        .into_iter()
        .filter_map(|(key, value)| Some((key.to_owned(), config::text(&value?))))
        .collect()
    }

    fn withholds(
        &self,
        stored: &OverlayConfig,
        delivered: &OverlayConfig,
        args: &ArgStack,
    ) -> bool {
        let filter = ChatRowFilter::from_config(&config::effective_overlay_config(self, stored));
        let chat_text = |canonical: CanonicalVariable, shown: &str| {
            args.get(canonical.name())
                .and_then(Variant::as_str)
                .or_else(|| delivered.get(shown).and_then(Variant::as_str))
                .unwrap_or_default()
                .to_owned()
        };
        filter.hides(
            &chat_text(
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                config::AUTHOR,
            ),
            &chat_text(CanonicalVariable::MessageText, config::MESSAGE),
        )
    }

    fn sizing(&self) -> Option<ElementSizing> {
        Some(metrics::CHAT_SIZING)
    }

    fn style_declarations(&self) -> Vec<String> {
        metrics::CHAT.declarations()
    }

    fn page_assets(&self) -> PageAssets {
        PageAssets {
            markup: include_str!("../../assets/chat/index.html"),
            style: include_str!("../../assets/chat/overlay.css"),
            behavior: include_str!("../../assets/chat/overlay.js"),
        }
    }

    fn preview(&self, config: &OverlayConfig, _: forge_types::NamesLanguage) -> PreviewComposition {
        compose(PreviewShape::MessageFeed, self.sizing(), config)
    }
}
