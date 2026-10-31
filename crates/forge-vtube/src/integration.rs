use forge_platform_core::{
    ConnectionAffordance, IntegrationCategory, IntegrationDeclaration, IntegrationIdentity,
    IntegrationTint, SignInRoute, TileMark,
};
use forge_types::IntegrationId;

use crate::vocabulary;

pub const VTUBE_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static(vocabulary::INTEGRATION_ID),
    brand_name: "VTube Studio",
    category: IntegrationCategory::APPS,
    description_key: "integration_vtube_description",
    connection: ConnectionAffordance::Connectable,
    sign_in: SignInRoute::CredentialForm,
    identity: IntegrationIdentity {
        icon: "mood-tongue",
        monogram: "V",
        tile_mark: TileMark::Glyph,
        tile_tint: IntegrationTint::Warning,
        hero_tint: IntegrationTint::Warning,
    },
    disclaimer: None,
};
