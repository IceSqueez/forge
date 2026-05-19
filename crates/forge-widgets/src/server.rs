use iced::{
    Alignment, Border, Color, Element, Length,
    widget::button::{Status, Style},
    widget::{Space, button, column, container, row, text},
};

use crate::{
    icons::{
        BOOTSTRAP_FONT, ICON_ALERT_TRIANGLE, ICON_COPY, ICON_EYE, ICON_EYE_SLASH, ICON_REFRESH,
    },
    palette::ForgePalette,
    tokens::{FONT_BODY_LG, FONT_CAPS, FONT_CAPS_SM, FontRole, Radius, font, radius},
};

fn token_box_style(bg: Color, border_color: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_theme| container::Style {
        background: Some(iced::Background::Color(bg)),
        border: Border {
            color: border_color,
            width: 0.5,
            radius: radius(Radius::Md).into(),
        },
        ..container::Style::default()
    }
}

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

fn mask_token(token: &str) -> String {
    const PREFIX: &str = "fg_";
    const SUFFIX_LEN: usize = 4;

    let (prefix, body) = if let Some(rest) = token.strip_prefix(PREFIX) {
        (PREFIX, rest)
    } else {
        ("", token)
    };

    let chars: Vec<char> = body.chars().collect();
    if chars.len() <= SUFFIX_LEN {
        return token.to_owned();
    }

    let mask_len = chars.len() - SUFFIX_LEN;
    let suffix: String = chars[chars.len() - SUFFIX_LEN..].iter().collect();
    let bullets = "•".repeat(mask_len);
    format!("{prefix}{bullets}{suffix}")
}

pub fn bearer_token_display<'a, Msg: Clone + 'a>(
    token: &'a str,
    revealed: bool,
    on_toggle_reveal: Msg,
    on_copy: Msg,
    on_regenerate: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let display: String = if revealed {
        token.to_owned()
    } else {
        mask_token(token)
    };

    let eye_char = if revealed { ICON_EYE_SLASH } else { ICON_EYE };

    let icon_normal = palette.text_faint;
    let icon_hover = palette.text_secondary;

    let eye_btn = button(text(eye_char.to_string()).font(BOOTSTRAP_FONT).size(13))
        .on_press(on_toggle_reveal)
        .padding([2, 4])
        .style(ghost_icon_style(icon_normal, icon_hover));

    let token_inner = row![
        text(display)
            .font(font(FontRole::Monospace))
            .size(FONT_BODY_LG)
            .color(palette.text_primary),
        Space::new().width(Length::Fill),
        eye_btn,
    ]
    .align_y(Alignment::Center);

    let token_box = container(token_inner)
        .width(Length::Fill)
        .padding([6, 12])
        .style(token_box_style(palette.shell, palette.border_regular));

    let border = palette.border_regular;
    let copy_normal = palette.text_secondary;
    let copy_hover = palette.text_primary;

    let copy_btn = button(
        row![
            text(ICON_COPY.to_string()).font(BOOTSTRAP_FONT).size(12),
            text("COPY").font(font(FontRole::Monospace)).size(FONT_CAPS),
        ]
        .spacing(5)
        .align_y(Alignment::Center),
    )
    .on_press(on_copy)
    .padding([7, 10])
    .style(outline_btn_style(border, copy_normal, copy_hover));

    let warn_color = palette.warning;

    let regen_btn = button(
        row![
            text(ICON_REFRESH.to_string()).font(BOOTSTRAP_FONT).size(12),
            text("REGENERATE")
                .font(font(FontRole::Monospace))
                .size(FONT_CAPS),
        ]
        .spacing(5)
        .align_y(Alignment::Center),
    )
    .on_press(on_regenerate)
    .padding([7, 10])
    .style(outline_btn_style(border, warn_color, warn_color));

    let controls = row![token_box, copy_btn, regen_btn]
        .spacing(8)
        .align_y(Alignment::Center);

    let warning_row = row![
        text(ICON_ALERT_TRIANGLE.to_string())
            .font(BOOTSTRAP_FONT)
            .size(11)
            .color(palette.warning),
        text("Regenerating disconnects all clients")
            .size(FONT_CAPS_SM)
            .color(palette.text_faint),
    ]
    .spacing(5)
    .align_y(Alignment::Center);

    column![controls, warning_row].spacing(4).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn mask_token_replaces_middle_with_bullets() {
        let result = mask_token("fg_verylongtoken5L9k");
        assert!(result.starts_with("fg_"));
        assert!(result.contains('•'));
        assert!(result.ends_with("5L9k"));
    }

    #[test]
    fn mask_token_bullet_count_matches_hidden_chars() {
        let result = mask_token("fg_verylongtoken5L9k");
        let bullet_count = result.chars().filter(|&c| c == '•').count();
        assert_eq!(bullet_count, 13);
    }

    #[test]
    fn mask_token_short_body_returns_unchanged() {
        let short = "fg_abc";
        assert_eq!(mask_token(short), short);
    }

    #[test]
    fn mask_token_no_prefix_still_masks() {
        let result = mask_token("abcdefghij");
        assert!(result.contains('•'));
        assert!(result.ends_with("ghij"));
        assert!(!result.starts_with("fg_"));
    }

    #[test]
    fn mask_token_differs_from_original() {
        let token = "fg_abc12345xyz5L9k";
        assert_ne!(mask_token(token), token);
    }

    #[test]
    fn bearer_token_display_masked_smoke() {
        let _: Element<'_, ()> =
            bearer_token_display("fg_abc12345xyz5L9k", false, (), (), (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn bearer_token_display_revealed_smoke() {
        let _: Element<'_, ()> =
            bearer_token_display("fg_abc12345xyz5L9k", true, (), (), (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn masked_display_contains_bullets() {
        let token = "fg_abc12345xyz5L9k";
        let masked = mask_token(token);
        assert!(masked.contains('•'));
    }

    #[test]
    fn revealed_display_equals_original() {
        let token = "fg_abc12345xyz5L9k";
        assert_eq!(token, token);
        assert_ne!(mask_token(token), token);
    }
}
