mod bearer_token;
mod bind_card;
mod confirm_modal;

pub use bearer_token::bearer_token_display;
pub use bind_card::{BindAddressCardParams, BindBadge, bind_address_card};
pub use confirm_modal::{BulletItem, BulletKind, TypeToConfirmModalParams, type_to_confirm_modal};

use iced::{
    Border, Color,
    widget::button::{Status, Style},
};

use crate::tokens::{Radius, radius};

fn outline_btn_style(
    border_color: Color,
    normal_text: Color,
    hover_text: Color,
) -> impl Fn(&iced::Theme, Status) -> Style {
    let r = radius(Radius::Md);
    move |_theme, status| match status {
        Status::Active | Status::Pressed => Style {
            background: Some(iced::Background::Color(Color::TRANSPARENT)),
            text_color: normal_text,
            border: Border {
                color: border_color,
                width: 0.5,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        },
        Status::Hovered => Style {
            background: Some(iced::Background::Color(Color {
                a: 0.06,
                ..border_color
            })),
            text_color: hover_text,
            border: Border {
                color: border_color,
                width: 0.5,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        },
        Status::Disabled => Style {
            background: Some(iced::Background::Color(Color::TRANSPARENT)),
            text_color: Color {
                a: 0.4,
                ..normal_text
            },
            border: Border {
                color: Color {
                    a: 0.4,
                    ..border_color
                },
                width: 0.5,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        },
    }
}

fn ghost_icon_style(normal: Color, hover: Color) -> impl Fn(&iced::Theme, Status) -> Style {
    move |_theme, status| Style {
        background: match status {
            Status::Hovered => Some(iced::Background::Color(Color { a: 0.06, ..hover })),
            _ => None,
        },
        text_color: match status {
            Status::Hovered => hover,
            _ => normal,
        },
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        shadow: iced::Shadow::default(),
        snap: false,
    }
}
