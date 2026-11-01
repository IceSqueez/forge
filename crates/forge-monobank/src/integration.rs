use forge_platform_core::{
    CatalogLabel, ConnectionAffordance, IntegrationCatalogGroup, IntegrationCategory,
    IntegrationDeclaration, IntegrationIdentity, IntegrationTint, SignInRoute, TileMark,
};
use forge_types::IntegrationId;

pub const MONOBANK_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static("monobank"),
    brand_name: "monobank",
    category: IntegrationCategory::DONATIONS,
    description_key: "integration_monobank_description",
    connection: ConnectionAffordance::Connectable,
    sign_in: SignInRoute::CredentialForm,
    identity: IntegrationIdentity {
        icon: "coin",
        monogram: "M",
        tile_mark: TileMark::Monogram,
        tile_tint: IntegrationTint::TextSecondary,
        hero_tint: IntegrationTint::TextSecondary,
    },
    catalog_group: IntegrationCatalogGroup {
        label: CatalogLabel::Brand("Monobank"),
        filter_label: CatalogLabel::Brand("Monobank"),
        step_tint: IntegrationTint::TextSecondary,
        trigger_tint: IntegrationTint::TextSecondary,
    },
    connection_strip: None,
    disclaimer: None,
};
