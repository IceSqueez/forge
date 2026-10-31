use crate::vocabulary::PLATFORM_ID;
use forge_platform_core::{
    ConnectionAffordance, IntegrationCategory, IntegrationDeclaration, IntegrationIdentity,
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
    disclaimer: None,
};
