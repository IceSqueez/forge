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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::HashSet;

    use forge_components::{ThemeId, accent_swatch};
    use forge_overlay::config::{ACCENT, ACCENT_OPTIONS};
    use forge_overlay::{OverlayConfig, register_builtin_kinds};
    use forge_storage::{OverlayCredential, OverlayId};
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
    fn every_preview_accent_gets_its_own_swatch_in_every_shipped_theme() {
        for theme in ThemeId::ALL {
            let palette = theme.palette();
            let swatches: Vec<Rgba> = ACCENTS
                .iter()
                .map(|accent| accent_color(*accent, &palette))
                .collect();

            for (index, swatch) in swatches.iter().enumerate() {
                for (other_index, other) in swatches.iter().enumerate().skip(index + 1) {
                    assert_ne!(
                        swatch, other,
                        "{:?}: {:?} and {:?} render the same swatch",
                        theme, ACCENTS[index], ACCENTS[other_index]
                    );
                }
            }
        }
    }

    /// The picker paints its dots from a name, the stage paints its composition from a
    /// `PreviewAccent`; two independent tables that must agree or the chosen dot lies about the
    /// color the overlay will actually render.
    #[test]
    fn the_picker_dot_and_the_rendered_stage_agree_on_the_color_of_every_offered_accent() {
        let registry = registry();
        let descriptor = registry
            .get("overlay.alert")
            .expect("the alert kind ships in this build");

        for theme in ThemeId::ALL {
            let palette = theme.palette();

            for name in ACCENT_OPTIONS {
                let config =
                    OverlayConfig::from([(ACCENT.to_owned(), Variant::String((*name).to_owned()))]);
                let rendered = accent_color(descriptor.preview(&config).accent, &palette);

                assert_eq!(
                    accent_swatch(name, &palette),
                    Some(rendered),
                    "{theme:?}: the '{name}' dot does not paint what the stage renders for it"
                );
            }
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
