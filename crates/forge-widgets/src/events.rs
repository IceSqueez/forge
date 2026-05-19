use forge_events::EventSource;
use iced::{
    Background, Border, Color, Element, Length, Padding,
    widget::{button, column, container, row},
};

use crate::{
    palette::ForgePalette,
    tokens::{FONT_CAPS, FONT_CAPS_SM, FONT_CAPS_XS, FontRole, Radius, font, radius},
};

pub fn color_for_source(source: EventSource, palette: &ForgePalette) -> Color {
    match source {
        EventSource::Twitch => palette.brand,
        EventSource::YouTube => palette.random,
        EventSource::Kick => palette.info,
        EventSource::Trovo => palette.accent_pink_light,
        EventSource::Core => palette.warning,
        EventSource::Rhai => palette.warning,
        EventSource::Http => palette.random,
        EventSource::Obs => palette.success,
        EventSource::VTube => palette.accent_teal,
        EventSource::Discord => palette.brand,
        EventSource::Midi => palette.bits,
        EventSource::Hotkey => palette.bits,
        EventSource::Timer => palette.warning,
        EventSource::Server => palette.info,
    }
}

fn source_label(source: EventSource) -> &'static str {
    match source {
        EventSource::Twitch => "TWITCH",
        EventSource::YouTube => "YOUTUBE",
        EventSource::Kick => "KICK",
        EventSource::Trovo => "TROVO",
        EventSource::Core => "CORE",
        EventSource::Rhai => "RHAI",
        EventSource::Http => "HTTP",
        EventSource::Obs => "OBS",
        EventSource::VTube => "VTUBE",
        EventSource::Discord => "DISCORD",
        EventSource::Midi => "MIDI",
        EventSource::Hotkey => "HOTKEY",
        EventSource::Timer => "TIMER",
        EventSource::Server => "SERVER",
    }
}

pub fn source_badge<'a, Msg: 'a>(source: EventSource, palette: &ForgePalette) -> Element<'a, Msg> {
    let fg = color_for_source(source, palette);
    let bg = palette.surface_overlay;
    let label = source_label(source);

    let txt = iced::widget::text(label)
        .size(FONT_CAPS_XS)
        .color(fg)
        .font(iced::Font {
            family: iced::font::Family::Name("JetBrains Mono"),
            weight: iced::font::Weight::Medium,
            stretch: iced::font::Stretch::Normal,
            style: iced::font::Style::Normal,
        });

    container(txt)
        .padding([1, 5])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                radius: radius(Radius::Xs).into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

pub struct EventRowData<'a> {
    pub timestamp: &'a str,
    pub source: EventSource,
    pub event_type: &'a str,
    pub summary: &'a str,
    pub result_tag: Option<&'a str>,
    pub is_error: bool,
}

pub fn event_row_observability<'a, Msg: Clone + 'a>(
    event: &'a EventRowData<'a>,
    selected: bool,
    on_click: Msg,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let mono = font(FontRole::Monospace);

    let accent_color = if event.is_error {
        palette.random
    } else if selected {
        palette.brand
    } else {
        Color::TRANSPARENT
    };

    let bg_selected = palette.elevated;
    let bg_error = Color {
        r: palette.random.r,
        g: palette.random.g,
        b: palette.random.b,
        a: 0.06,
    };
    let bg_hover = Color {
        r: palette.brand.r,
        g: palette.brand.g,
        b: palette.brand.b,
        a: 0.05,
    };
    let sep_color = palette.elevated;

    let accent_bar = container(iced::widget::Space::new().width(2))
        .height(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(accent_color)),
            ..container::Style::default()
        });

    let ts = iced::widget::text(event.timestamp)
        .size(FONT_CAPS)
        .color(palette.text_faint)
        .font(mono)
        .width(80);

    let badge = source_badge(event.source, palette);

    let etype = container(
        iced::widget::text(event.event_type)
            .size(FONT_CAPS)
            .color(palette.text_primary)
            .font(mono),
    )
    .width(104);

    let summary = container(
        iced::widget::text(event.summary)
            .size(FONT_CAPS)
            .color(palette.text_secondary)
            .font(mono),
    )
    .width(Length::Fill)
    .clip(true);

    let result_color = if event.is_error {
        palette.random
    } else {
        match event.result_tag {
            Some("ok") | Some("sent") => palette.success,
            Some("err") => palette.random,
            _ => palette.text_muted,
        }
    };

    let mut content_row = row![ts, badge, etype, summary]
        .spacing(10)
        .align_y(iced::Alignment::Center);

    if let Some(tag) = event.result_tag {
        content_row = content_row.push(
            iced::widget::text(tag)
                .size(FONT_CAPS_SM)
                .color(result_color)
                .font(mono),
        );
    }

    let content = container(content_row)
        .padding(Padding {
            top: 5.0,
            right: 14.0,
            bottom: 5.0,
            left: 10.0,
        })
        .width(Length::Fill);

    let full_row = row![accent_bar, content];

    let separator = container(iced::widget::Space::new().width(Length::Fill).height(1)).style(
        move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(sep_color)),
            ..container::Style::default()
        },
    );

    let btn = button(full_row)
        .on_press(on_click)
        .padding(0)
        .width(Length::Fill)
        .style(
            move |_theme: &iced::Theme, status: button::Status| button::Style {
                background: match status {
                    button::Status::Hovered if !selected && !event.is_error => {
                        Some(Background::Color(bg_hover))
                    }
                    _ if selected => Some(Background::Color(bg_selected)),
                    _ if event.is_error => Some(Background::Color(bg_error)),
                    _ => None,
                },
                text_color: Color::TRANSPARENT,
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
        );

    column![btn, separator].into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn source_badge_constructs_for_all_sources() {
        let palette = &CATPPUCCIN_MOCHA;
        let sources = [
            EventSource::Twitch,
            EventSource::YouTube,
            EventSource::Kick,
            EventSource::Trovo,
            EventSource::Core,
            EventSource::Rhai,
            EventSource::Http,
            EventSource::Obs,
            EventSource::VTube,
            EventSource::Discord,
            EventSource::Midi,
            EventSource::Hotkey,
            EventSource::Timer,
            EventSource::Server,
        ];
        for source in sources {
            let _: iced::Element<'_, ()> = source_badge(source, palette);
        }
    }

    #[test]
    fn twitch_color_is_brand() {
        let color = color_for_source(EventSource::Twitch, &CATPPUCCIN_MOCHA);
        assert_eq!(color, CATPPUCCIN_MOCHA.brand);
    }

    #[test]
    fn event_row_unselected_constructs() {
        let palette = &CATPPUCCIN_MOCHA;
        let data = EventRowData {
            timestamp: "14:23:01.124",
            source: EventSource::Twitch,
            event_type: "chat.message",
            summary: "koval_dev: !quote",
            result_tag: Some("→ 1 action"),
            is_error: false,
        };
        let _: iced::Element<'_, ()> = event_row_observability(&data, false, (), palette);
    }

    #[test]
    fn event_row_selected_constructs() {
        let palette = &CATPPUCCIN_MOCHA;
        let data = EventRowData {
            timestamp: "14:23:01.142",
            source: EventSource::Twitch,
            event_type: "command.matched",
            summary: "!quote by koval_dev (VIP)",
            result_tag: Some("→ trigger fired"),
            is_error: false,
        };
        let _: iced::Element<'_, ()> = event_row_observability(&data, true, (), palette);
    }

    #[test]
    fn event_row_error_constructs() {
        let palette = &CATPPUCCIN_MOCHA;
        let data = EventRowData {
            timestamp: "14:23:02.402",
            source: EventSource::Http,
            event_type: "request.fail",
            summary: "GET api.twitch.tv/.../followers → 429 rate limited",
            result_tag: Some("retry in 12s"),
            is_error: true,
        };
        let _: iced::Element<'_, ()> = event_row_observability(&data, false, (), palette);
    }

    #[test]
    fn event_row_no_result_tag_constructs() {
        let palette = &CATPPUCCIN_MOCHA;
        let data = EventRowData {
            timestamp: "14:23:01.145",
            source: EventSource::Core,
            event_type: "subaction.run",
            summary: "[1/5] read_file → %lines% = [128]",
            result_tag: None,
            is_error: false,
        };
        let _: iced::Element<'_, ()> = event_row_observability(&data, false, (), palette);
    }

    #[test]
    fn result_tag_ok_uses_success_color() {
        let palette = &CATPPUCCIN_MOCHA;
        let data = EventRowData {
            timestamp: "14:23:01.158",
            source: EventSource::Core,
            event_type: "action.done",
            summary: "!quote · 5/5 sub-actions",
            result_tag: Some("ok"),
            is_error: false,
        };
        let _: iced::Element<'_, ()> = event_row_observability(&data, false, (), palette);
    }

    #[test]
    fn error_row_accent_is_random_color() {
        let color = color_for_source(EventSource::Http, &CATPPUCCIN_MOCHA);
        assert_eq!(color, CATPPUCCIN_MOCHA.random);
    }
}
