use forge_platform_core::{
    CatalogLabel, ConnectionAffordance, IntegrationCatalogGroup, IntegrationCategory,
    IntegrationDeclaration, IntegrationIdentity, IntegrationTint, SignInRoute, TileMark,
};
use forge_types::IntegrationId;

pub const MIDI_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static(crate::vocabulary::INTEGRATION_ID),
    brand_name: "MIDI",
    category: IntegrationCategory::CONTROLS,
    description_key: "integration_midi_description",
    connection: ConnectionAffordance::Connectionless,
    sign_in: SignInRoute::NotNeeded,
    identity: IntegrationIdentity {
        icon: "piano",
        monogram: "M",
        tile_mark: TileMark::Glyph,
        tile_tint: IntegrationTint::Info,
        hero_tint: IntegrationTint::AccentTeal,
    },
    catalog_group: IntegrationCatalogGroup {
        label: CatalogLabel::Brand("MIDI"),
        filter_label: CatalogLabel::Brand("MIDI"),
        step_tint: IntegrationTint::Danger,
        trigger_tint: IntegrationTint::Danger,
    },
    disclaimer: None,
};
