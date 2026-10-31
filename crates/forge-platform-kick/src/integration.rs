use forge_platform_core::{
    ConnectionAffordance, IntegrationCategory, IntegrationDeclaration, IntegrationDisclaimer,
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
    disclaimer: Some(IntegrationDisclaimer {
        short_key: "integration_kick_disclaimer_short",
        full_key: "integration_kick_disclaimer",
    }),
};
