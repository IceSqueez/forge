use forge_platform_core::{
    ConnectionAffordance, IntegrationCategory, IntegrationDeclaration, IntegrationIdentity,
    IntegrationTint, SignInRoute, TileMark,
};
use forge_types::IntegrationId;

use crate::vocabulary;

pub const DISCORD_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static(vocabulary::INTEGRATION_ID),
    brand_name: "Discord",
    category: IntegrationCategory::COMMUNITY,
    description_key: "integration_discord_description",
    connection: ConnectionAffordance::Connectionless,
    sign_in: SignInRoute::NotNeeded,
    identity: IntegrationIdentity {
        icon: "brand-discord",
        monogram: "D",
        tile_mark: TileMark::Glyph,
        tile_tint: IntegrationTint::ThemeBrand,
        hero_tint: IntegrationTint::ThemeBrand,
    },
    disclaimer: None,
};
