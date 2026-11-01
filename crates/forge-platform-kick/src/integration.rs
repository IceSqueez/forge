use forge_platform_core::{
    BanListScope, CatalogLabel, ChatLimits, ConnectionAffordance, ConnectionStripMark,
    IntegrationCatalogGroup, IntegrationCategory, IntegrationDeclaration, IntegrationDisclaimer,
    IntegrationIdentity, IntegrationTint, SignInRoute, TileMark,
};
use forge_types::{IntegrationId, PlatformId};

pub const KICK_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static(crate::vocabulary::INTEGRATION_ID),
    brand_name: "Kick",
    category: IntegrationCategory::STREAMING,
    description_key: "integration_kick_description",
    connection: ConnectionAffordance::Connectable,
    sign_in: SignInRoute::ChatPlatform(PlatformId::Kick),
    identity: IntegrationIdentity {
        icon: "broadcast",
        monogram: "K",
        tile_mark: TileMark::Monogram,
        tile_tint: IntegrationTint::PlatformBrand(PlatformId::Kick),
        hero_tint: IntegrationTint::Info,
    },
    catalog_group: IntegrationCatalogGroup {
        label: CatalogLabel::Brand("Kick"),
        filter_label: CatalogLabel::Brand("Kick"),
        step_tint: IntegrationTint::PlatformBrand(PlatformId::Kick),
        trigger_tint: IntegrationTint::PlatformBrand(PlatformId::Kick),
    },
    connection_strip: Some(ConnectionStripMark {
        label: "Kick",
        dot_tint: IntegrationTint::PlatformBrand(PlatformId::Kick),
    }),
    disclaimer: Some(IntegrationDisclaimer {
        short_key: "integration_kick_disclaimer_short",
        full_key: "integration_kick_disclaimer",
    }),
};

const MAX_CHAT_MESSAGE_CHARS: usize = 500;

pub const KICK_CHAT_LIMITS: ChatLimits = ChatLimits {
    max_message_chars: MAX_CHAT_MESSAGE_CHARS,
    replies_to_messages: false,
    follow_lookup: false,
    ban_list: BanListScope::SeenByForge,
};
