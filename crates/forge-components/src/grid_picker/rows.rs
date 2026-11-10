use gpui::{
    AnyElement, App, ClickEvent, Context, Div, FontWeight, MouseButton, MouseDownEvent, Rgba,
    SharedString, Stateful, div, prelude::*, px, relative,
};

use super::{
    CardData, GRID_BADGE_FS, GRID_CARD_GAP, GRID_CARD_ICON, GRID_CARD_NAME_FS, GRID_CARD_PAD_H,
    GRID_CARD_PAD_V, GRID_CARD_ROW_MB, GRID_CARD_TILE_RADIUS, GRID_GROUP_DOT, GRID_GROUP_FS,
    GRID_HEADER_ROW_H, GRID_META_FS, GRID_SECTION_DOT, GRID_SECTION_FS, GRID_SECTION_ROW_H,
    GRID_STAR_PAD, GridPicker, GridPickerArt, GridPickerItemState,
};
use crate::glyph::glyph_art;
use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::presentation::ActivePresentation;
use crate::status::badge;
use crate::tokens::{
    BORDER_THIN, DISABLED_OPACITY, Density, Radius, Spacing, radius, scaled, spacing,
};
use crate::tr;

pub(super) fn render_header_row(
    label: &SharedString,
    dot: Rgba,
    count: usize,
    p: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let inner = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .child(
            div()
                .flex_none()
                .size(GRID_GROUP_DOT)
                .rounded(radius(Radius::Pill))
                .bg(dot),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(cx.mono_family())
                .text_size(GRID_GROUP_FS)
                .text_color(p.text_muted)
                .child(label.to_uppercase()),
        )
        .child(
            div()
                .font_family(cx.mono_family())
                .text_size(GRID_GROUP_FS)
                .text_color(p.text_faint)
                .child(count.to_string()),
        );

    div()
        .h(GRID_HEADER_ROW_H)
        .w_full()
        .flex()
        .items_end()
        .pb(spacing(Spacing::Xs, density))
        .child(inner)
        .into_any_element()
}

pub(super) fn render_section_row(
    label: &SharedString,
    dot: Rgba,
    count: usize,
    p: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let inner = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .pb(spacing(Spacing::Xxs, density))
        .border_b(BORDER_THIN)
        .border_color(p.surface_overlay)
        .child(
            div()
                .flex_none()
                .size(GRID_SECTION_DOT)
                .rounded(radius(Radius::Pill))
                .bg(dot),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .font_family(cx.body_family())
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(GRID_SECTION_FS)
                .text_color(dot)
                .child(label.clone()),
        )
        .child(
            div()
                .font_family(cx.mono_family())
                .text_size(GRID_GROUP_FS)
                .text_color(p.text_faint)
                .child(count.to_string()),
        );

    div()
        .h(GRID_SECTION_ROW_H)
        .w_full()
        .flex()
        .items_end()
        .pb(spacing(Spacing::Xs, density))
        .child(inner)
        .into_any_element()
}

pub(super) fn render_card_row(
    cards: &[Option<CardData>; 2],
    accent: Rgba,
    art: GridPickerArt,
    p: &ForgePalette,
    cx: &mut Context<GridPicker>,
) -> AnyElement {
    let row = div()
        .h(art.row_height())
        .w_full()
        .flex()
        .items_start()
        .gap(GRID_CARD_GAP);
    match cards {
        [Some(left), Some(right)] => row
            .child(render_card_el(left, accent, art, p, cx))
            .child(render_card_el(right, accent, art, p, cx)),
        [Some(left), None] => row.child(
            div()
                .w(relative(0.5))
                .flex()
                .child(render_card_el(left, accent, art, p, cx)),
        ),
        _ => row,
    }
    .into_any_element()
}

fn render_card_el(
    card: &CardData,
    accent: Rgba,
    art: GridPickerArt,
    p: &ForgePalette,
    cx: &mut Context<GridPicker>,
) -> AnyElement {
    let density = cx.density();
    let dim = !matches!(card.state, GridPickerItemState::Normal);

    let top = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .pb(GRID_CARD_ROW_MB)
        .child(card_tile(card, art, p))
        .child(card_name(card, p, cx))
        .child(card_star(card, accent, p, cx))
        .children(card_disabled_badge(card, p));

    let edge = if card.cursored {
        accent
    } else {
        p.border_regular
    };
    let base = div()
        .flex_1()
        .min_w(px(0.0))
        .h(art.card_height())
        .flex()
        .flex_col()
        .py(GRID_CARD_PAD_V)
        .px(scaled(GRID_CARD_PAD_H, density))
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(edge)
        .bg(p.shell)
        .child(top)
        .child(card_desc(card, p, cx));

    let base = if dim {
        base.opacity(DISABLED_OPACITY)
    } else {
        base
    };
    base.id(card.id.clone())
        .cursor_pointer()
        .hover(move |s| s.border_color(accent))
        .on_click(cx.listener({
            let id = card.id.clone();
            move |this, _: &ClickEvent, _, cx| this.emit_picked(id.clone(), cx)
        }))
        .into_any_element()
}

fn card_tile(card: &CardData, art: GridPickerArt, p: &ForgePalette) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(art.tile)
        .rounded(GRID_CARD_TILE_RADIUS)
        .bg(p.surface_overlay)
        .child(glyph_art(
            &card.glyph,
            art.glyph,
            card.tint,
            SharedString::from(format!("forge-grid-art-{}", card.id)),
        ))
}

fn card_name(card: &CardData, p: &ForgePalette, cx: &App) -> Div {
    div()
        .flex_1()
        .min_w(px(0.0))
        .truncate()
        .font_family(cx.body_family())
        .font_weight(FontWeight::MEDIUM)
        .text_size(GRID_CARD_NAME_FS)
        .text_color(p.text_primary)
        .child(card.name.clone())
}

fn card_star(
    card: &CardData,
    accent: Rgba,
    p: &ForgePalette,
    cx: &mut Context<GridPicker>,
) -> Stateful<Div> {
    let star_glyph = if card.favorite {
        Icon::StarFilled
    } else {
        Icon::Star
    };
    let star_tint = if card.favorite { accent } else { p.text_faint };
    let star = div()
        .id(SharedString::from(format!("forge-grid-star-{}", card.id)))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .p(GRID_STAR_PAD)
        .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
            cx.stop_propagation()
        })
        .child(icon(star_glyph, GRID_CARD_ICON, star_tint));
    if card.favorite_editable {
        star.cursor_pointer().on_click(cx.listener({
            let id = card.id.clone();
            move |this, _: &ClickEvent, _, cx| this.toggle_favorite(id.clone(), cx)
        }))
    } else {
        star.opacity(DISABLED_OPACITY)
    }
}

fn card_disabled_badge(card: &CardData, p: &ForgePalette) -> Option<AnyElement> {
    match card.state {
        GridPickerItemState::Disabled => Some(
            badge(
                p.surface_overlay,
                p.text_primary,
                tr!("widget_grid_picker_badge_off"),
                true,
                GRID_BADGE_FS,
            )
            .into_any_element(),
        ),
        GridPickerItemState::Normal => None,
    }
}

fn card_desc(card: &CardData, p: &ForgePalette, cx: &App) -> Div {
    div()
        .truncate()
        .w_full()
        .min_w(px(0.0))
        .font_family(cx.body_family())
        .text_size(GRID_META_FS)
        .text_color(p.text_muted)
        .child(card.desc.clone())
}
