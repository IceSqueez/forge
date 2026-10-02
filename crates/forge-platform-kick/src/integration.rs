use forge_platform_core::{ConnectionAffordance, IntegrationCategory, IntegrationDeclaration};
use forge_types::IntegrationId;

pub const KICK_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static("kick"),
    brand_name: "Kick",
    category: IntegrationCategory::STREAMING,
    description_key: "integration_kick_description",
    connection: ConnectionAffordance::Connectable,
};
