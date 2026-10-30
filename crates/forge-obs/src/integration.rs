use forge_platform_core::{ConnectionAffordance, IntegrationCategory, IntegrationDeclaration};
use forge_types::IntegrationId;

use crate::vocabulary::INTEGRATION_ID;

pub const OBS_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static(INTEGRATION_ID),
    brand_name: "OBS Studio",
    category: IntegrationCategory::APPS,
    description_key: "integration_obs_description",
    connection: ConnectionAffordance::Connectable,
};
