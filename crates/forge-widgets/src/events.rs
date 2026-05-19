use forge_events::EventSource;
use iced::{Border, Color, Element, widget::container};

use crate::{
    palette::ForgePalette,
    tokens::{FONT_CAPS_XS, Radius, radius},
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
}
