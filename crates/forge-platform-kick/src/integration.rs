use forge_platform_core::{
    CatalogLabel, ConnectionAffordance, IntegrationCatalogGroup, IntegrationCategory,
    IntegrationDeclaration, IntegrationDisclaimer, IntegrationIdentity, IntegrationTint,
    SignInRoute, TileMark,
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
    disclaimer: Some(IntegrationDisclaimer {
        short_key: "integration_kick_disclaimer_short",
        full_key: "integration_kick_disclaimer",
    }),
};
