use forge_platform_core::{ConnectionAffordance, IntegrationCategory, IntegrationDeclaration};
use forge_types::IntegrationId;

use crate::vocabulary;

pub const HOTKEY_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static(vocabulary::INTEGRATION_ID),
    brand_name: "Global hotkeys",
    category: IntegrationCategory::CONTROLS,
    description_key: "integration_hotkey_description",
    connection: ConnectionAffordance::Connectionless,
};
