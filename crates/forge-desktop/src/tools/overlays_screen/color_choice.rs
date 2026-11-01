use forge_components::{
    Density, ForgePalette, Icon, InputEvent, Radius, Spacing, TextInput, icon_button,
    secondary_button, spacing, tr,
};
use forge_overlay::config::{
    ACCENT, ACCENT_OPTIONS, BACKGROUND_COLOR, BACKGROUND_COLOR_OPTIONS, DEFAULT_BACKGROUND_COLOR,
};
use forge_overlay::{PreviewAccent, hex_color_channels};
use forge_storage::OverlayConfig;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, Rgba, SharedString, Subscription, Window,
    div, prelude::*, px,
};

use super::custom_accents::normalized_accent;
use super::kind_visuals::accent_color;
use super::property_panel::{AccentListEdit, OverlayPropertyPanel};
use crate::shared::config_form::{
    ConfigField, ConfigFieldHandlers, SwatchTile, read_text, swatch_row, swatch_tile,
};

const COLOR_INPUT_PLACEHOLDER: &str = "#ff8800";
const SWATCH_ID_PREFIX: &str = "overlays-panel";
const REMOVE_BADGE: Pixels = px(14.0);
const REMOVE_GLYPH: Pixels = px(10.0);
const REMOVE_BADGE_OVERHANG: Pixels = px(-5.0);
pub(super) const COLOR_FIELD_KEYS: [&str; 2] = [ACCENT, BACKGROUND_COLOR];

struct ColorEntry {
    key: &'static str,
    input: Entity<TextInput>,
    rejected: bool,
    _sub: Subscription,
}

pub(super) struct ColorChoices {
    saved: Vec<String>,
    entries: Vec<ColorEntry>,
}

impl ColorChoices {
    pub(super) fn new(
        saved: Vec<String>,
        fields: &[ConfigField],
        cx: &mut Context<OverlayPropertyPanel>,
    ) -> Self {
        let entries = COLOR_FIELD_KEYS
            .into_iter()
            .filter(|key| fields.iter().any(|field| field.key() == *key))
            .map(|key| ColorEntry::new(key, cx))
            .collect();
        Self { saved, entries }
    }

    fn entry(&self, key: &str) -> Option<&ColorEntry> {
        self.entries.iter().find(|entry| entry.key == key)
    }

    fn set_rejected(&mut self, key: &str, rejected: bool) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.key == key) {
            entry.rejected = rejected;
        }
    }

    pub(super) fn rejected(&self, key: &str) -> bool {
        self.entry(key).is_some_and(|entry| entry.rejected)
    }

    fn is_saved(&self, color: &str) -> bool {
        self.saved.iter().any(|saved| saved == color)
    }
}

impl ColorEntry {
    fn new(key: &'static str, cx: &mut Context<OverlayPropertyPanel>) -> Self {
        let input = cx.new(|cx| TextInput::new(COLOR_INPUT_PLACEHOLDER, cx));
        let sub = cx.subscribe(&input, move |panel, _input, event: &InputEvent, cx| {
            panel.on_color_input_event(key, event, cx);
        });
        Self {
            key,
            input,
            rejected: false,
            _sub: sub,
        }
    }
}

pub(super) fn stored_color_tint(color: &str) -> Rgba {
    accent_color(PreviewAccent::from_stored(color))
}

fn presets_of(key: &str) -> &'static [&'static str] {
    if key == BACKGROUND_COLOR {
        BACKGROUND_COLOR_OPTIONS
    } else {
        ACCENT_OPTIONS
    }
}

fn preset_colors(key: &str) -> Vec<(String, Rgba)> {
    presets_of(key)
        .iter()
        .map(|color| ((*color).to_owned(), stored_color_tint(color)))
        .collect()
}

pub(super) fn offered_colors(key: &str, saved: &[String], selected: &str) -> Vec<(String, Rgba)> {
    let mut offered = preset_colors(key);
    let chosen = normalized_accent(selected).filter(|color| !saved.contains(color));
    for color in saved.iter().cloned().chain(chosen) {
        let listed = offered.iter().any(|(known, _)| *known == color);
        if !listed && hex_color_channels(&color).is_some() {
            let tint = stored_color_tint(&color);
            offered.push((color, tint));
        }
    }
    offered
}

pub(super) fn background_choice(stored: &str) -> String {
    normalized_accent(stored).unwrap_or_else(|| DEFAULT_BACKGROUND_COLOR.to_owned())
}

pub(super) fn settle_background_choice(fields: &mut [ConfigField]) {
    for field in fields {
        if let ConfigField::Swatch { key, selected, .. } = field
            && key == BACKGROUND_COLOR
        {
            *selected = background_choice(selected);
        }
    }
}

impl OverlayPropertyPanel {
    pub(super) fn set_custom_accents(&mut self, colors: Vec<String>, cx: &mut Context<Self>) {
        self.colors.saved = colors;
        self.refresh_color_options();
        cx.notify();
    }

    pub(super) fn selected_color(&self, wanted: &str) -> Option<&str> {
        self.fields.iter().find_map(|field| match field {
            ConfigField::Swatch { key, selected, .. } if key == wanted => Some(selected.as_str()),
            _ => None,
        })
    }

    pub(super) fn refresh_color_options(&mut self) {
        for field in &mut self.fields {
            if let ConfigField::Swatch {
                key,
                options,
                selected,
                ..
            } = field
                && COLOR_FIELD_KEYS.contains(&key.as_str())
            {
                *options = offered_colors(key, &self.colors.saved, selected);
            }
        }
    }

    fn on_color_input_event(
        &mut self,
        key: &'static str,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Submitted(_) => self.add_typed_color(key, cx),
            _ => {
                self.colors.set_rejected(key, false);
                cx.notify();
            }
        }
    }

    pub(super) fn add_typed_color(&mut self, key: &'static str, cx: &mut Context<Self>) {
        let Some(input) = self.colors.entry(key).map(|entry| entry.input.clone()) else {
            return;
        };
        let Some(color) = normalized_accent(input.read(cx).content()) else {
            self.colors.set_rejected(key, true);
            cx.notify();
            return;
        };
        self.colors.set_rejected(key, false);
        input.update(cx, |input, cx| input.set_content("", cx));
        self.pick_swatch(key.to_owned(), color.clone(), cx);
        cx.emit(AccentListEdit::Add(color));
    }

    pub(super) fn remove_color(&mut self, key: &str, color: &str, cx: &mut Context<Self>) {
        if self.removable_color(key, color) {
            cx.emit(AccentListEdit::Remove(color.to_owned()));
        }
    }

    pub(super) fn adopt_color_reset(
        &mut self,
        stored: OverlayConfig,
        color: &str,
        cx: &mut Context<Self>,
    ) {
        for field in &mut self.fields {
            if let ConfigField::Swatch { key, selected, .. } = field
                && COLOR_FIELD_KEYS.contains(&key.as_str())
                && normalized_accent(selected).as_deref() == Some(color)
            {
                *selected = read_text(&self.defaults, key);
            }
        }
        self.stored = stored;
        self.refresh_color_options();
        cx.notify();
    }

    pub(super) fn removable_color(&self, key: &str, color: &str) -> bool {
        self.colors.is_saved(color) && !presets_of(key).contains(&color)
    }

    fn removable_swatch(
        &self,
        key: &'static str,
        color: &str,
        swatch: AnyElement,
        palette: &ForgePalette,
        view: &Entity<Self>,
    ) -> AnyElement {
        let group = SharedString::from(format!("overlays-color-swatch-{key}-{color}"));
        let view = view.clone();
        let picked = color.to_owned();
        let remove = icon_button(Icon::X, palette)
            .square(REMOVE_BADGE)
            .glyph_size(REMOVE_GLYPH)
            .corner(Radius::Pill)
            .fill(palette.elevated, palette.text_primary)
            .hover_fill(palette.danger)
            .tooltip(tr!("overlays_accent_remove"))
            .revealed_by(group.clone())
            .on_click(
                SharedString::from(format!("overlays-color-remove-{key}-{color}")),
                move |_: &ClickEvent, _window: &mut Window, cx: &mut App| {
                    view.update(cx, |this, cx| this.remove_color(key, &picked, cx));
                },
            );
        div()
            .relative()
            .group(group)
            .child(swatch)
            .child(
                div()
                    .absolute()
                    .top(REMOVE_BADGE_OVERHANG)
                    .right(REMOVE_BADGE_OVERHANG)
                    .child(remove),
            )
            .into_any_element()
    }

    pub(super) fn color_control(
        &self,
        field: &ConfigField,
        palette: &ForgePalette,
        view: &Entity<Self>,
        handlers: &ConfigFieldHandlers<Self>,
        density: Density,
    ) -> Option<AnyElement> {
        let entry = self.colors.entry(field.key())?;
        let key = entry.key;
        let ConfigField::Swatch {
            options, selected, ..
        } = field
        else {
            return None;
        };
        let swatches = options
            .iter()
            .fold(swatch_row(density), |row, (name, tint)| {
                let swatch = swatch_tile(
                    SwatchTile {
                        key,
                        name,
                        tint: *tint,
                        selected: name == selected,
                        id_prefix: SWATCH_ID_PREFIX,
                    },
                    palette,
                    view,
                    handlers.pick,
                )
                .into_any_element();
                row.child(if self.removable_color(key, name) {
                    self.removable_swatch(key, name, swatch, palette, view)
                } else {
                    swatch
                })
            });
        let add_view = view.clone();
        let add = secondary_button(tr!("overlays_accent_add"), palette).on_click(
            SharedString::from(format!("overlays-color-add-{key}")),
            move |_: &ClickEvent, _window: &mut Window, cx: &mut App| {
                add_view.update(cx, |this, cx| this.add_typed_color(key, cx));
            },
        );
        let add_row = div()
            .mt(spacing(Spacing::Xs, density))
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(div().flex_1().min_w(px(0.0)).child(entry.input.clone()))
            .child(add);
        Some(
            div()
                .flex()
                .flex_col()
                .child(swatches)
                .child(add_row)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use forge_overlay::config::{
        ACCENT_GREEN, ACCENT_GREEN_HEX, ACCENT_MAUVE, ACCENT_MAUVE_HEX, ACCENT_PEACH,
        ACCENT_PEACH_HEX, ACCENT_RED, ACCENT_RED_HEX, ACCENT_SKY, ACCENT_SKY_HEX, ACCENT_YELLOW,
        ACCENT_YELLOW_HEX,
    };

    use super::*;

    const CUSTOM: &str = "#123456";
    const OTHER_CUSTOM: &str = "#abcdef";

    fn names(offered: &[(String, Rgba)]) -> Vec<&str> {
        offered.iter().map(|(name, _)| name.as_str()).collect()
    }

    fn offered(key: &str, saved: &[&str], selected: &str) -> Vec<(String, Rgba)> {
        let saved: Vec<String> = saved.iter().map(|color| (*color).to_owned()).collect();
        offered_colors(key, &saved, selected)
    }

    fn extended(presets: &[&'static str], extra: &[&'static str]) -> Vec<&'static str> {
        presets.iter().chain(extra).copied().collect()
    }

    fn swatch(key: &str, selected: &str) -> ConfigField {
        ConfigField::Swatch {
            key: key.to_owned(),
            gate: None,
            options: Vec::new(),
            selected: selected.to_owned(),
        }
    }

    fn selected_of(field: &ConfigField) -> Option<&str> {
        match field {
            ConfigField::Swatch { selected, .. } => Some(selected),
            _ => None,
        }
    }

    #[test]
    fn each_color_field_starts_from_its_own_preset_list() {
        assert_eq!(
            names(&offered(ACCENT, &[], ACCENT_MAUVE)),
            ACCENT_OPTIONS.to_vec()
        );
        assert_eq!(
            names(&offered(BACKGROUND_COLOR, &[], DEFAULT_BACKGROUND_COLOR)),
            BACKGROUND_COLOR_OPTIONS.to_vec()
        );
    }

    fn tint_of(offered: &[(String, Rgba)], color: &str) -> Option<Rgba> {
        offered
            .iter()
            .find(|(name, _)| name == color)
            .map(|(_, tint)| *tint)
    }

    #[test]
    fn a_named_accent_and_its_background_hex_paint_the_same_color() {
        let accents = offered(ACCENT, &[], ACCENT_MAUVE);
        let backgrounds = offered(BACKGROUND_COLOR, &[], DEFAULT_BACKGROUND_COLOR);

        for (name, hex) in [
            (ACCENT_MAUVE, ACCENT_MAUVE_HEX),
            (ACCENT_SKY, ACCENT_SKY_HEX),
            (ACCENT_GREEN, ACCENT_GREEN_HEX),
            (ACCENT_PEACH, ACCENT_PEACH_HEX),
            (ACCENT_YELLOW, ACCENT_YELLOW_HEX),
            (ACCENT_RED, ACCENT_RED_HEX),
        ] {
            let accent = tint_of(&accents, name);
            assert!(accent.is_some(), "{name}");
            assert_eq!(accent, tint_of(&backgrounds, hex), "{name}");
            assert_eq!(accent, Some(stored_color_tint(hex)), "{name}");
        }
    }

    #[test]
    fn saved_colors_follow_the_presets_in_saved_order() {
        assert_eq!(
            names(&offered(ACCENT, &[OTHER_CUSTOM, CUSTOM], ACCENT_MAUVE)),
            extended(ACCENT_OPTIONS, &[OTHER_CUSTOM, CUSTOM])
        );
    }

    #[test]
    fn a_saved_color_equal_to_a_preset_is_offered_once() {
        assert_eq!(
            names(&offered(
                BACKGROUND_COLOR,
                &[ACCENT_MAUVE_HEX, CUSTOM],
                DEFAULT_BACKGROUND_COLOR
            )),
            extended(BACKGROUND_COLOR_OPTIONS, &[CUSTOM])
        );
    }

    #[test]
    fn an_unsaved_selection_is_offered_last_in_its_normalized_form() {
        assert_eq!(
            names(&offered(ACCENT, &[CUSTOM], "  #ABCDEF ")),
            extended(ACCENT_OPTIONS, &[CUSTOM, OTHER_CUSTOM])
        );
    }

    #[test]
    fn a_saved_selection_is_not_offered_twice() {
        assert_eq!(
            names(&offered(ACCENT, &[CUSTOM], CUSTOM)),
            extended(ACCENT_OPTIONS, &[CUSTOM])
        );
    }

    #[test]
    fn colors_that_cannot_be_tinted_are_never_offered() {
        for (saved, selected) in [
            ("not-a-color", ACCENT_MAUVE),
            ("#12345", "#12345"),
            ("", ""),
        ] {
            assert_eq!(
                names(&offered(ACCENT, &[saved], selected)),
                ACCENT_OPTIONS.to_vec(),
                "{saved:?} with {selected:?} selected"
            );
        }
    }

    #[test]
    fn a_stored_background_keeps_its_hex_in_normalized_form() {
        assert_eq!(background_choice(" #ABCDEF "), OTHER_CUSTOM);
    }

    #[test]
    fn anything_but_a_hex_color_falls_back_to_the_default_background() {
        for stored in ["", ACCENT_MAUVE, "#12345", "#1234567", "#gggggg"] {
            assert_eq!(
                background_choice(stored),
                DEFAULT_BACKGROUND_COLOR,
                "{stored:?}"
            );
        }
    }

    #[test]
    fn settling_replaces_only_the_background_selection() {
        let mut fields = [
            swatch(ACCENT, ACCENT_MAUVE),
            swatch(BACKGROUND_COLOR, ACCENT_MAUVE),
        ];

        settle_background_choice(&mut fields);

        assert_eq!(selected_of(&fields[0]), Some(ACCENT_MAUVE));
        assert_eq!(selected_of(&fields[1]), Some(DEFAULT_BACKGROUND_COLOR));
    }
}

#[cfg(test)]
mod panel_tests {
    use std::collections::HashMap;

    use forge_overlay::config::{ACCENT_MAUVE, ACCENT_SKY};
    use forge_overlay::{ConfigSection, SectionedField};
    use forge_registry::FormField;
    use forge_types::Variant;
    use gpui::{AppContext, TestAppContext, VisualTestContext, size};
    use tokio::runtime::Runtime;

    use super::super::property_panel::tests::launch;
    use super::*;
    use crate::test_support::click_selector;

    const CUSTOM: &str = "#102030";
    const OTHER_CUSTOM: &str = "#405060";
    const CUSTOM_SWATCH: &str = "overlays-panel-accent-#102030";
    const CUSTOM_REMOVE: &str = "overlays-color-remove-accent-#102030";
    const CUSTOM_BACKGROUND_REMOVE: &str = "overlays-color-remove-background_color-#102030";
    const PRESET_REMOVE: &str = "overlays-color-remove-accent-mauve";
    const OFF_LIST_REMOVE: &str = "overlays-color-remove-accent-#405060";
    const PANEL_SIZE: gpui::Pixels = px(1600.0);

    fn color_specs() -> Vec<SectionedField> {
        vec![
            SectionedField {
                section: ConfigSection::Style,
                field: FormField::Swatch {
                    key: ACCENT,
                    label: "Accent",
                    options: ACCENT_OPTIONS,
                },
            },
            SectionedField {
                section: ConfigSection::Style,
                field: FormField::Swatch {
                    key: BACKGROUND_COLOR,
                    label: "Background color",
                    options: BACKGROUND_COLOR_OPTIONS,
                },
            },
        ]
    }

    fn colors_config(entries: &[(&str, &str)]) -> OverlayConfig {
        entries
            .iter()
            .map(|(key, color)| ((*key).to_owned(), Variant::String((*color).to_owned())))
            .collect()
    }

    struct Edits {
        removed: Vec<String>,
        _sub: Subscription,
    }

    struct Rig {
        panel: Entity<OverlayPropertyPanel>,
        edits: Entity<Edits>,
        _rt: Runtime,
    }

    fn open(cx: &mut TestAppContext, stored: OverlayConfig, colors: &[&str]) -> Rig {
        let defaults = colors_config(&[
            (ACCENT, ACCENT_MAUVE),
            (BACKGROUND_COLOR, DEFAULT_BACKGROUND_COLOR),
        ]);
        let (mut panel_launch, _repo, rt) =
            launch(cx, stored, color_specs(), defaults, HashMap::new());
        panel_launch.custom_accents = colors.iter().map(|color| (*color).to_owned()).collect();
        let panel = cx.new(|cx| OverlayPropertyPanel::new(panel_launch, cx));
        let edits = cx.new(|cx| Edits {
            removed: Vec::new(),
            _sub: cx.subscribe(&panel, |this: &mut Edits, _panel, event, _cx| {
                if let AccentListEdit::Remove(color) = event {
                    this.removed.push(color.clone());
                }
            }),
        });
        Rig {
            panel,
            edits,
            _rt: rt,
        }
    }

    impl Rig {
        fn remove(&self, key: &'static str, color: &str, cx: &mut TestAppContext) {
            self.panel
                .update(cx, |panel, cx| panel.remove_color(key, color, cx));
            cx.run_until_parked();
        }

        fn offers(&self, key: &str, color: &str, cx: &TestAppContext) -> bool {
            self.panel.read_with(cx, |panel, _| {
                panel.fields.iter().any(|field| match field {
                    ConfigField::Swatch {
                        key: field_key,
                        options,
                        ..
                    } => field_key == key && options.iter().any(|(known, _)| known == color),
                    _ => false,
                })
            })
        }

        fn selected(&self, key: &str, cx: &TestAppContext) -> String {
            self.panel.read_with(cx, |panel, _| {
                panel.selected_color(key).unwrap_or_default().to_owned()
            })
        }

        fn removable(&self, key: &str, color: &str, cx: &TestAppContext) -> bool {
            self.panel
                .read_with(cx, |panel, _| panel.removable_color(key, color))
        }

        fn removed(&self, cx: &TestAppContext) -> Vec<String> {
            self.edits.read_with(cx, |edits, _| edits.removed.clone())
        }
    }

    #[gpui::test]
    fn asking_to_remove_a_listed_color_changes_nothing_until_the_answer(cx: &mut TestAppContext) {
        let rig = open(cx, colors_config(&[(ACCENT, CUSTOM)]), &[CUSTOM]);

        rig.remove(ACCENT, CUSTOM, cx);

        assert_eq!(rig.removed(cx), vec![CUSTOM.to_owned()]);
        assert_eq!(rig.selected(ACCENT, cx), CUSTOM);
        assert!(rig.offers(ACCENT, CUSTOM, cx));
    }

    #[gpui::test]
    fn a_committed_reset_returns_both_fields_to_their_defaults_and_drops_the_swatches(
        cx: &mut TestAppContext,
    ) {
        let rig = open(
            cx,
            colors_config(&[(ACCENT, CUSTOM), (BACKGROUND_COLOR, CUSTOM)]),
            &[CUSTOM, OTHER_CUSTOM],
        );

        rig.panel.update(cx, |panel, cx| {
            panel.set_custom_accents(vec![OTHER_CUSTOM.to_owned()], cx);
            panel.adopt_color_reset(OverlayConfig::new(), CUSTOM, cx);
        });

        assert_eq!(rig.selected(ACCENT, cx), ACCENT_MAUVE);
        assert_eq!(rig.selected(BACKGROUND_COLOR, cx), DEFAULT_BACKGROUND_COLOR);
        assert!(!rig.offers(ACCENT, CUSTOM, cx));
        assert!(!rig.offers(BACKGROUND_COLOR, CUSTOM, cx));
        assert!(rig.offers(ACCENT, OTHER_CUSTOM, cx));
        rig.panel.read_with(cx, |panel, cx| {
            assert_eq!(panel.pending_config(cx), panel.stored);
        });
    }

    #[gpui::test]
    fn a_committed_reset_leaves_a_field_holding_another_color(cx: &mut TestAppContext) {
        let kept = colors_config(&[(BACKGROUND_COLOR, OTHER_CUSTOM)]);
        let rig = open(
            cx,
            colors_config(&[(ACCENT, CUSTOM), (BACKGROUND_COLOR, OTHER_CUSTOM)]),
            &[CUSTOM, OTHER_CUSTOM],
        );

        rig.panel
            .update(cx, |panel, cx| panel.adopt_color_reset(kept, CUSTOM, cx));

        assert_eq!(rig.selected(BACKGROUND_COLOR, cx), OTHER_CUSTOM);
    }

    #[gpui::test]
    fn a_color_off_the_list_stays_shown_until_another_one_is_picked(cx: &mut TestAppContext) {
        let rig = open(cx, colors_config(&[(ACCENT, CUSTOM)]), &[]);

        assert_eq!(rig.selected(ACCENT, cx), CUSTOM);
        assert!(rig.offers(ACCENT, CUSTOM, cx));
        assert!(!rig.removable(ACCENT, CUSTOM, cx));

        rig.panel.update(cx, |panel, cx| {
            panel.pick_swatch(ACCENT.to_owned(), ACCENT_SKY.to_owned(), cx);
        });

        assert!(!rig.offers(ACCENT, CUSTOM, cx));
    }

    #[gpui::test]
    fn remove_asks_nothing_for_a_color_off_the_list_or_a_preset(cx: &mut TestAppContext) {
        let rig = open(cx, colors_config(&[(ACCENT, CUSTOM)]), &[OTHER_CUSTOM]);

        rig.remove(ACCENT, CUSTOM, cx);
        rig.remove(ACCENT, ACCENT_MAUVE, cx);
        rig.remove(BACKGROUND_COLOR, DEFAULT_BACKGROUND_COLOR, cx);

        assert!(rig.removed(cx).is_empty());
    }

    #[gpui::test]
    fn a_listed_color_is_removable_in_both_fields_while_neither_selects_it(
        cx: &mut TestAppContext,
    ) {
        let rig = open(cx, colors_config(&[(ACCENT, ACCENT_SKY)]), &[CUSTOM]);

        assert!(rig.removable(ACCENT, CUSTOM, cx));
        assert!(rig.removable(BACKGROUND_COLOR, CUSTOM, cx));
    }

    #[gpui::test]
    fn a_listed_color_equal_to_a_preset_is_a_preset_in_that_field(cx: &mut TestAppContext) {
        let rig = open(cx, OverlayConfig::new(), &[DEFAULT_BACKGROUND_COLOR]);

        assert!(rig.removable(ACCENT, DEFAULT_BACKGROUND_COLOR, cx));
        assert!(!rig.removable(BACKGROUND_COLOR, DEFAULT_BACKGROUND_COLOR, cx));
    }

    struct Shown<'a> {
        panel: Entity<OverlayPropertyPanel>,
        edits: Entity<Edits>,
        vcx: &'a mut VisualTestContext,
        _rt: Runtime,
    }

    fn show<'a>(cx: &'a mut TestAppContext, stored: OverlayConfig, colors: &[&str]) -> Shown<'a> {
        let defaults = colors_config(&[
            (ACCENT, ACCENT_MAUVE),
            (BACKGROUND_COLOR, DEFAULT_BACKGROUND_COLOR),
        ]);
        let (mut panel_launch, _repo, rt) =
            launch(cx, stored, color_specs(), defaults, HashMap::new());
        panel_launch.custom_accents = colors.iter().map(|color| (*color).to_owned()).collect();
        let (panel, vcx) =
            cx.add_window_view(|_window, cx| OverlayPropertyPanel::new(panel_launch, cx));
        vcx.simulate_resize(size(PANEL_SIZE, PANEL_SIZE));
        let edits = vcx.update(|_window, cx| {
            cx.new(|cx| Edits {
                removed: Vec::new(),
                _sub: cx.subscribe(&panel, |this: &mut Edits, _panel, event, _cx| {
                    if let AccentListEdit::Remove(color) = event {
                        this.removed.push(color.clone());
                    }
                }),
            })
        });
        vcx.run_until_parked();
        Shown {
            panel,
            edits,
            vcx,
            _rt: rt,
        }
    }

    impl Shown<'_> {
        fn removed(&mut self) -> Vec<String> {
            let edits = self.edits.clone();
            self.vcx
                .update(|_window, cx| edits.read(cx).removed.clone())
        }

        fn selected(&mut self, key: &str) -> String {
            let panel = self.panel.clone();
            self.vcx.update(|_window, cx| {
                panel
                    .read(cx)
                    .selected_color(key)
                    .unwrap_or_default()
                    .to_owned()
            })
        }
    }

    #[gpui::test]
    fn clicking_the_remove_control_asks_to_remove_without_selecting_the_swatch(
        cx: &mut TestAppContext,
    ) {
        let mut shown = show(cx, colors_config(&[(ACCENT, ACCENT_SKY)]), &[CUSTOM]);

        click_selector(shown.vcx, CUSTOM_REMOVE);
        shown.vcx.run_until_parked();

        assert_eq!(shown.removed(), vec![CUSTOM.to_owned()]);
        assert_eq!(shown.selected(ACCENT), ACCENT_SKY);
    }

    #[gpui::test]
    fn clicking_a_removable_swatch_selects_it_and_removes_nothing(cx: &mut TestAppContext) {
        let mut shown = show(cx, colors_config(&[(ACCENT, ACCENT_SKY)]), &[CUSTOM]);

        click_selector(shown.vcx, CUSTOM_SWATCH);
        shown.vcx.run_until_parked();

        assert_eq!(shown.selected(ACCENT), CUSTOM);
        assert!(shown.removed().is_empty());
    }

    #[gpui::test]
    fn the_remove_control_paints_on_listed_colors_in_both_rows_and_nowhere_else(
        cx: &mut TestAppContext,
    ) {
        let shown = show(cx, colors_config(&[(ACCENT, OTHER_CUSTOM)]), &[CUSTOM]);

        for (selector, painted) in [
            (CUSTOM_REMOVE, true),
            (CUSTOM_BACKGROUND_REMOVE, true),
            (PRESET_REMOVE, false),
            (OFF_LIST_REMOVE, false),
        ] {
            assert_eq!(
                shown.vcx.debug_bounds(selector).is_some(),
                painted,
                "{selector}"
            );
        }
    }
}
