use forge_platform_core::{
    CatalogLabel, ConnectionAffordance, ConnectionStripMark, IntegrationCatalogGroup,
    IntegrationCategory, IntegrationDeclaration, IntegrationIdentity, IntegrationTint, SignInRoute,
    TileMark,
};
use forge_types::IntegrationId;

use crate::vocabulary;

pub const DISCORD_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static(vocabulary::INTEGRATION_ID),
    brand_name: "Discord",
    category: IntegrationCategory::COMMUNITY,
    description_key: "integration_discord_description",
    connection: ConnectionAffordance::Configured,
    sign_in: SignInRoute::NotNeeded,
    identity: IntegrationIdentity {
        icon: "brand-discord",
        monogram: "D",
        tile_mark: TileMark::Glyph,
        tile_tint: IntegrationTint::ThemeBrand,
        hero_tint: IntegrationTint::ThemeBrand,
    },
    catalog_group: IntegrationCatalogGroup {
        label: CatalogLabel::Brand("Discord"),
        filter_label: CatalogLabel::Brand("Discord"),
        step_tint: IntegrationTint::Info,
        trigger_tint: IntegrationTint::Info,
    },
    connection_strip: Some(ConnectionStripMark {
        label: "Discord",
        dot_tint: IntegrationTint::ThemeBrand,
    }),
    disclaimer: None,
};
