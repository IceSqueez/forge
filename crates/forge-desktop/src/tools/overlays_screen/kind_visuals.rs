use forge_components::{ForgePalette, Icon};
use forge_overlay::{OverlayKindRegistry, PreviewAccent, effective_overlay_config};
use forge_storage::OverlayDefinition;
use forge_types::{MessageCode, MessageRef, NamesLanguage};
use gpui::{Rgba, rgb};

use crate::shared::user_text::resolve_message;

pub(super) struct KindVisuals {
    pub(super) icon: Icon,
    pub(super) accent: Rgba,
    pub(super) page_accent: Rgba,
    pub(super) label: Option<String>,
}

impl KindVisuals {
    pub(super) fn is_available(&self) -> bool {
        self.label.is_some()
    }
}

pub(super) fn accent_color(accent: PreviewAccent) -> Rgba {
    let [red, green, blue] = accent.channels();
    rgb(u32::from_be_bytes([0, red, green, blue]))
}

fn chrome_accent(accent: PreviewAccent, palette: &ForgePalette) -> Rgba {
    accent
        .name()
        .and_then(|name| forge_components::accent_swatch(name, palette))
        .unwrap_or_else(|| accent_color(accent))
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
            page_accent: palette.text_faint,
            label: None,
        };
    };
    let effective = effective_overlay_config(descriptor, &definition.config);
    let accent = descriptor
        .preview(&effective, NamesLanguage::default())
        .accent;
    KindVisuals {
        icon: Icon::from_name(descriptor.icon_name()),
        accent: chrome_accent(accent, palette),
        page_accent: accent_color(accent),
        label: Some(kind_text(descriptor.label())),
    }
}

pub(super) fn kind_text(code: MessageCode) -> String {
    resolve_message(&MessageRef::from(code)).inline()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use forge_components::{ThemeId, accent_swatch};
    use forge_overlay::config::{
        ACCENT, ACCENT_GREEN, ACCENT_GREEN_HEX, ACCENT_MAUVE, ACCENT_MAUVE_HEX, ACCENT_OPTIONS,
        ACCENT_PEACH, ACCENT_PEACH_HEX, ACCENT_RED, ACCENT_RED_HEX, ACCENT_SKY, ACCENT_SKY_HEX,
        ACCENT_YELLOW, ACCENT_YELLOW_HEX,
    };
    use forge_overlay::hex_color_channels;
    use forge_overlay::{OverlayConfig, register_builtin_kinds};
    use forge_storage::OverlayId;
    use forge_types::OverlayCredential;
    use forge_types::Variant;
    use time::OffsetDateTime;

    use super::*;

    const ACCENTS: [PreviewAccent; 6] = [
        PreviewAccent::Mauve,
        PreviewAccent::Sky,
        PreviewAccent::Green,
        PreviewAccent::Peach,
        PreviewAccent::Yellow,
        PreviewAccent::Red,
    ];

    fn registry() -> OverlayKindRegistry {
        let mut reg = OverlayKindRegistry::new();
        register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
        reg
    }

    fn definition(kind_id: &str) -> OverlayDefinition {
        OverlayDefinition {
            id: OverlayId::new("stage-alerts"),
            display_name: "Stage alerts".to_owned(),
            kind_id: kind_id.to_owned(),
            enabled: true,
            position: 0,
            config: Default::default(),
            config_schema_version: 1,
            generator_version: 1,
            source_overrides: Vec::new(),
            credential: OverlayCredential::new("token"),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn every_overlay_kind_names_an_icon_this_build_actually_carries() {
        let registry = registry();
        let mut seen: HashSet<Icon> = HashSet::new();

        for descriptor in registry.all() {
            let resolved = Icon::from_name(descriptor.icon_name());
            assert_ne!(
                resolved,
                Icon::InfoCircle,
                "{} asks for the '{}' icon, which falls back to the generic glyph",
                descriptor.id(),
                descriptor.icon_name()
            );
            assert!(
                seen.insert(resolved),
                "{} shares its glyph with an earlier kind, so the registry rows look alike",
                descriptor.id()
            );
        }
    }

    #[test]
    fn a_custom_accent_tints_the_chrome_with_its_own_color_in_every_shipped_theme() {
        let custom = PreviewAccent::Custom([0x10, 0x20, 0x30]);

        for theme in ThemeId::ALL {
            assert_eq!(
                chrome_accent(custom, &theme.palette()),
                accent_color(custom),
                "{theme:?}"
            );
        }
    }

    #[test]
    fn every_preview_accent_tints_the_chrome_with_its_own_theme_color() {
        for theme in ThemeId::ALL {
            let palette = theme.palette();
            let tints: Vec<Rgba> = ACCENTS
                .iter()
                .map(|accent| chrome_accent(*accent, &palette))
                .collect();

            for (index, tint) in tints.iter().enumerate() {
                for (other_index, other) in tints.iter().enumerate().skip(index + 1) {
                    assert_ne!(
                        tint, other,
                        "{theme:?}: {:?} and {:?} share a tint",
                        ACCENTS[index], ACCENTS[other_index]
                    );
                }
            }
        }
    }

    #[test]
    fn every_preview_accent_gets_its_own_color() {
        let colors: Vec<Rgba> = ACCENTS.iter().map(|accent| accent_color(*accent)).collect();

        for (index, color) in colors.iter().enumerate() {
            for (other_index, other) in colors.iter().enumerate().skip(index + 1) {
                assert_ne!(
                    color, other,
                    "{:?} and {:?} paint the same color",
                    ACCENTS[index], ACCENTS[other_index]
                );
            }
        }
    }

    #[test]
    fn the_stage_paints_the_hex_the_overlay_page_gets_for_every_offered_accent() {
        let registry = registry();
        let descriptor = registry
            .get("overlay.alert")
            .expect("the alert kind ships in this build");

        for (name, hex) in [
            (ACCENT_MAUVE, ACCENT_MAUVE_HEX),
            (ACCENT_SKY, ACCENT_SKY_HEX),
            (ACCENT_GREEN, ACCENT_GREEN_HEX),
            (ACCENT_PEACH, ACCENT_PEACH_HEX),
            (ACCENT_YELLOW, ACCENT_YELLOW_HEX),
            (ACCENT_RED, ACCENT_RED_HEX),
        ] {
            assert!(ACCENT_OPTIONS.contains(&name));
            let config =
                OverlayConfig::from([(ACCENT.to_owned(), Variant::String(name.to_owned()))]);
            let [red, green, blue] = hex_color_channels(hex).expect("a valid accent hex");

            assert_eq!(
                accent_color(descriptor.preview(&config, NamesLanguage::default()).accent),
                rgb(u32::from_be_bytes([0, red, green, blue])),
                "{name}"
            );
        }
    }

    #[test]
    fn an_accent_name_this_build_does_not_offer_has_no_dot_of_its_own() {
        let palette = ThemeId::default().palette();

        for unknown in ["teal", "Mauve", "", "brand", "mauve "] {
            assert_eq!(
                accent_swatch(unknown, &palette),
                None,
                "'{unknown}' is outside the offered vocabulary and must not borrow another meaning"
            );
        }
    }

    #[test]
    fn a_record_is_marked_unavailable_exactly_when_this_build_lacks_its_overlay_type() {
        crate::app::i18n::install_language(forge_storage::Language::En);
        let registry = registry();
        let palette = ThemeId::default().palette();

        for (kind_id, expected_label) in [
            ("overlay.alert", Some("Alert")),
            ("overlay.ticker", Some("Ticker")),
            ("overlay.written.by.a.newer.build", None),
            ("", None),
        ] {
            let visuals = kind_visuals(&definition(kind_id), &registry, &palette);

            assert_eq!(visuals.label.as_deref(), expected_label, "kind {kind_id:?}");
            assert_eq!(
                visuals.is_available(),
                expected_label.is_some(),
                "kind {kind_id:?}"
            );
        }
    }
}
