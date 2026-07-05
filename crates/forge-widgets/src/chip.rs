use std::borrow::Cow;

use iced::{
    Background, Border, Color, Element,
    widget::{button, container, row, text},
};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::status::status_dot;
use crate::tokens::{FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf};

#[derive(Clone, Copy)]
pub enum ChipGlyph {
    None,
    Dot(Color),
    Icon(Icon, Color),
}

pub fn chip<'a, Msg: Clone + 'a>(
    label: impl Into<Cow<'a, str>>,
    glyph: ChipGlyph,
    active: bool,
    on_press: Option<Msg>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let bg = if active {
        Some(Background::Color(palette.surface_overlay))
    } else {
        None
    };
    let text_color = if active {
        palette.text_primary
    } else {
        palette.text_secondary
    };

    let label_text = text(label.into())
        .size(FONT_XS)
        .color(text_color)
        .font(font(FontRole::Body));

    let mut content = row![]
        .spacing(spf(Spacing::Xxs))
        .align_y(iced::Alignment::Center);
    match glyph {
        ChipGlyph::None => {}
        ChipGlyph::Dot(color) => content = content.push(status_dot(color, 5.0)),
        ChipGlyph::Icon(icon, color) => content = content.push(tabler_icon(icon, FONT_XS, color)),
    }
    content = content.push(label_text);

    let padding = [sp(Spacing::Xxs), sp(Spacing::Sm)];

    match on_press {
        Some(msg) => button(content)
            .on_press(msg)
            .padding(padding)
            .style(move |_theme: &iced::Theme, _status| button::Style {
                background: bg,
                border: Border {
                    radius: radius(Radius::Pill).into(),
                    color: Color::TRANSPARENT,
                    width: 0.0,
                },
                text_color,
                shadow: iced::Shadow::default(),
                snap: false,
            })
            .into(),
        None => container(content)
            .padding(padding)
            .style(move |_theme: &iced::Theme| container::Style {
                background: bg,
                border: Border {
                    radius: radius(Radius::Pill).into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into(),
    }
}

pub struct ChipSpec<'a, Msg> {
    pub label: Cow<'a, str>,
    pub glyph: ChipGlyph,
    pub active: bool,
    pub on_press: Msg,
}

impl<'a, Msg> ChipSpec<'a, Msg> {
    pub fn new(
        label: impl Into<Cow<'a, str>>,
        glyph: ChipGlyph,
        active: bool,
        on_press: Msg,
    ) -> Self {
        Self {
            label: label.into(),
            glyph,
            active,
            on_press,
        }
    }
}

pub fn filter_chip_row<'a, Msg: Clone + 'a>(
    specs: Vec<ChipSpec<'a, Msg>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let chips: Vec<Element<'a, Msg>> = specs
        .into_iter()
        .map(|spec| {
            chip(
                spec.label,
                spec.glyph,
                spec.active,
                Some(spec.on_press),
                palette,
            )
        })
        .collect();
    row(chips)
        .spacing(spf(Spacing::Xxs))
        .align_y(iced::Alignment::Center)
        .into()
}
