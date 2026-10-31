use forge_platform_core::{
    ConnectionAffordance, IntegrationCategory, IntegrationDeclaration, IntegrationIdentity,
    IntegrationTint, SignInRoute, TileMark,
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
    disclaimer: None,
};
