use forge_platform_core::{
    CatalogLabel, ConnectionAffordance, ConnectionStripMark, IntegrationCatalogGroup,
    IntegrationCategory, IntegrationDeclaration, IntegrationIdentity, IntegrationTint, SignInRoute,
    TileMark,
};
use forge_types::IntegrationId;

pub const DONATELLO_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static("donatello"),
    brand_name: "Donatello",
    category: IntegrationCategory::DONATIONS,
    description_key: "integration_donatello_description",
    connection: ConnectionAffordance::Connectable,
    sign_in: SignInRoute::CredentialForm,
    identity: IntegrationIdentity {
        icon: "coin",
        monogram: "D",
        tile_mark: TileMark::Monogram,
        tile_tint: IntegrationTint::AccentPinkLight,
        hero_tint: IntegrationTint::AccentPinkLight,
    },
    catalog_group: IntegrationCatalogGroup {
        label: CatalogLabel::Brand("Donatello"),
        filter_label: CatalogLabel::Brand("Donatello"),
        step_tint: IntegrationTint::AccentPinkLight,
        trigger_tint: IntegrationTint::AccentPinkLight,
    },
    connection_strip: Some(ConnectionStripMark {
        label: "Donatello",
        dot_tint: IntegrationTint::AccentPinkLight,
    }),
    disclaimer: None,
};
