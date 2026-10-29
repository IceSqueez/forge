use forge_platform_core::{ConnectionAffordance, IntegrationCategory, IntegrationDeclaration};
use forge_types::IntegrationId;

use crate::vocabulary;

pub const DISCORD_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static(vocabulary::INTEGRATION_ID),
    brand_name: "Discord",
    category: IntegrationCategory::COMMUNITY,
    description_key: "integration_discord_description",
    connection: ConnectionAffordance::Connectionless,
};
