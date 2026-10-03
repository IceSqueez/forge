use forge_platform_core::{ConnectionAffordance, IntegrationCategory, IntegrationDeclaration};
use forge_types::IntegrationId;

pub const DONATELLO_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static("donatello"),
    brand_name: "Donatello",
    category: IntegrationCategory::DONATIONS,
    description_key: "integration_donatello_description",
    connection: ConnectionAffordance::Connectable,
};
