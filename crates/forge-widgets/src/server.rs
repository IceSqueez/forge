use forge_events::EventSource;
use iced::{
    Alignment, Border, Color, Element, Length,
    font::Style as FontStyle,
    widget::Row,
    widget::button::{Status, Style},
    widget::{Space, button, column, container, row, text},
};

use crate::{
    events::color_for_source,
    icons::{
        BOOTSTRAP_FONT, ICON_ALERT_TRIANGLE, ICON_COPY, ICON_EYE, ICON_EYE_SLASH, ICON_REFRESH,
        ICON_X,
    },
    palette::ForgePalette,
    tokens::{
        FONT_BODY_LG, FONT_BODY_SM, FONT_CAPS, FONT_CAPS_SM, FONT_CAPS_XS, FontRole, Radius, font,
        radius,
    },
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

const MAX_VISIBLE_CHIPS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientStatus {
    Active,
    Idle,
    Disconnecting,
}

pub struct SubscriptionChipData<'a> {
    pub label: &'a str,
    pub source: EventSource,
}

pub struct ClientRowData<'a> {
    pub status: ClientStatus,
    pub identification: &'a str,
    pub client_type_label: &'a str,
    pub subscriptions: &'a [SubscriptionChipData<'a>],
    pub events_per_second: f32,
    pub uptime_short: &'a str,
    pub highlight: bool,
}

fn dot_style(color: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(iced::Background::Color(color)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 3.0.into(),
        },
        ..container::Style::default()
    }
}

fn chip_bg_style(bg: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(iced::Background::Color(bg)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: radius(Radius::Xxl).into(),
        },
        ..container::Style::default()
    }
}

fn separator_style(color: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(iced::Background::Color(color)),
        ..container::Style::default()
    }
}

fn chip_element<'a, Msg: 'a>(label: &'a str, fg: Color, bg: Color) -> Element<'a, Msg> {
    container(
        text(label)
            .font(font(FontRole::Monospace))
            .size(FONT_CAPS_XS)
            .color(fg),
    )
    .padding([1u16, 5u16])
    .style(chip_bg_style(bg))
    .into()
}

fn more_badge<'a, Msg: 'a>(n: usize, palette: &ForgePalette) -> Element<'a, Msg> {
    let label = format!("+{n} more");
    let fg = palette.text_faint;
    let bg = palette.surface_overlay;
    container(
        text(label)
            .font(font(FontRole::Monospace))
            .size(FONT_CAPS_XS)
            .color(fg),
    )
    .padding([1u16, 5u16])
    .style(chip_bg_style(bg))
    .into()
}

fn chips_cell_row<'a, Msg: 'a>(
    subscriptions: &'a [SubscriptionChipData<'a>],
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.surface_overlay;

    if subscriptions.len() == 1 && subscriptions[0].label == "*" {
        let wildcard = chip_element("*", palette.success, bg);
        let hint = text("all events")
            .font(iced::Font {
                style: FontStyle::Italic,
                ..font(FontRole::Monospace)
            })
            .size(FONT_CAPS_XS)
            .color(palette.text_faint);
        return row![wildcard, hint]
            .spacing(4)
            .align_y(Alignment::Center)
            .into();
    }

    let visible = subscriptions.len().min(MAX_VISIBLE_CHIPS);
    let overflow = subscriptions.len().saturating_sub(MAX_VISIBLE_CHIPS);

    let mut chips: Vec<Element<'a, Msg>> = subscriptions[..visible]
        .iter()
        .map(|c| chip_element(c.label, color_for_source(c.source, palette), bg))
        .collect();

    if overflow > 0 {
        chips.push(more_badge(overflow, palette));
    }

    Row::with_children(chips)
        .spacing(3)
        .align_y(Alignment::Center)
        .into()
}

pub fn client_table_row<'a, Msg: Clone + 'a>(
    row: &'a ClientRowData<'a>,
    on_disconnect: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let dot_color = match row.status {
        ClientStatus::Active => palette.success,
        ClientStatus::Idle => palette.warning,
        ClientStatus::Disconnecting => palette.random,
    };

    let dot = container(Space::new().width(6.0f32).height(6.0f32)).style(dot_style(dot_color));

    let dot_cell = container(dot).width(Length::Fixed(24.0));

    let id_col = column![
        text(row.identification)
            .font(font(FontRole::Monospace))
            .size(FONT_BODY_SM)
            .color(palette.text_primary),
        text(row.client_type_label)
            .font(font(FontRole::Monospace))
            .size(FONT_CAPS_SM)
            .color(palette.text_faint),
    ]
    .spacing(2);

    let id_cell = container(id_col).width(Length::FillPortion(14));

    let subs_cell =
        container(chips_cell_row::<Msg>(row.subscriptions, palette)).width(Length::FillPortion(16));

    let evs_label = format!("{:.1}", row.events_per_second);
    let evs_cell = container(
        text(evs_label)
            .font(font(FontRole::Monospace))
            .size(FONT_CAPS)
            .color(palette.text_primary),
    )
    .width(Length::Fixed(80.0));

    let uptime_cell = container(
        text(row.uptime_short)
            .font(font(FontRole::Monospace))
            .size(FONT_CAPS)
            .color(palette.text_muted),
    )
    .width(Length::Fixed(70.0));

    let x_btn = button(text(ICON_X.to_string()).font(BOOTSTRAP_FONT).size(13))
        .on_press(on_disconnect)
        .padding([2u16, 3u16])
        .style(ghost_icon_style(palette.text_faint, palette.text_secondary));

    let x_cell = container(x_btn).width(Length::Fixed(22.0));

    let content = row![dot_cell, id_cell, subs_cell, evs_cell, uptime_cell, x_cell]
        .align_y(Alignment::Center)
        .padding([8u16, 14u16]);

    let highlight = row.highlight;
    let shell = palette.shell;
    let elevated = palette.elevated;

    let separator =
        container(Space::new().width(Length::Fill).height(1.0f32)).style(separator_style(elevated));

    container(column![content, separator])
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: if highlight {
                Some(iced::Background::Color(shell))
            } else {
                None
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

    fn make_row<'a>(
        status: ClientStatus,
        subs: &'a [SubscriptionChipData<'a>],
        highlight: bool,
    ) -> ClientRowData<'a> {
        ClientRowData {
            status,
            identification: "test.html",
            client_type_label: "OBS browser · 127.0.0.1",
            subscriptions: subs,
            events_per_second: 0.4,
            uptime_short: "2h 14m",
            highlight,
        }
    }

    #[test]
    fn smoke_client_row_active() {
        let subs = [SubscriptionChipData {
            label: "twitch.sub",
            source: EventSource::Twitch,
        }];
        let data = make_row(ClientStatus::Active, &subs, false);
        let _: Element<'_, ()> = client_table_row(&data, (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn smoke_client_row_idle() {
        let subs = [SubscriptionChipData {
            label: "twitch.cheer",
            source: EventSource::Twitch,
        }];
        let data = make_row(ClientStatus::Idle, &subs, false);
        let _: Element<'_, ()> = client_table_row(&data, (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn smoke_client_row_disconnecting() {
        let subs: &[SubscriptionChipData<'_>] = &[];
        let data = make_row(ClientStatus::Disconnecting, subs, true);
        let _: Element<'_, ()> = client_table_row(&data, (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn smoke_client_row_four_subscriptions() {
        let subs = [
            SubscriptionChipData {
                label: "twitch.sub",
                source: EventSource::Twitch,
            },
            SubscriptionChipData {
                label: "twitch.cheer",
                source: EventSource::Twitch,
            },
            SubscriptionChipData {
                label: "twitch.raid",
                source: EventSource::Twitch,
            },
            SubscriptionChipData {
                label: "yt.super_chat",
                source: EventSource::YouTube,
            },
        ];
        let data = make_row(ClientStatus::Active, &subs, false);
        let _: Element<'_, ()> = client_table_row(&data, (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn smoke_client_row_wildcard_subscription() {
        let subs = [SubscriptionChipData {
            label: "*",
            source: EventSource::Core,
        }];
        let data = make_row(ClientStatus::Active, &subs, true);
        let _: Element<'_, ()> = client_table_row(&data, (), &CATPPUCCIN_MOCHA);
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
