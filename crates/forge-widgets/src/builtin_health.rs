use iced::{Alignment, Border, Element, Length, widget::container};

use crate::{
    palette::ForgePalette,
    tokens::{
        BORDER_THIN, Density, FONT_MD, FONT_XS, FontRole, Radius, Spacing, font, radius, spacing,
    },
};

pub fn builtin_health_grid<'a, Msg: 'a>(
    metrics: &[forge_platform_core::HealthMetric; 4],
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let gap = spacing(Spacing::Sm, Density::Cozy) as f32;

    iced::widget::Row::new()
        .spacing(gap)
        .push(container(health_metric_card(&metrics[0], palette)).width(Length::FillPortion(1)))
        .push(container(health_metric_card(&metrics[1], palette)).width(Length::FillPortion(1)))
        .push(container(health_metric_card(&metrics[2], palette)).width(Length::FillPortion(1)))
        .push(container(health_metric_card(&metrics[3], palette)).width(Length::FillPortion(1)))
        .into()
}

fn health_metric_card<'a, Msg: 'a>(
    metric: &forge_platform_core::HealthMetric,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    use forge_platform_core::HealthValue;

    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Md);
    let v_pad = spacing(Spacing::Sm, Density::Cozy);
    let h_pad = spacing(Spacing::Md, Density::Cozy);

    let cap_label = iced::widget::text(metric.label.to_uppercase())
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(palette.text_muted);

    let value_col: Element<'a, Msg> = match &metric.value {
        HealthValue::Status {
            label: val_label,
            active,
            detail,
        } => {
            let color = if *active {
                palette.success
            } else {
                palette.disabled
            };
            let dot = crate::status::status_dot(color, 7.0);
            let val_text = iced::widget::text(val_label.clone())
                .size(FONT_MD)
                .color(color);
            let value_row: Element<'a, Msg> = iced::widget::row![dot, val_text]
                .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
                .align_y(Alignment::Center)
                .into();
            if let Some(d) = detail {
                let detail_text = iced::widget::text(d.clone())
                    .font(font(FontRole::Monospace))
                    .size(FONT_XS)
                    .color(palette.text_faint);
                iced::widget::column![value_row, detail_text]
                    .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
                    .into()
            } else {
                value_row
            }
        }
        HealthValue::Text { primary, secondary } => {
            let primary_text = iced::widget::text(primary.clone())
                .size(FONT_MD)
                .color(palette.text_primary);
            if let Some(sec) = secondary {
                let sub = iced::widget::text(sec.clone())
                    .font(font(FontRole::Monospace))
                    .size(FONT_XS)
                    .color(palette.text_faint);
                iced::widget::column![primary_text, sub]
                    .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
                    .into()
            } else {
                iced::widget::column![primary_text].into()
            }
        }
        HealthValue::Pair { left, right } => iced::widget::text(format!("{left} · {right}"))
            .font(font(FontRole::Monospace))
            .size(FONT_MD)
            .color(palette.text_primary)
            .into(),
        HealthValue::Ratio {
            used,
            total,
            reset_hint,
        } => {
            let ratio_text = iced::widget::text(format!("{used} / {total}"))
                .size(FONT_MD)
                .color(palette.text_primary);
            if let Some(hint) = reset_hint {
                let hint_text = iced::widget::text(hint.clone())
                    .font(font(FontRole::Monospace))
                    .size(FONT_XS)
                    .color(palette.text_faint);
                iced::widget::column![ratio_text, hint_text]
                    .spacing(spacing(Spacing::Xs, Density::Cozy) as f32)
                    .into()
            } else {
                iced::widget::column![ratio_text].into()
            }
        }
    };

    let inner = iced::widget::column![cap_label, value_col]
        .spacing(spacing(Spacing::Xs, Density::Cozy) as f32);

    container(inner)
        .padding([v_pad, h_pad])
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: r.into(),
            },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn builtin_health_grid_renders_all_value_variants() {
        use forge_platform_core::{HealthMetric, HealthValue};

        let metrics: [HealthMetric; 4] = [
            HealthMetric {
                label: "Stream".to_owned(),
                value: HealthValue::Status {
                    label: "Live".to_owned(),
                    active: true,
                    detail: None,
                },
            },
            HealthMetric {
                label: "CPU / FPS".to_owned(),
                value: HealthValue::Pair {
                    left: "8.2%".to_owned(),
                    right: "60.0".to_owned(),
                },
            },
            HealthMetric {
                label: "Dropped".to_owned(),
                value: HealthValue::Text {
                    primary: "0 frames".to_owned(),
                    secondary: Some("0.00%".to_owned()),
                },
            },
            HealthMetric {
                label: "API Calls".to_owned(),
                value: HealthValue::Ratio {
                    used: 142,
                    total: 800,
                    reset_hint: Some("resets in 47s".to_owned()),
                },
            },
        ];

        let _: Element<'_, ()> = builtin_health_grid(&metrics, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn builtin_health_grid_renders_inactive_status_and_bare_text() {
        use forge_platform_core::{HealthMetric, HealthValue};

        let metrics: [HealthMetric; 4] = [
            HealthMetric {
                label: "Recording".to_owned(),
                value: HealthValue::Status {
                    label: "Off".to_owned(),
                    active: false,
                    detail: None,
                },
            },
            HealthMetric {
                label: "Viewers".to_owned(),
                value: HealthValue::Text {
                    primary: "1,284".to_owned(),
                    secondary: None,
                },
            },
            HealthMetric {
                label: "EventSub".to_owned(),
                value: HealthValue::Status {
                    label: "11 subs".to_owned(),
                    active: true,
                    detail: Some("WebSocket".to_owned()),
                },
            },
            HealthMetric {
                label: "Quota".to_owned(),
                value: HealthValue::Ratio {
                    used: 142,
                    total: 10_000,
                    reset_hint: None,
                },
            },
        ];

        let _: Element<'_, ()> = builtin_health_grid(&metrics, &CATPPUCCIN_MOCHA);
    }
}
