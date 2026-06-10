use iced::{
    Alignment, Border, Color, Element, Length,
    widget::{Space, button, column, container, row, text},
};

use crate::{
    icons::{Icon, tabler_icon},
    palette::ForgePalette,
    tokens::{FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp},
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

    let eye_icon = if revealed { Icon::EyeOff } else { Icon::Eye };

    let icon_normal = palette.text_faint;
    let icon_hover = palette.text_secondary;

    let eye_btn = button(tabler_icon(eye_icon, 13.0, icon_normal))
        .on_press(on_toggle_reveal)
        .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)])
        .style(super::ghost_icon_style(icon_normal, icon_hover));

    let token_inner = row![
        text(display)
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(palette.text_primary),
        Space::new().width(Length::Fill),
        eye_btn,
    ]
    .align_y(Alignment::Center);

    let token_box = container(token_inner)
        .width(Length::Fill)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
        .style(token_box_style(palette.shell, palette.border_regular));

    let border = palette.border_regular;
    let copy_normal = palette.text_secondary;
    let copy_hover = palette.text_primary;

    let copy_btn = button(
        row![
            tabler_icon(Icon::Copy, 12.0, copy_normal),
            text(crate::tr!("widget.bearer.copy"))
                .font(font(FontRole::Monospace))
                .size(FONT_XS),
        ]
        .spacing(5)
        .align_y(Alignment::Center),
    )
    .on_press(on_copy)
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .style(super::outline_btn_style(border, copy_normal, copy_hover));

    let warn_color = palette.warning;

    let regen_btn = button(
        row![
            tabler_icon(Icon::Refresh, 12.0, warn_color),
            text(crate::tr!("widget.bearer.regenerate"))
                .font(font(FontRole::Monospace))
                .size(FONT_XS),
        ]
        .spacing(5)
        .align_y(Alignment::Center),
    )
    .on_press(on_regenerate)
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .style(super::outline_btn_style(border, warn_color, warn_color));

    let controls = row![token_box, copy_btn, regen_btn]
        .spacing(8)
        .align_y(Alignment::Center);

    let warning_row = row![
        tabler_icon(Icon::AlertTriangle, 11.0, palette.warning),
        text(crate::tr!("widget.bearer.regen_warning"))
            .size(FONT_XS)
            .color(palette.text_faint),
    ]
    .spacing(5)
    .align_y(Alignment::Center);

    column![controls, warning_row].spacing(4).into()
}

#[cfg(test)]
mod tests {
    use super::*;

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
