use super::*;
use forge_components::{
    BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, Spacing, body_family,
    icon, radius, spacing, with_alpha,
};
use gpui::{AnyElement, App, ClickEvent, ElementId, Rgba, SharedString, Window, div};

pub(super) fn add_row_button(
    id: impl Into<ElementId>,
    glyph: Icon,
    label: impl Into<SharedString>,
    accent: Rgba,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let label = label.into();
    let hover = palette.surface_overlay;
    div()
        .id(id.into())
        .w_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .py(CARD_PAD_V)
        .px(CARD_PAD_H)
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_input)
        .bg(palette.shell)
        .cursor_pointer()
        .hover(move |s| s.bg(hover).border_color(accent))
        .on_click(handler)
        .child(icon(glyph, CARD_GLYPH, accent))
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(accent)
                .child(label),
        )
        .into_any_element()
}

pub(super) fn empty_placeholder_card(
    glyph: Icon,
    glyph_color: Rgba,
    label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> AnyElement {
    let label = label.into();
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .py(EMPTY_CARD_PAD_V)
        .px(EMPTY_CARD_PAD_H)
        .rounded(radius(Radius::Md))
        .border(HALF_BORDER)
        .border_color(palette.border_input)
        .child(icon(glyph, EMPTY_CARD_GLYPH, glyph_color))
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(label),
        )
        .into_any_element()
}

pub(super) fn inline_warning_card(
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
    palette: &ForgePalette,
) -> AnyElement {
    let title = title.into();
    let hint = hint.into();
    div()
        .w_full()
        .flex()
        .items_start()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .py(CARD_PAD_V)
        .px(CARD_PAD_H)
        .rounded(radius(Radius::Md))
        .bg(with_alpha(palette.warning, STEP_HEALTH_TILE_ALPHA))
        .child(icon(Icon::AlertTriangle, CARD_GLYPH, palette.warning))
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xxs, Density::Cozy))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_primary)
                        .child(title),
                )
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_faint)
                        .child(hint),
                ),
        )
        .into_any_element()
}
