use iced::{
    Alignment, Border, Color, Element, Length,
    widget::button::{Status, Style},
    widget::{Space, button, column, container, row, text},
};

use crate::{
    icons::{Icon, tabler_icon},
    palette::ForgePalette,
    tokens::{BORDER_THIN, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindBadge {
    Recommended,
    RequiresConfirmation,
}

pub struct BindAddressCardParams<'a> {
    pub title: String,
    pub tech_label: &'a str,
    pub badge: BindBadge,
    pub description: String,
    pub selected: bool,
}

fn badge_color(badge: BindBadge, palette: &ForgePalette) -> Color {
    match badge {
        BindBadge::Recommended => palette.success,
        BindBadge::RequiresConfirmation => palette.warning,
    }
}

fn badge_icon(badge: BindBadge) -> Icon {
    match badge {
        BindBadge::Recommended => Icon::Lock,
        BindBadge::RequiresConfirmation => Icon::AlertTriangle,
    }
}

fn badge_label(badge: BindBadge) -> &'static str {
    match badge {
        BindBadge::Recommended => "Recommended",
        BindBadge::RequiresConfirmation => "Requires confirmation",
    }
}

fn radio_ring_style(ring_color: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(iced::Background::Color(Color::TRANSPARENT)),
        border: Border {
            color: ring_color,
            width: 2.0,
            radius: 999.0.into(),
        },
        ..container::Style::default()
    }
}

fn radio_fill_style(fill_color: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(iced::Background::Color(fill_color)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 999.0.into(),
        },
        ..container::Style::default()
    }
}

fn radio_dot<'a, Msg: 'a>(selected: bool, palette: &ForgePalette) -> Element<'a, Msg> {
    let ring_color = if selected {
        palette.brand
    } else {
        palette.border_input
    };

    let inner: Element<'a, Msg> = if selected {
        container(Space::new().width(7.0f32).height(7.0f32))
            .style(radio_fill_style(palette.brand))
            .into()
    } else {
        Space::new().into()
    };

    container(inner)
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(radio_ring_style(ring_color))
        .into()
}

fn bind_badge_element<'a, Msg: 'a>(badge: BindBadge, palette: &ForgePalette) -> Element<'a, Msg> {
    let color = badge_color(badge, palette);
    let surface = palette.surface_overlay;

    let badge_row = row![
        tabler_icon(badge_icon(badge), 10.0, color),
        text(badge_label(badge))
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(color),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    container(badge_row)
        .padding([0, sp(Spacing::Xs)])
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(surface)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 8.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn bind_card_style(
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
            background: Some(iced::Background::Color(bg)),
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

pub fn bind_address_card<'a, Msg: Clone + 'a>(
    params: BindAddressCardParams<'a>,
    on_click: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let dot = radio_dot::<Msg>(params.selected, palette);

    let title_row = row![
        text(params.title).size(FONT_SM).color(palette.text_primary),
        bind_badge_element::<Msg>(params.badge, palette),
        Space::new().width(Length::Fill),
        text(params.tech_label)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(palette.text_faint),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let description = text(params.description)
        .size(FONT_SM)
        .color(palette.text_muted);

    let content = column![title_row, description].spacing(2);

    let dot_padded = container(dot).padding(0);

    let card_row = row![dot_padded, content,]
        .spacing(11)
        .align_y(Alignment::Start)
        .padding([sp(Spacing::Sm), sp(Spacing::Md)]);

    button(card_row)
        .on_press(on_click)
        .width(Length::Fill)
        .style(bind_card_style(
            params.selected,
            palette.brand,
            palette.border_regular,
            palette.elevated,
            palette.text_primary,
        ))
        .into()
}
