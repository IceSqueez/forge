use forge_components::ActivePresentation;
use forge_components::Spacing;
use forge_components::spacing;
use forge_components::{
    BORDER_THIN, Button, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, body_family,
    empty_state, ghost_button_with_icon, icon, mono_family, radius, tr,
};
use gpui::{AnyElement, App, ClickEvent, Entity, Pixels, SharedString, div, prelude::*, px};

use super::EventWiringView;
use super::plan::{FeedRow, feed_summary};

const GLYPH: Pixels = px(12.0);
const CHIP_PAD_V: Pixels = px(8.0);
const CHIP_GAP: Pixels = px(8.0);

pub(crate) fn entry_button(
    view: &Entity<EventWiringView>,
    id: &'static str,
    palette: &ForgePalette,
) -> Button {
    let target = view.clone();
    ghost_button_with_icon(Icon::Bolt, tr!("overlays_wire_open"), palette).on_click(
        id,
        move |_: &ClickEvent, window: &mut gpui::Window, cx: &mut App| {
            target.update(cx, |this, cx| this.open_picker(window, cx));
        },
    )
}

pub(crate) fn render_readout(
    view: &Entity<EventWiringView>,
    palette: &ForgePalette,
    cx: &mut App,
) -> Option<AnyElement> {
    let density = cx.density();
    let wiring = view.read(cx);
    if !wiring.eligible() {
        return None;
    }

    if wiring.feeds.is_empty() {
        return Some(
            div()
                .flex_none()
                .w_full()
                .pt(spacing(Spacing::Sm, density))
                .child(
                    empty_state(tr!("overlays_wire_empty"), palette)
                        .glyph(Icon::Bolt)
                        .density(Density::Compact)
                        .cta(entry_button(view, "overlays-wire-entry-empty", palette)),
                )
                .into_any_element(),
        );
    }

    let mut rows = div()
        .flex_none()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .pt(spacing(Spacing::Sm, density))
        .child(note_row(
            Icon::Bolt,
            palette.brand,
            feed_summary(&wiring.feeds),
            palette.text_secondary,
            density,
        ));

    for row in &wiring.feeds {
        rows = rows.child(feed_chip(view, row, palette, density));
    }

    let overlay_off = wiring
        .overlay
        .as_ref()
        .is_some_and(|definition| !definition.enabled);
    if overlay_off {
        rows = rows.child(note_row(
            Icon::AlertTriangle,
            palette.warning,
            tr!("overlays_wire_overlay_off"),
            palette.warning,
            density,
        ));
    }

    Some(rows.into_any_element())
}

fn note_row(
    glyph: Icon,
    tint: gpui::Rgba,
    message: String,
    text_color: gpui::Rgba,
    density: Density,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .child(icon(glyph, GLYPH, tint))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(text_color)
                .child(message),
        )
}

fn feed_chip(
    view: &Entity<EventWiringView>,
    row: &FeedRow,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    let target = view.clone();
    let action_id = row.action_id;
    let disabled_note = (!row.action_enabled).then(|| {
        div()
            .flex_none()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.warning)
            .child(tr!("overlays_wire_action_disabled").to_uppercase())
    });

    div()
        .id(SharedString::from(format!(
            "overlays-wire-feed-{action_id}"
        )))
        .w_full()
        .flex()
        .items_center()
        .gap(CHIP_GAP)
        .px(spacing(Spacing::Sm, density))
        .py(CHIP_PAD_V)
        .rounded(radius(Radius::Sm))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.shell)
        .cursor_pointer()
        .on_click(move |_: &ClickEvent, _window, cx: &mut App| {
            target.update(cx, |this, cx| this.open_action(action_id, cx));
        })
        .child(icon(Icon::Bolt, GLYPH, palette.brand))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_secondary)
                .child(row.action_name.clone()),
        )
        .children(disabled_note)
        .child(icon(Icon::ChevronRight, GLYPH, palette.text_faint))
}
