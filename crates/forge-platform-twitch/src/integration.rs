use crate::vocabulary::PLATFORM_ID;
use forge_platform_core::{
    BanListScope, CatalogLabel, ChatLimits, ConnectionAffordance, ConnectionStripMark,
    IntegrationCatalogGroup, IntegrationCategory, IntegrationDeclaration, IntegrationIdentity,
    IntegrationTint, SignInRoute, TileMark,
};
use forge_types::{IntegrationId, PlatformId};

pub const TWITCH_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static(PLATFORM_ID),
    brand_name: "Twitch",
    category: IntegrationCategory::STREAMING,
    description_key: "integration_twitch_description",
    connection: ConnectionAffordance::Connectable,
    sign_in: SignInRoute::ChatPlatform(PlatformId::Twitch),
    identity: IntegrationIdentity {
        icon: "broadcast",
        monogram: "T",
        tile_mark: TileMark::Monogram,
        tile_tint: IntegrationTint::PlatformBrand(PlatformId::Twitch),
        hero_tint: IntegrationTint::ThemeBrand,
    },
    catalog_group: IntegrationCatalogGroup {
        label: CatalogLabel::Brand("Twitch"),
        filter_label: CatalogLabel::Brand("Twitch"),
        step_tint: IntegrationTint::ThemeBrand,
        trigger_tint: IntegrationTint::PlatformBrand(PlatformId::Twitch),
    },
    connection_strip: Some(ConnectionStripMark {
        label: "Twitch",
        dot_tint: IntegrationTint::PlatformBrand(PlatformId::Twitch),
    }),
    disclaimer: None,
};

pub const TWITCH_CHAT_LIMITS: ChatLimits = ChatLimits {
    max_message_chars: crate::chat::MAX_MESSAGE_LEN,
    replies_to_messages: true,
    follow_lookup: true,
    ban_list: BanListScope::WholeChannel,
};
