use forge_components::{
    ActivePresentation, Density, ForgePalette, InputEvent, Spacing, TextInput, accent_swatch,
    ghost_button, secondary_button, spacing, tr,
};
use forge_overlay::config::{
    ACCENT, ACCENT_OPTIONS, BACKGROUND_COLOR, BACKGROUND_COLOR_OPTIONS, DEFAULT_BACKGROUND_COLOR,
};
use forge_overlay::hex_color_channels;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Rgba, SharedString, Subscription, Window, div,
    prelude::*, px, rgb,
};

use super::custom_accents::normalized_accent;
use super::property_panel::{AccentListEdit, OverlayPropertyPanel};
use crate::shared::config_form::{ConfigField, ConfigFieldHandlers, render_config_control};

const COLOR_INPUT_PLACEHOLDER: &str = "#ff8800";
const COLOR_FIELD_KEYS: [&str; 2] = [ACCENT, BACKGROUND_COLOR];

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

fn hex_tint(color: &str) -> Option<Rgba> {
    hex_color_channels(color)
        .map(|[red, green, blue]| rgb(u32::from_be_bytes([0, red, green, blue])))
}

fn preset_colors(key: &str, palette: &ForgePalette) -> Vec<(String, Rgba)> {
    let presets = if key == BACKGROUND_COLOR {
        BACKGROUND_COLOR_OPTIONS
    } else {
        ACCENT_OPTIONS
    };
    presets
        .iter()
        .map(|color| {
            let tint = hex_tint(color)
                .or_else(|| accent_swatch(color, palette))
                .unwrap_or(palette.text_faint);
            ((*color).to_owned(), tint)
        })
        .collect()
}

pub(super) fn offered_colors(
    key: &str,
    saved: &[String],
    selected: &str,
    palette: &ForgePalette,
) -> Vec<(String, Rgba)> {
    let mut offered = preset_colors(key, palette);
    let chosen = normalized_accent(selected).filter(|color| !saved.contains(color));
    for color in saved.iter().cloned().chain(chosen) {
        let listed = offered.iter().any(|(known, _)| *known == color);
        if let (false, Some(tint)) = (listed, hex_tint(&color)) {
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
        let palette = cx.palette();
        self.refresh_color_options(&palette);
        cx.notify();
    }

    pub(super) fn selected_color(&self, wanted: &str) -> Option<&str> {
        self.fields.iter().find_map(|field| match field {
            ConfigField::Swatch { key, selected, .. } if key == wanted => Some(selected.as_str()),
            _ => None,
        })
    }

    pub(super) fn refresh_color_options(&mut self, palette: &ForgePalette) {
        for field in &mut self.fields {
            if let ConfigField::Swatch {
                key,
                options,
                selected,
                ..
            } = field
                && COLOR_FIELD_KEYS.contains(&key.as_str())
            {
                *options = offered_colors(key, &self.colors.saved, selected, palette);
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

    pub(super) fn remove_selected_color(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(color) = self.selected_color(key).map(str::to_owned) else {
            return;
        };
        cx.emit(AccentListEdit::Remove(color));
    }

    pub(super) fn color_control(
        &self,
        field: &ConfigField,
        palette: &ForgePalette,
        view: &Entity<Self>,
        handlers: &ConfigFieldHandlers<Self>,
        density: Density,
        cx: &App,
    ) -> Option<AnyElement> {
        let entry = self.colors.entry(field.key())?;
        let key = entry.key;
        let swatches = render_config_control(
            field,
            palette,
            "overlays-panel",
            view,
            handlers,
            density,
            cx,
        );
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
        let removable = self
            .selected_color(key)
            .is_some_and(|selected| self.colors.is_saved(selected));
        let remove_view = view.clone();
        let remove = removable.then(|| {
            div().mt(spacing(Spacing::Xs, density)).child(
                ghost_button(tr!("overlays_accent_remove"), palette).on_click(
                    SharedString::from(format!("overlays-color-remove-{key}")),
                    move |_: &ClickEvent, _window: &mut Window, cx: &mut App| {
                        remove_view.update(cx, |this, cx| this.remove_selected_color(key, cx));
                    },
                ),
            )
        });
        Some(
            div()
                .flex()
                .flex_col()
                .child(swatches)
                .child(add_row)
                .children(remove)
                .into_any_element(),
        )
    }
}
