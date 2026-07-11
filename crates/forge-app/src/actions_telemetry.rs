use forge_storage::ActionTelemetry;
use forge_widgets::ForgePalette;
use forge_widgets::tokens::{Spacing, sp, spf};
use iced::{Color, Element, Length};
use time::OffsetDateTime;

pub fn format_relative_time(opt: Option<OffsetDateTime>) -> String {
    forge_widgets::fmt_relative_time(opt)
}

pub fn action_stat<'a, Msg: 'a>(
    label: &str,
    value: &str,
    value_color: Color,
    hint: Option<&str>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    use forge_widgets::FontRole;
    use forge_widgets::tokens::{FONT_SM, FONT_XXS};
    use iced::widget::{column, text};

    let p = *palette;
    let mono = forge_widgets::font(FontRole::Monospace);

    let label_el = text(label.to_owned())
        .size(FONT_XXS)
        .color(p.text_faint)
        .font(mono);

    let value_el = text(value.to_owned())
        .size(FONT_SM)
        .color(value_color)
        .font(mono);

    if let Some(hint_str) = hint {
        let hint_el = text(hint_str.to_owned())
            .size(FONT_XXS)
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

    let lbl_last = forge_widgets::tr!("telemetry_stat_last_fired");
    let lbl_runs = forge_widgets::tr!("telemetry_stat_runs_today");
    let lbl_avg = forge_widgets::tr!("telemetry_stat_avg_time");
    let lbl_errors = forge_widgets::tr!("telemetry_stat_errors_7d");

    let cells = row![
        container(action_stat(
            &lbl_last,
            &last_fired_val,
            p.text_primary,
            None,
            palette
        ))
        .width(Length::FillPortion(1)),
        container(action_stat(&lbl_runs, &runs_val, p.brand, None, palette))
            .width(Length::FillPortion(1)),
        container(action_stat(&lbl_avg, &avg_val, p.success, None, palette))
            .width(Length::FillPortion(1)),
        container(action_stat(
            &lbl_errors,
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
