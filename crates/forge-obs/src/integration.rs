use forge_platform_core::{
    CatalogLabel, ConnectionAffordance, IntegrationCatalogGroup, IntegrationCategory,
    IntegrationDeclaration, IntegrationIdentity, IntegrationTint, SignInRoute, TileMark,
};
use forge_types::IntegrationId;

use crate::vocabulary::INTEGRATION_ID;

pub const OBS_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static(INTEGRATION_ID),
    brand_name: "OBS Studio",
    category: IntegrationCategory::APPS,
    description_key: "integration_obs_description",
    connection: ConnectionAffordance::Connectable,
    sign_in: SignInRoute::CredentialForm,
    identity: IntegrationIdentity {
        icon: "broadcast",
        monogram: "O",
        tile_mark: TileMark::Glyph,
        tile_tint: IntegrationTint::Success,
        hero_tint: IntegrationTint::Success,
    },
    catalog_group: IntegrationCatalogGroup {
        label: CatalogLabel::Brand("OBS"),
        filter_label: CatalogLabel::Brand("OBS"),
        step_tint: IntegrationTint::TextSecondary,
        trigger_tint: IntegrationTint::AccentTeal,
    },
    disclaimer: None,
};
