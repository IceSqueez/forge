use forge_platform_core::{ConnectionAffordance, IntegrationCategory, IntegrationDeclaration};
use forge_types::IntegrationId;

pub const OBS_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static("obs"),
    brand_name: "OBS Studio",
    category: IntegrationCategory::APPS,
    description_key: "integration_obs_description",
    connection: ConnectionAffordance::Connectable,
};
