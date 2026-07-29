use forge_components::{ForgePalette, Icon};
use forge_overlay::{OverlayKindRegistry, PreviewAccent, effective_overlay_config};
use forge_storage::OverlayDefinition;
use gpui::Rgba;

pub(super) struct KindVisuals {
    pub(super) icon: Icon,
    pub(super) accent: Rgba,
    /// `None` marks a record whose overlay type this build does not carry.
    pub(super) label: Option<String>,
}

impl KindVisuals {
    pub(super) fn is_available(&self) -> bool {
        self.label.is_some()
    }
}

pub(super) fn accent_color(accent: PreviewAccent, palette: &ForgePalette) -> Rgba {
    match accent {
        PreviewAccent::Mauve => palette.brand,
        PreviewAccent::Sky => palette.info,
        PreviewAccent::Green => palette.success,
        PreviewAccent::Peach => palette.bits,
        PreviewAccent::Yellow => palette.warning,
        PreviewAccent::Red => palette.random,
    }
}

pub(super) fn kind_visuals(
    definition: &OverlayDefinition,
    registry: &OverlayKindRegistry,
    palette: &ForgePalette,
) -> KindVisuals {
    let Some(descriptor) = registry.get(&definition.kind_id) else {
        return KindVisuals {
            icon: Icon::AlertTriangle,
            accent: palette.text_faint,
            label: None,
        };
    };
    let effective = effective_overlay_config(descriptor, &definition.config);
    KindVisuals {
        icon: Icon::from_name(descriptor.icon_name()),
        accent: accent_color(descriptor.preview(&effective).accent, palette),
        label: Some(descriptor.label().to_owned()),
    }
}
