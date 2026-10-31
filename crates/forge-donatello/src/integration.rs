use forge_platform_core::{
    ConnectionAffordance, IntegrationCategory, IntegrationDeclaration, IntegrationIdentity,
    IntegrationTint, SignInRoute, TileMark,
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
    disclaimer: None,
};
