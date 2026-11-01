use gpui::{
    App, Div, FontWeight, IntoElement, ParentElement, Pixels, Rgba, SharedString, Styled, div, px,
    relative,
};

use crate::cards::card::card;
use crate::palette::ForgePalette;
use crate::presentation::ActivePresentation;
use crate::tokens::{
    BORDER_THIN, Density, FONT_SM, FONT_XXS, Radius, Spacing, radius, scaled, spacing,
};

pub fn metric_card(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    sublabel: Option<impl Into<SharedString>>,
    sublabel_color: Option<Rgba>,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> impl IntoElement {
    let label_upper = SharedString::from(label.into().to_uppercase());

    let mut col = div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(
            div()
                .font_family(cx.mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(label_upper),
        )
        .child(
            div()
                .font_family(cx.body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .child(value.into()),
        );

    if let Some(sub) = sublabel {
        col = col.child(
            div()
                .font_family(cx.mono_family())
                .text_size(FONT_XXS)
                .text_color(sublabel_color.unwrap_or(palette.text_faint))
                .child(sub.into()),
        );
    }

    div()
        .p(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.elevated)
        .child(col)
}

pub fn stat_row(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    palette: &ForgePalette,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(label.into()),
        )
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .child(value.into()),
        )
}

const STAT_CARD_PAD_V: Pixels = px(10.0);
const STAT_CARD_PAD_H: Pixels = px(12.0);
const STAT_CARD_HINT_MT: Pixels = px(2.0);
const STAT_CARD_GROW: f32 = 1.0;

pub fn stat_card(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    value_color: Rgba,
    hint: impl Into<SharedString>,
    hint_color: Rgba,
    density: Density,
    cx: &App,
) -> impl IntoElement {
    let palette = &cx.palette();
    let body = div()
        .w_full()
        .flex()
        .flex_col()
        .child(
            div()
                .mb(spacing(Spacing::Xxs, density))
                .font_family(cx.mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(SharedString::from(label.into().to_uppercase())),
        )
        .child(
            div()
                .w_full()
                .truncate()
                .font_family(cx.body_family())
                .text_size(FONT_SM)
                .font_weight(FontWeight::MEDIUM)
                .text_color(value_color)
                .child(value.into()),
        )
        .child(
            div()
                .mt(STAT_CARD_HINT_MT)
                .font_family(cx.mono_family())
                .text_size(FONT_XXS)
                .text_color(hint_color)
                .child(hint.into()),
        );

    grow_cell(
        card(body, palette)
            .padding_xy(STAT_CARD_PAD_V, scaled(STAT_CARD_PAD_H, density))
            .full_width()
            .full_height(),
        STAT_CARD_GROW,
    )
}

pub fn grow_cell(el: impl IntoElement, grow: f32) -> Div {
    let mut cell = div().min_w(px(0.0)).child(el);
    let style = cell.style();
    style.flex_grow = Some(grow);
    style.flex_basis = Some(relative(0.0).into());
    cell
}
