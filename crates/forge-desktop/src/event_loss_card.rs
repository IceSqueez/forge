use forge_components::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, Spacing, body_family,
    card, icon, mono_family, spacing, status_dot, tr,
};
use forge_runtime::ConsumerLoss;
use gpui::{AnyElement, IntoElement, Pixels, Rgba, div, prelude::*, px};

use crate::event_loss::{EventLoss, breakdown, consumer_label, tier_label};

const HEADER_ICON: Pixels = px(14.0);
const ROW_DOT: Pixels = px(6.0);
const ROW_PAD_V: Pixels = px(5.0);
const TIER_W: Pixels = px(72.0);

fn row_color(row: &ConsumerLoss, palette: &ForgePalette) -> Rgba {
    if row.loss.priority_dropped > 0 {
        palette.random
    } else {
        palette.text_secondary
    }
}

fn loss_row(
    row: &ConsumerLoss,
    last: bool,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let color = row_color(row, palette);
    let total = row.loss.total();
    let mut line = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .py(ROW_PAD_V)
        .child(status_dot(color, ROW_DOT))
        .child(
            div()
                .flex_none()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(consumer_label(row.consumer)),
        )
        .child(
            div()
                .flex_none()
                .w(TIER_W)
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(tier_label(row.tier)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(breakdown(&row.loss)),
        )
        .child(
            div()
                .flex_none()
                .font_family(mono_family())
                .text_size(FONT_SM)
                .text_color(color)
                .child(total.to_string()),
        );
    if !last {
        line = line
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular);
    }
    line.into_any_element()
}

fn quiet_row(palette: &ForgePalette, density: Density) -> AnyElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .py(ROW_PAD_V)
        .child(status_dot(palette.success, ROW_DOT))
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(tr!("event_loss_none")),
        )
        .into_any_element()
}

pub fn event_loss_card(
    loss: &EventLoss,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement + use<> {
    let rows = loss.rows();
    let priority_lost = loss.priority_dropped() > 0;
    let header_color = if rows.is_empty() {
        palette.success
    } else if priority_lost {
        palette.random
    } else {
        palette.text_secondary
    };

    let header = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, density))
                .child(icon(Icon::AlertTriangle, HEADER_ICON, header_color))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_SM)
                        .text_color(palette.text_primary)
                        .child(tr!("event_loss_title")),
                ),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(tr!("event_loss_scope_hint")),
        );

    let mut body = div().w_full().flex().flex_col();
    if rows.is_empty() {
        body = body.child(quiet_row(palette, density));
    } else {
        let last = rows.len() - 1;
        for (index, row) in rows.iter().enumerate() {
            body = body.child(loss_row(row, index == last, palette, density));
        }
    }

    let content = div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .child(header)
        .child(body);

    card(content, palette)
        .full_width()
        .padding(spacing(Spacing::Sm, density))
}
