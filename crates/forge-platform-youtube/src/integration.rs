use forge_platform_core::{
    BanListScope, CatalogLabel, ChatLimits, ConnectionAffordance, ConnectionStripMark,
    IntegrationCatalogGroup, IntegrationCategory, IntegrationDeclaration, IntegrationIdentity,
    IntegrationTint, SignInRoute, TileMark,
};
use forge_types::{IntegrationId, PlatformId};

pub const YOUTUBE_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static(crate::vocabulary::INTEGRATION_ID),
    brand_name: "YouTube",
    category: IntegrationCategory::STREAMING,
    description_key: "integration_youtube_description",
    connection: ConnectionAffordance::Connectable,
    sign_in: SignInRoute::ChatPlatform(PlatformId::YouTube),
    identity: IntegrationIdentity {
        icon: "broadcast",
        monogram: "Y",
        tile_mark: TileMark::Monogram,
        tile_tint: IntegrationTint::PlatformBrand(PlatformId::YouTube),
        hero_tint: IntegrationTint::Danger,
    },
    catalog_group: IntegrationCatalogGroup {
        label: CatalogLabel::Brand("YouTube"),
        filter_label: CatalogLabel::Brand("YouTube"),
        step_tint: IntegrationTint::PlatformBrand(PlatformId::YouTube),
        trigger_tint: IntegrationTint::PlatformBrand(PlatformId::YouTube),
    },
    connection_strip: Some(ConnectionStripMark {
        label: "YouTube",
        dot_tint: IntegrationTint::PlatformBrand(PlatformId::YouTube),
    }),
    disclaimer: None,
};

const MAX_CHAT_MESSAGE_CHARS: usize = 200;

pub const YOUTUBE_CHAT_LIMITS: ChatLimits = ChatLimits {
    max_message_chars: MAX_CHAT_MESSAGE_CHARS,
    replies_to_messages: false,
    follow_lookup: true,
    ban_list: BanListScope::SeenByForge,
};
