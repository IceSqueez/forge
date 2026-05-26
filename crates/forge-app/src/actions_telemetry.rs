use forge_storage::ActionTelemetry;
use forge_widgets::ForgePalette;
use forge_widgets::tokens::{Spacing, sp, spf};
use iced::{Color, Element, Length};
use time::OffsetDateTime;

pub fn format_relative_time(opt: Option<OffsetDateTime>) -> String {
    let Some(dt) = opt else {
        return "never".to_string();
    };
    let delta = OffsetDateTime::now_utc() - dt;
    let secs = delta.whole_seconds().max(0) as u64;
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

pub fn action_stat<'a, Msg: 'a>(
    label: &str,
    value: &str,
    value_color: Color,
    hint: Option<&str>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    use forge_widgets::FontRole;
    use forge_widgets::tokens::{FONT_SM, FONT_XS};
    use iced::widget::{column, text};

    let p = *palette;
    let mono = forge_widgets::font(FontRole::Monospace);

    let label_el = text(label.to_owned())
        .size(FONT_XS)
        .color(p.text_faint)
        .font(mono);

    let value_el = text(value.to_owned())
        .size(FONT_SM)
        .color(value_color)
        .font(mono);

    if let Some(hint_str) = hint {
        let hint_el = text(hint_str.to_owned())
            .size(FONT_XS)
            .color(p.text_muted)
            .font(mono);
        column![label_el, value_el, hint_el]
            .spacing(spf(Spacing::Xxs))
            .into()
    } else {
        column![label_el, value_el]
            .spacing(spf(Spacing::Xxs))
            .into()
    }
}

pub fn telemetry_grid<'a, Msg: 'a>(
    t: &ActionTelemetry,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    use forge_widgets::radius;
    use forge_widgets::tokens::Radius;
    use iced::widget::{container, row};

    let p = *palette;

    let last_fired_val = format_relative_time(t.last_fired_at);
    let runs_val = t.runs_today.to_string();
    let avg_val = t
        .avg_duration_ms
        .map(|ms| format!("{ms} ms"))
        .unwrap_or_else(|| "\u{2014}".to_string());
    let errors_val = t.errors_7d.to_string();
    let errors_color = if t.errors_7d > 0 { p.random } else { p.success };

    let cells = row![
        container(action_stat(
            "LAST FIRED",
            &last_fired_val,
            p.text_primary,
            None,
            palette
        ))
        .width(Length::FillPortion(1)),
        container(action_stat(
            "RUNS \u{00b7} TODAY",
            &runs_val,
            p.brand,
            None,
            palette
        ))
        .width(Length::FillPortion(1)),
        container(action_stat("AVG TIME", &avg_val, p.success, None, palette))
            .width(Length::FillPortion(1)),
        container(action_stat(
            "ERRORS \u{00b7} 7D",
            &errors_val,
            errors_color,
            None,
            palette
        ))
        .width(Length::FillPortion(1)),
    ]
    .spacing(spf(Spacing::Xs));

    container(cells)
        .width(Length::Fill)
        .padding([sp(Spacing::Md), sp(Spacing::Sm)])
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.shell)),
            border: iced::Border {
                color: p.border_input,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}
