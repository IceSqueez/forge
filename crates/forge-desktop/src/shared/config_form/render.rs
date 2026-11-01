use forge_components::{
    ActivePresentation, BORDER_THIN, Density, ForgePalette, Icon, KeyActivated, Radius, Spacing,
    dropdown, icon, radius, scaled, slider, spacing, toggle, tr,
};
use gpui::{
    AnyElement, App, ClickEvent, Entity, Pixels, SharedString, Window, div, prelude::*, px,
};

use super::{
    CHOICE_GLYPH, CHOICE_PAD_H, CHOICE_PAD_V, ConfigField, ConfigFieldHandlers, FILL_KEY_FS,
    FILL_KEY_W, FILL_ROW_PAD_H, FILL_ROW_PAD_V, FILL_VAL_FS, SLIDER_READOUT_W, SWATCH_RADIUS,
    SWATCH_RING, SWATCH_SIZE,
};
use crate::shared::option_choices::ChoiceOption;

pub(crate) fn render_config_control<V: 'static>(
    field: &ConfigField,
    palette: &ForgePalette,
    id_prefix: &str,
    view: &Entity<V>,
    handlers: &ConfigFieldHandlers<V>,
    density: Density,
    cx: &App,
) -> AnyElement {
    match field {
        ConfigField::Input { input, .. } => div().child(input.clone()).into_any_element(),
        ConfigField::Area { area, .. } => div().child(area.clone()).into_any_element(),
        ConfigField::Code { editor, .. } => div().child(editor.clone()).into_any_element(),
        ConfigField::Bool { key, value, .. } => {
            let toggle_key = key.clone();
            let view = view.clone();
            let on_toggle = handlers.toggle;
            toggle(*value, palette)
                .on_click(
                    SharedString::from(format!("{id_prefix}-{key}")),
                    move |_: &ClickEvent, _window: &mut Window, cx: &mut App| {
                        view.update(cx, |this, cx| on_toggle(this, toggle_key.clone(), cx));
                    },
                )
                .into_any_element()
        }
        ConfigField::Slide {
            key,
            min,
            max,
            unit,
            value,
            ..
        } => render_slide(
            key, *min, *max, unit, *value, palette, id_prefix, view, handlers, density, cx,
        ),
        ConfigField::Swatch {
            key,
            options,
            selected,
            ..
        } => render_swatch(
            key, options, selected, palette, id_prefix, view, handlers, density,
        ),
        ConfigField::Choice {
            key,
            options,
            selected,
            ..
        } => render_choice(
            key, options, selected, palette, id_prefix, view, handlers, density, cx,
        ),
        ConfigField::UnitAmount {
            unit_picker_key,
            units,
            unit,
            input,
            ..
        } => div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(div().flex_1().min_w(px(0.0)).child(input.clone()))
            .child(div().flex_1().min_w(px(0.0)).child(render_choice(
                unit_picker_key,
                units,
                unit,
                palette,
                id_prefix,
                view,
                handlers,
                density,
                cx,
            )))
            .into_any_element(),
        ConfigField::Hint { .. } => div()
            .italic()
            .font_family(cx.body_family())
            .text_size(FILL_VAL_FS)
            .text_color(palette.text_faint)
            .child(tr!("triggers_sheet_config_authored"))
            .into_any_element(),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "render helper takes each displayed value and handler separately"
)]
fn render_slide<V: 'static>(
    key: &str,
    min: i64,
    max: i64,
    unit: &str,
    value: i64,
    palette: &ForgePalette,
    id_prefix: &str,
    view: &Entity<V>,
    handlers: &ConfigFieldHandlers<V>,
    density: Density,
    cx: &App,
) -> AnyElement {
    let slide_key = key.to_owned();
    let view = view.clone();
    let on_slide = handlers.slide;

    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(
            div().flex_1().min_w(px(0.0)).child(
                slider(value as f32, min as f32, max as f32, palette)
                    .accent(palette.brand)
                    .whole_numbers()
                    .on_change(
                        SharedString::from(format!("{id_prefix}-{key}")),
                        move |next: &f32, _window: &mut Window, cx: &mut App| {
                            let stepped = next.round() as i64;
                            view.update(cx, |this, cx| {
                                on_slide(this, slide_key.clone(), stepped, cx)
                            });
                        },
                    ),
            ),
        )
        .child(
            div()
                .flex_none()
                .min_w(SLIDER_READOUT_W)
                .whitespace_nowrap()
                .font_family(cx.mono_family())
                .text_size(FILL_VAL_FS)
                .text_color(palette.text_secondary)
                .child(format!("{value}{unit}")),
        )
        .into_any_element()
}

#[expect(
    clippy::too_many_arguments,
    reason = "each argument is one input of the rendered control plus the active density"
)]
fn render_swatch<V: 'static>(
    key: &str,
    options: &[(String, gpui::Rgba)],
    selected: &str,
    palette: &ForgePalette,
    id_prefix: &str,
    view: &Entity<V>,
    handlers: &ConfigFieldHandlers<V>,
    density: Density,
) -> AnyElement {
    let mut row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(spacing(Spacing::Xs, density));

    for (name, tint) in options {
        let ring = if name == selected {
            palette.text_primary
        } else {
            palette.surface_overlay
        };
        let pick_key = key.to_owned();
        let pick_value = name.clone();
        let view = view.clone();
        let on_pick = handlers.pick;

        row = row.child(
            div()
                .id(SharedString::from(format!("{id_prefix}-{key}-{name}")))
                .size(SWATCH_SIZE)
                .rounded(SWATCH_RADIUS)
                .bg(*tint)
                .border(SWATCH_RING)
                .border_color(ring)
                .cursor_pointer()
                .key_activated(palette.text_primary)
                .on_click(move |_: &ClickEvent, _window: &mut Window, cx: &mut App| {
                    view.update(cx, |this, cx| {
                        on_pick(this, pick_key.clone(), pick_value.clone(), cx)
                    });
                }),
        );
    }

    row.into_any_element()
}

#[expect(
    clippy::too_many_arguments,
    reason = "each argument is one input of the rendered control plus the active density"
)]
fn render_choice<V: 'static>(
    key: &str,
    options: &[ChoiceOption],
    selected: &str,
    palette: &ForgePalette,
    id_prefix: &str,
    view: &Entity<V>,
    handlers: &ConfigFieldHandlers<V>,
    density: Density,
    cx: &App,
) -> AnyElement {
    let (display, tone) = match options.iter().find(|option| option.value == selected) {
        Some(option) => (option.label.clone(), palette.text_primary),
        None if !selected.is_empty() => (selected.to_owned(), palette.text_primary),
        None => (tr!("config_form_choice_placeholder"), palette.text_faint),
    };

    let mut trigger = div()
        .debug_selector(|| format!("{id_prefix}-{key}"))
        .id(SharedString::from(format!("{id_prefix}-{key}")))
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(spacing(Spacing::Xs, density))
        .py(CHOICE_PAD_V)
        .px(scaled(CHOICE_PAD_H, density))
        .rounded(radius(Radius::Sm))
        .border(BORDER_THIN)
        .border_color(palette.border_input)
        .bg(palette.shell)
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .font_family(cx.body_family())
                .text_size(FILL_VAL_FS)
                .text_color(tone)
                .child(display),
        )
        .child(icon(Icon::ChevronDown, CHOICE_GLYPH, palette.text_faint));

    let Some(choice) = handlers.choice.as_ref() else {
        return trigger.into_any_element();
    };
    let open_choice = choice.open;
    let open_key = key.to_owned();
    let open_view = view.clone();
    let hover_border = palette.brand;
    trigger = trigger
        .cursor_pointer()
        .hover(move |s| s.border_color(hover_border))
        .key_activated(palette.text_primary)
        .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            open_view.update(cx, |this, cx| {
                open_choice(this, open_key.clone(), window, cx)
            });
        });

    let close_choice = choice.close;
    let popover = choice
        .active
        .as_ref()
        .filter(|(active_key, _)| active_key == key)
        .map(|(_, picker)| {
            let view = view.clone();
            dropdown(picker.clone()).on_dismiss(move |_window, cx| {
                view.update(cx, close_choice);
            })
        });

    div()
        .relative()
        .w_full()
        .child(trigger)
        .children(popover)
        .into_any_element()
}

#[expect(
    clippy::too_many_arguments,
    reason = "each argument is one input of the rendered control plus the active density"
)]
pub(crate) fn render_config_row<V: 'static>(
    field: &ConfigField,
    label: SharedString,
    last: bool,
    palette: &ForgePalette,
    id_prefix: &str,
    view: &Entity<V>,
    handlers: &ConfigFieldHandlers<V>,
    density: Density,
    cx: &App,
) -> AnyElement {
    let label = config_label_cell(label, FILL_KEY_W, FILL_KEY_FS, palette.text_muted, cx);

    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .py(scaled(FILL_ROW_PAD_V, density))
        .px(scaled(FILL_ROW_PAD_H, density))
        .when(!last, |row| {
            row.border_b(BORDER_THIN)
                .border_color(palette.border_regular)
        })
        .child(label)
        .child(div().flex_1().min_w(px(0.0)).child(render_config_control(
            field, palette, id_prefix, view, handlers, density, cx,
        )))
        .into_any_element()
}

pub(crate) fn config_label_cell(
    label: SharedString,
    width: Pixels,
    size: Pixels,
    color: gpui::Rgba,
    cx: &App,
) -> gpui::Div {
    div()
        .w(width)
        .min_w(width)
        .max_w(width)
        .flex_none()
        .whitespace_normal()
        .font_family(cx.mono_family())
        .text_size(size)
        .text_color(color)
        .child(label)
}
