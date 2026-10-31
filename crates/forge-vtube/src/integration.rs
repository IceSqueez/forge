use forge_platform_core::{
    CatalogLabel, ConnectionAffordance, IntegrationCatalogGroup, IntegrationCategory,
    IntegrationDeclaration, IntegrationIdentity, IntegrationTint, SignInRoute, TileMark,
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
    catalog_group: IntegrationCatalogGroup {
        label: CatalogLabel::Brand("VTube Studio"),
        filter_label: CatalogLabel::Brand("VTube"),
        step_tint: IntegrationTint::AccentTeal,
        trigger_tint: IntegrationTint::AccentTeal,
    },
    disclaimer: None,
};
