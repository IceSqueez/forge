use forge_platform_core::{
    ConnectionAffordance, IntegrationCategory, IntegrationDeclaration, IntegrationIdentity,
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
    disclaimer: None,
};
