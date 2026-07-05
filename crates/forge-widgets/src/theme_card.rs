use iced::{
    Alignment, Background, Border, Color, Element, Length,
    widget::button::{Status, Style},
    widget::{Space, button, column, container, row, text},
};

use crate::palette::ForgePalette;
use crate::theme::palette_for_theme;
use crate::tokens::ThemeId;
use crate::tokens::{
    BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf,
};

pub struct ThemeCardParams {
    pub theme_id: ThemeId,
    pub title: String,
    pub subtitle: String,
    pub active_label: String,
    pub selected: bool,
}

fn swatch<'a, Msg: 'a>(color: Color, height: f32, width: Length) -> Element<'a, Msg> {
    container(Space::new().width(width).height(Length::Fixed(height)))
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(color)),
            border: Border {
                radius: 2.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn preview<'a, Msg: 'a>(theme_id: ThemeId) -> Element<'a, Msg> {
    let tp = palette_for_theme(theme_id);
    let faint = Color {
        a: 0.4,
        ..tp.text_muted
    };

    let sidebar = column![
        swatch::<Msg>(tp.brand, 5.0, Length::Fill),
        swatch::<Msg>(faint, 5.0, Length::FillPortion(85)),
        swatch::<Msg>(faint, 5.0, Length::FillPortion(70)),
        Space::new().height(Length::Fill),
        swatch::<Msg>(faint, 4.0, Length::FillPortion(60)),
    ]
    .spacing(4)
    .padding([8, 4])
    .width(Length::FillPortion(28));

    let sidebar_panel = container(sidebar)
        .height(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(tp.shell)),
            border: Border {
                color: tp.border_regular,
                width: 0.5,
                ..Border::default()
            },
            ..container::Style::default()
        });

    let main = column![
        swatch::<Msg>(tp.text_muted, 8.0, Length::FillPortion(50)),
        swatch::<Msg>(faint, 4.0, Length::FillPortion(80)),
        Space::new().height(Length::Fill),
        row![
            swatch::<Msg>(tp.brand, 14.0, Length::FillPortion(40)),
            swatch::<Msg>(faint, 14.0, Length::FillPortion(30)),
        ]
        .spacing(5),
        swatch::<Msg>(faint, 6.0, Length::FillPortion(70)),
    ]
    .spacing(5)
    .padding(8)
    .width(Length::FillPortion(72));

    container(row![sidebar_panel, main].height(Length::Fixed(100.0)))
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(tp.base)),
            border: Border {
                color: tp.border_regular,
                width: 0.5,
                radius: radius(Radius::Sm).into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn card_style(
    selected: bool,
    brand: Color,
    border_regular: Color,
    elevated: Color,
    text_primary: Color,
) -> impl Fn(&iced::Theme, Status) -> Style {
    move |_theme, status| {
        let (border_color, border_width) = if selected {
            (brand, BORDER_THIN)
        } else {
            (border_regular, 0.5)
        };
        let bg = match status {
            Status::Hovered => Color {
                a: if selected { 0.08 } else { 0.04 },
                ..brand
            },
            Status::Pressed => Color { a: 0.12, ..brand },
            _ => elevated,
        };
        Style {
            background: Some(Background::Color(bg)),
            text_color: text_primary,
            border: Border {
                color: border_color,
                width: border_width,
                radius: radius(Radius::Md).into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        }
    }
}

fn active_badge<'a, Msg: 'a>(label: String, palette: &ForgePalette) -> Element<'a, Msg> {
    let brand = palette.brand;
    let surface = palette.surface_overlay;
    let badge = row![
        crate::icons::tabler_icon(crate::icons::Icon::CircleCheck, 10.0, brand),
        text(label)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(brand),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    container(badge)
        .padding([0, sp(Spacing::Xs)])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                radius: 8.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

pub fn theme_card<'a, Msg: Clone + 'a>(
    params: ThemeCardParams,
    on_click: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let title_color = if params.selected {
        palette.text_primary
    } else {
        palette.text_secondary
    };

    let mut footer = row![
        text(params.title).size(FONT_SM).color(title_color),
        Space::new().width(Length::Fill),
    ]
    .align_y(Alignment::Center);
    if params.selected {
        footer = footer.push(active_badge::<Msg>(params.active_label, palette));
    }

    let content = column![
        preview::<Msg>(params.theme_id),
        footer,
        text(params.subtitle)
            .size(FONT_XS)
            .color(palette.text_muted),
    ]
    .spacing(spf(Spacing::Xs));

    button(container(content).padding(sp(Spacing::Sm)))
        .on_press(on_click)
        .width(Length::Fill)
        .style(card_style(
            params.selected,
            palette.brand,
            palette.border_regular,
            palette.elevated,
            palette.text_primary,
        ))
        .into()
}
