use forge_platform_core::{ConnectionAffordance, IntegrationCategory, IntegrationDeclaration};
use forge_types::IntegrationId;

pub const YOUTUBE_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static("youtube"),
    brand_name: "YouTube",
    category: IntegrationCategory::STREAMING,
    description_key: "integration_youtube_description",
    connection: ConnectionAffordance::Connectable,
};
