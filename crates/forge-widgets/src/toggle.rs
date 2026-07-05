use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding,
    widget::{Space, button, column, container, row, text},
};

use crate::palette::ForgePalette;
use crate::tokens::{FONT_SM, FontRole, Spacing, font, sp};

const TRACK_WIDTH: f32 = 32.0;
const TRACK_HEIGHT: f32 = 18.0;
const THUMB_SIZE: f32 = 14.0;
const THUMB_INSET: f32 = 2.0;

#[derive(Debug, Clone)]
pub struct ToggleProps<Msg> {
    pub label: String,
    pub description: String,
    pub value: bool,
    pub on_toggle: Msg,
}

fn switch_visual<'a, Msg: 'a>(
    on: bool,
    on_color: Option<Color>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let track_bg = if on {
        on_color.unwrap_or(palette.success)
    } else {
        palette.surface_overlay
    };
    let thumb_color = if on {
        palette.shell
    } else {
        palette.text_faint
    };
    let thumb_offset = if on {
        TRACK_WIDTH - THUMB_SIZE - THUMB_INSET
    } else {
        THUMB_INSET
    };

    let thumb = container(Space::new().width(THUMB_SIZE).height(THUMB_SIZE))
        .width(THUMB_SIZE)
        .height(THUMB_SIZE)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(thumb_color)),
            border: Border {
                radius: (THUMB_SIZE / 2.0).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    container(thumb)
        .width(TRACK_WIDTH)
        .height(TRACK_HEIGHT)
        .padding(Padding {
            top: THUMB_INSET,
            right: 0.0,
            bottom: 0.0,
            left: thumb_offset,
        })
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(track_bg)),
            border: Border {
                radius: (TRACK_HEIGHT / 2.0).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        })
        .into()
}

pub fn toggle_switch<'a, Msg: Clone + 'a>(
    on: bool,
    on_color: Option<Color>,
    on_change: Msg,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    button(switch_visual(on, on_color, palette))
        .on_press(on_change)
        .padding(0)
        .style(|_: &iced::Theme, _status| button::Style {
            background: None,
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .into()
}

pub fn toggle<'a, Msg: Clone + 'a>(
    palette: &'a ForgePalette,
    props: ToggleProps<Msg>,
) -> Element<'a, Msg> {
    let label_el = text(props.label)
        .size(FONT_SM)
        .color(palette.text_primary)
        .font(font(FontRole::Body));

    let desc_el = text(props.description)
        .size(FONT_SM)
        .color(palette.text_faint)
        .font(font(FontRole::Body));

    let label_col = column![label_el, desc_el].spacing(2);

    let inner = row![
        container(label_col).width(Length::Fill),
        switch_visual(props.value, None, palette),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    button(inner)
        .on_press(props.on_toggle)
        .padding([sp(Spacing::Xs), 0])
        .width(Length::Fill)
        .style(|_: &iced::Theme, _status| button::Style {
            background: None,
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .into()
}
