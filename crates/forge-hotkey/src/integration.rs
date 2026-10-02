use forge_platform_core::{ConnectionAffordance, IntegrationCategory, IntegrationDeclaration};
use forge_types::IntegrationId;

pub const HOTKEY_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static("hotkey"),
    brand_name: "Global hotkeys",
    category: IntegrationCategory::DEVICES_AND_INPUT,
    description_key: "integration_hotkey_description",
    connection: ConnectionAffordance::Connectionless,
};
