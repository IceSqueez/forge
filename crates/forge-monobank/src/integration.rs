use forge_platform_core::{ConnectionAffordance, IntegrationCategory, IntegrationDeclaration};
use forge_types::IntegrationId;

pub const MONOBANK_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static("monobank"),
    brand_name: "monobank",
    category: IntegrationCategory::DONATIONS,
    description_key: "integration_monobank_description",
    connection: ConnectionAffordance::Connectable,
};
