use forge_platform_core::{ConnectionAffordance, IntegrationCategory, IntegrationDeclaration};
use forge_types::IntegrationId;

pub const MIDI_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static(crate::vocabulary::INTEGRATION_ID),
    brand_name: "MIDI",
    category: IntegrationCategory::CONTROLS,
    description_key: "integration_midi_description",
    connection: ConnectionAffordance::Connectionless,
};
