use crate::vocabulary::PLATFORM_ID;
use forge_platform_core::{ConnectionAffordance, IntegrationCategory, IntegrationDeclaration};
use forge_types::IntegrationId;

pub const TWITCH_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static(PLATFORM_ID),
    brand_name: "Twitch",
    category: IntegrationCategory::STREAMING,
    description_key: "integration_twitch_description",
    connection: ConnectionAffordance::Connectable,
};
