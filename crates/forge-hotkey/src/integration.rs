use forge_platform_core::{
    CatalogLabel, ConnectionAffordance, IntegrationCatalogGroup, IntegrationCategory,
    IntegrationDeclaration, IntegrationIdentity, IntegrationTint, SignInRoute, TileMark,
};
use forge_types::IntegrationId;

use crate::vocabulary;

pub const HOTKEY_INTEGRATION: IntegrationDeclaration = IntegrationDeclaration {
    id: IntegrationId::from_static(vocabulary::INTEGRATION_ID),
    brand_name: "Global hotkeys",
    category: IntegrationCategory::CONTROLS,
    description_key: "integration_hotkey_description",
    connection: ConnectionAffordance::Connectionless,
    sign_in: SignInRoute::NotNeeded,
    identity: IntegrationIdentity {
        icon: "keyboard",
        monogram: "H",
        tile_mark: TileMark::Glyph,
        tile_tint: IntegrationTint::Success,
        hero_tint: IntegrationTint::Info,
    },
    catalog_group: IntegrationCatalogGroup {
        label: CatalogLabel::Localized("triggers_filter_hotkey"),
        filter_label: CatalogLabel::Localized("triggers_filter_hotkey"),
        step_tint: IntegrationTint::Warning,
        trigger_tint: IntegrationTint::Warning,
    },
    connection_strip: None,
    disclaimer: None,
};
