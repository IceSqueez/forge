use iced::{Alignment, Border, Element, Length, widget::container};

use crate::{
    palette::ForgePalette,
    tokens::{BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf},
};

pub fn builtin_health_grid<'a, Msg: 'a>(
    metrics: &[forge_platform_core::HealthMetric; 4],
    loading: bool,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let gap = spf(Spacing::Sm);

    let card = |idx: usize| -> Element<'a, Msg> {
        if loading {
            skeleton_health_card(palette)
        } else {
            health_metric_card(&metrics[idx], palette)
        }
    };

    iced::widget::Row::new()
        .spacing(gap)
        .push(container(card(0)).width(Length::FillPortion(1)))
        .push(container(card(1)).width(Length::FillPortion(1)))
        .push(container(card(2)).width(Length::FillPortion(1)))
        .push(container(card(3)).width(Length::FillPortion(1)))
        .into()
}

fn skeleton_health_card<'a, Msg: 'a>(palette: &ForgePalette) -> Element<'a, Msg> {
    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Md);

    let inner = iced::widget::column![
        crate::skeleton::skeleton(Length::Fixed(48.0), 10.0, palette),
        crate::skeleton::skeleton(Length::Fixed(72.0), FONT_SM, palette),
    ]
    .spacing(spf(Spacing::Xs));

    container(inner)
        .padding([sp(Spacing::Sm), sp(Spacing::Md)])
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

fn health_metric_card<'a, Msg: 'a>(
    metric: &forge_platform_core::HealthMetric,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    use forge_platform_core::HealthValue;

    let bg = palette.elevated;
    let border_color = palette.border_regular;
    let r = radius(Radius::Md);
    let v_pad = sp(Spacing::Sm);
    let h_pad = sp(Spacing::Md);

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
                .size(FONT_SM)
                .color(color);
            let value_row: Element<'a, Msg> = iced::widget::row![dot, val_text]
                .spacing(spf(Spacing::Xs))
                .align_y(Alignment::Center)
                .into();
            if let Some(d) = detail {
                let detail_text = iced::widget::text(d.clone())
                    .font(font(FontRole::Monospace))
                    .size(FONT_XS)
                    .color(palette.text_faint);
                iced::widget::column![value_row, detail_text]
                    .spacing(spf(Spacing::Xs))
                    .into()
            } else {
                value_row
            }
        }
        HealthValue::Text { primary, secondary } => {
            let primary_text = iced::widget::text(primary.clone())
                .size(FONT_SM)
                .color(palette.text_primary);
            if let Some(sec) = secondary {
                let sub = iced::widget::text(sec.clone())
                    .font(font(FontRole::Monospace))
                    .size(FONT_XS)
                    .color(palette.text_faint);
                iced::widget::column![primary_text, sub]
                    .spacing(spf(Spacing::Xs))
                    .into()
            } else {
                iced::widget::column![primary_text].into()
            }
        }
        HealthValue::Pair { left, right } => iced::widget::text(format!("{left} · {right}"))
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(palette.text_primary)
            .into(),
        HealthValue::Ratio {
            used,
            total,
            reset_hint,
        } => {
            let ratio_text = iced::widget::text(format!("{used} / {total}"))
                .size(FONT_SM)
                .color(palette.text_primary);
            if let Some(hint) = reset_hint {
                let hint_text = iced::widget::text(hint.clone())
                    .font(font(FontRole::Monospace))
                    .size(FONT_XS)
                    .color(palette.text_faint);
                iced::widget::column![ratio_text, hint_text]
                    .spacing(spf(Spacing::Xs))
                    .into()
            } else {
                iced::widget::column![ratio_text].into()
            }
        }
    };

    let inner = iced::widget::column![cap_label, value_col].spacing(spf(Spacing::Xs));

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
