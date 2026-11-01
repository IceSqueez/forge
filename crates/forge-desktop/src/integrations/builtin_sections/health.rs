use super::*;

pub(super) fn health_bar_section(
    bar: &HealthBar,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let level_color = health_level_color(&bar.level, palette);
    let fraction = bar.fraction.clamp(0.0, 1.0);

    let track = div()
        .flex_1()
        .min_w(px(0.0))
        .h(METER_H)
        .rounded(METER_RADIUS)
        .bg(palette.shell)
        .child(
            div()
                .h(METER_H)
                .w(relative(fraction))
                .rounded(METER_RADIUS)
                .bg(level_color),
        );

    let bar_row = div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(track)
        .child(mono(user_text_string(&bar.label), FONT_XS, level_color, cx));

    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .child(mono(
            tr!("widget_builtin_stream_health"),
            FONT_XS,
            palette.text_muted,
            cx,
        ))
        .child(bar_row)
        .into_any_element()
}
pub(crate) fn health_grid(
    metrics: &[HealthMetric; 4],
    loading: bool,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let mut row = div().w_full().flex().gap(spacing(Spacing::Sm, density));
    for metric in metrics.iter() {
        row = row.child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .child(health_metric_card(metric, loading, palette, density, cx)),
        );
    }
    row.into_any_element()
}

pub(super) fn health_metric_card(
    metric: &HealthMetric,
    loading: bool,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let value_col: AnyElement = if loading {
        body("-", FONT_SM, palette.text_faint, cx).into_any_element()
    } else {
        health_value_col(&metric.value, palette, density, cx)
    };

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .py(spacing(Spacing::Sm, density))
        .px(scaled(HEALTH_TILE_PAD_H, density))
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.elevated)
        .child(mono(
            health_metric_label(&metric.id).to_uppercase(),
            FONT_XXS,
            palette.text_muted,
            cx,
        ))
        .child(value_col)
        .into_any_element()
}

pub(super) fn health_value_col(
    value: &HealthValue,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    match value {
        HealthValue::Status {
            label,
            active,
            detail,
        } => {
            let color = if *active {
                palette.success
            } else {
                palette.disabled
            };
            let value_row = div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, density))
                .child(status_dot(color, ROW_DOT))
                .child(body(health_text(label), FONT_SM, color, cx));
            if let Some(d) = detail {
                div()
                    .flex()
                    .flex_col()
                    .gap(scaled(HEALTH_VALUE_HINT_GAP, density))
                    .child(value_row)
                    .child(mono(health_text(d), FONT_XXS, palette.text_faint, cx))
                    .into_any_element()
            } else {
                value_row.into_any_element()
            }
        }
        HealthValue::Text { primary, secondary } => {
            let primary_el = body(health_text(primary), FONT_SM, palette.text_primary, cx);
            if let Some(sec) = secondary {
                div()
                    .flex()
                    .flex_col()
                    .gap(scaled(HEALTH_VALUE_HINT_GAP, density))
                    .child(primary_el)
                    .child(mono(health_text(sec), FONT_XXS, palette.text_faint, cx))
                    .into_any_element()
            } else {
                primary_el.into_any_element()
            }
        }
        HealthValue::Pair { left, right } => mono(
            format!("{} \u{00b7} {}", health_text(left), health_text(right)),
            FONT_SM,
            palette.text_primary,
            cx,
        )
        .into_any_element(),
        HealthValue::Ratio {
            used,
            total,
            reset_hint,
        } => {
            let ratio_el = body(
                format!("{used} / {total}"),
                FONT_SM,
                palette.text_primary,
                cx,
            );
            if let Some(hint) = reset_hint {
                div()
                    .flex()
                    .flex_col()
                    .gap(scaled(HEALTH_VALUE_HINT_GAP, density))
                    .child(ratio_el)
                    .child(mono(health_text(hint), FONT_XXS, palette.text_faint, cx))
                    .into_any_element()
            } else {
                ratio_el.into_any_element()
            }
        }
    }
}
