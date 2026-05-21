use iced::{
    Background, Border, Color, Element, Length, Padding,
    widget::{button, column, container, row, text, text_input},
};

use crate::palette::ForgePalette;
use crate::tokens::{FONT_BODY, FONT_SM, FONT_XS, FontRole, Radius, font, radius};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeKind {
    Moderator,
    Vip,
    Subscriber,
    Bot,
    Broadcaster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Twitch,
    YouTube,
    Kick,
    Trovo,
}

impl Platform {
    pub fn color(self, palette: &ForgePalette) -> Color {
        match self {
            Platform::Twitch => palette.brand,
            Platform::YouTube => palette.random,
            Platform::Kick => palette.info,
            Platform::Trovo => palette.success,
        }
    }

    fn letter(self) -> &'static str {
        match self {
            Platform::Twitch => "T",
            Platform::YouTube => "Y",
            Platform::Kick => "K",
            Platform::Trovo => "V",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatBody {
    Message(String),
    Event { kind: String, detail: String },
}

#[derive(Debug, Clone)]
pub struct ChatRow {
    pub timestamp: String,
    pub platform: Platform,
    pub badges: Vec<BadgeKind>,
    pub username: String,
    pub username_color: Color,
    pub body: ChatBody,
}

pub struct PlatformTarget<'a, Msg> {
    pub platform: Platform,
    pub active: bool,
    pub on_press: Option<Box<dyn Fn() -> Msg + 'a>>,
}

fn badge_label(kind: BadgeKind) -> &'static str {
    match kind {
        BadgeKind::Moderator => "MOD",
        BadgeKind::Vip => "VIP",
        BadgeKind::Subscriber => "SUB",
        BadgeKind::Bot => "BOT",
        BadgeKind::Broadcaster => "LIVE",
    }
}

fn badge_color(kind: BadgeKind, palette: &ForgePalette) -> Color {
    match kind {
        BadgeKind::Moderator => palette.success,
        BadgeKind::Vip => palette.warning,
        BadgeKind::Bot => palette.brand,
        BadgeKind::Subscriber => palette.info,
        BadgeKind::Broadcaster => palette.random,
    }
}

fn badge_pill<'a, Msg: 'a>(kind: BadgeKind, palette: &ForgePalette) -> Element<'a, Msg> {
    let color = badge_color(kind, palette);
    let bg = Color { a: 0.18, ..color };
    let shell = palette.shell;
    let label = badge_label(kind);
    container(
        text(label)
            .size(FONT_XS)
            .color(color)
            .font(font(FontRole::Body)),
    )
    .padding([1, 5])
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            radius: radius(Radius::Xs).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        text_color: Some(shell),
        ..container::Style::default()
    })
    .into()
}

fn platform_dot<'a, Msg: 'a>(platform: Platform, palette: &ForgePalette) -> Element<'a, Msg> {
    let color = platform.color(palette);
    let letter = platform.letter();
    let shell = palette.shell;
    container(
        text(letter)
            .size(8.0)
            .color(shell)
            .font(font(FontRole::Body)),
    )
    .width(14)
    .height(14)
    .align_x(iced::Alignment::Center)
    .align_y(iced::Alignment::Center)
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            radius: radius(Radius::Xs).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    })
    .into()
}

pub fn chat_row<'a, Msg: Clone + 'a>(
    palette: &'a ForgePalette,
    row_data: &'a ChatRow,
) -> Element<'a, Msg> {
    let ts = container(
        text(row_data.timestamp.as_str())
            .size(FONT_XS)
            .color(palette.text_faint)
            .font(font(FontRole::Monospace)),
    )
    .width(42)
    .padding(Padding {
        top: 2.0,
        ..Padding::ZERO
    });

    let dot = container(platform_dot(row_data.platform, palette)).padding(Padding {
        top: 2.0,
        ..Padding::ZERO
    });

    match &row_data.body {
        ChatBody::Message(msg) => {
            let mut badge_row_items: Vec<Element<'a, Msg>> = row_data
                .badges
                .iter()
                .map(|&b| badge_pill::<Msg>(b, palette))
                .collect();

            let username_color = row_data.username_color;
            let username_el = text(row_data.username.as_str())
                .size(FONT_SM)
                .color(username_color)
                .font(font(FontRole::Body));

            let separator = text(": ")
                .size(FONT_SM)
                .color(palette.text_primary)
                .font(font(FontRole::Body));

            let message_el = text(msg.as_str())
                .size(FONT_SM)
                .color(palette.text_primary)
                .font(font(FontRole::Body));

            badge_row_items.push(username_el.into());
            badge_row_items.push(separator.into());
            badge_row_items.push(message_el.into());

            let content = row(badge_row_items)
                .spacing(4)
                .align_y(iced::Alignment::Center)
                .wrap();

            let body = container(content).width(Length::Fill);

            row![ts, dot, body]
                .spacing(8)
                .align_y(iced::Alignment::Start)
                .padding([3, 0])
                .into()
        }

        ChatBody::Event { kind, detail } => {
            let accent = event_accent_color(kind.as_str(), palette);

            let kind_el = text(kind.as_str())
                .size(FONT_SM)
                .color(accent)
                .font(font(FontRole::Body));

            let detail_el = text(detail.as_str())
                .size(FONT_SM)
                .color(palette.text_secondary)
                .font(font(FontRole::Body));

            let content = column![kind_el, detail_el].spacing(2);

            let body = container(content).width(Length::Fill);

            let inner = row![ts, dot, body]
                .spacing(8)
                .align_y(iced::Alignment::Start);

            container(inner)
                .width(Length::Fill)
                .padding([6, 10])
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(Background::Color(iced::Color {
                        a: 1.0,
                        ..palette.elevated
                    })),
                    border: Border {
                        color: accent,
                        width: 2.0,
                        radius: iced::border::left(0.0)
                            .top_right(radius(Radius::Sm))
                            .bottom_right(radius(Radius::Sm)),
                    },
                    ..container::Style::default()
                })
                .into()
        }
    }
}

fn event_accent_color(kind: &str, palette: &ForgePalette) -> Color {
    if kind.contains("sub") || kind.contains("Sub") {
        palette.brand
    } else if kind.contains("bits") || kind.contains("cheer") {
        palette.warning
    } else if kind.contains("raid") {
        palette.random
    } else {
        palette.info
    }
}

pub(crate) fn chip_bg(active: bool, palette: &ForgePalette) -> Color {
    if active {
        palette.surface_overlay
    } else {
        Color::TRANSPARENT
    }
}

pub fn filter_chip<'a, Msg: Clone + 'a>(
    palette: &ForgePalette,
    label: &str,
    dot_color: Color,
    active: bool,
    on_press: Msg,
) -> Element<'a, Msg> {
    let bg = chip_bg(active, palette);
    let text_color = if active {
        palette.text_primary
    } else {
        palette.text_secondary
    };
    let dot_size = 5.0_f32;
    let dot_radius = dot_size / 2.0;

    let dot = container(iced::widget::Space::new().width(dot_size).height(dot_size))
        .width(dot_size)
        .height(dot_size)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(dot_color)),
            border: Border {
                radius: dot_radius.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let label_text = text(label.to_owned())
        .size(FONT_XS)
        .color(text_color)
        .font(font(FontRole::Body));

    let content = row![dot, label_text]
        .spacing(5)
        .align_y(iced::Alignment::Center);

    button(content)
        .on_press(on_press)
        .padding([4, 10])
        .style(move |_theme: &iced::Theme, _status| button::Style {
            background: Some(Background::Color(bg)),
            border: Border {
                radius: radius(Radius::Pill).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            text_color,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .into()
}

fn platform_target_button<'a, Msg: Clone + 'a>(
    platform: Platform,
    active: bool,
    on_press: Option<Box<dyn Fn() -> Msg + 'a>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let platform_color = platform.color(palette);
    let letter = platform.letter();

    let (bg, text_color) = if active {
        (palette.surface_overlay, platform_color)
    } else {
        (Color::TRANSPARENT, palette.text_faint)
    };

    let content = text(letter)
        .size(9.0)
        .color(text_color)
        .font(font(FontRole::Body));

    let btn = button(
        container(content)
            .width(20)
            .height(20)
            .align_x(iced::Alignment::Center)
            .align_y(iced::Alignment::Center),
    )
    .padding(0)
    .style(move |_theme: &iced::Theme, _status| button::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            radius: radius(Radius::Xs).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        text_color,
        shadow: iced::Shadow::default(),
        snap: false,
    });

    if let Some(handler) = on_press {
        btn.on_press_with(handler).into()
    } else {
        btn.into()
    }
}

fn hint_row<'a, Msg: 'a>(palette: &ForgePalette) -> Element<'a, Msg> {
    let color = palette.text_faint;
    let mono = font(FontRole::Monospace);

    let slash_hint = row![
        text("/").size(FONT_XS).color(color).font(mono),
        text(" commands").size(FONT_XS).color(color),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    let at_hint = row![
        text("@").size(FONT_XS).color(color).font(mono),
        text(" mention").size(FONT_XS).color(color),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    let bang_hint = row![
        text("!").size(FONT_XS).color(color).font(mono),
        text(" trigger action").size(FONT_XS).color(color),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    row![slash_hint, at_hint, bang_hint]
        .spacing(14)
        .align_y(iced::Alignment::Center)
        .into()
}

pub fn input_bar<'a, Msg: Clone + 'a>(
    palette: &ForgePalette,
    value: &'a str,
    placeholder: &'a str,
    platform_targets: Vec<PlatformTarget<'a, Msg>>,
    on_input: impl Fn(String) -> Msg + 'a,
    on_submit: Msg,
) -> Element<'a, Msg> {
    let p = *palette;

    let target_buttons: Vec<Element<'a, Msg>> = platform_targets
        .into_iter()
        .map(|t| platform_target_button(t.platform, t.active, t.on_press, palette))
        .collect();

    let divider = container(iced::widget::Space::new().width(0.5_f32).height(18))
        .width(0.5_f32)
        .height(18)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(p.border_regular)),
            ..container::Style::default()
        });

    let send_msg = on_submit.clone();
    let input_widget = text_input(placeholder, value)
        .on_input(on_input)
        .on_submit(on_submit)
        .padding([0, 0])
        .size(FONT_BODY)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, _status| text_input::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 0.0.into(),
            },
            icon: p.text_muted,
            placeholder: p.text_muted,
            value: p.text_primary,
            selection: Color { a: 0.25, ..p.brand },
        });

    let send_button = button(
        text("\u{ea99}")
            .size(15.0)
            .color(palette.brand)
            .font(font(FontRole::Body)),
    )
    .on_press(send_msg)
    .padding(0)
    .style(|_theme: &iced::Theme, _status| button::Style {
        background: None,
        border: Border::default(),
        text_color: Color::TRANSPARENT,
        shadow: iced::Shadow::default(),
        snap: false,
    });

    let mut composer_children: Vec<Element<'a, Msg>> = target_buttons;
    composer_children.push(divider.into());
    composer_children.push(input_widget.into());
    composer_children.push(send_button.into());

    let composer = container(
        row(composer_children)
            .spacing(8)
            .align_y(iced::Alignment::Center),
    )
    .padding([6, 10])
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(p.elevated)),
        border: Border {
            color: p.border_input,
            width: 0.5,
            radius: radius(Radius::Xl).into(),
        },
        ..container::Style::default()
    });

    let hints = container(hint_row(palette)).padding(Padding {
        top: 6.0,
        right: 4.0,
        bottom: 0.0,
        left: 4.0,
    });

    container(column![composer, hints].spacing(0))
        .padding([10, 14])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(p.shell)),
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
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
    fn twitch_platform_color_is_brand() {
        let p = CATPPUCCIN_MOCHA;
        assert_eq!(Platform::Twitch.color(&p), p.brand);
    }

    #[test]
    fn youtube_platform_color_is_random() {
        let p = CATPPUCCIN_MOCHA;
        assert_eq!(Platform::YouTube.color(&p), p.random);
    }

    #[test]
    fn kick_platform_color_is_info() {
        let p = CATPPUCCIN_MOCHA;
        assert_eq!(Platform::Kick.color(&p), p.info);
    }

    #[test]
    fn trovo_platform_color_is_success() {
        let p = CATPPUCCIN_MOCHA;
        assert_eq!(Platform::Trovo.color(&p), p.success);
    }

    #[test]
    fn chip_bg_active_returns_surface_overlay() {
        let p = CATPPUCCIN_MOCHA;
        assert_eq!(chip_bg(true, &p), p.surface_overlay);
    }

    #[test]
    fn chip_bg_inactive_returns_transparent() {
        let p = CATPPUCCIN_MOCHA;
        assert_eq!(chip_bg(false, &p), Color::TRANSPARENT);
    }

    #[test]
    fn badge_kind_has_five_variants() {
        let variants = [
            BadgeKind::Moderator,
            BadgeKind::Vip,
            BadgeKind::Subscriber,
            BadgeKind::Bot,
            BadgeKind::Broadcaster,
        ];
        assert_eq!(variants.len(), 5);
    }

    #[test]
    fn platform_has_four_variants() {
        let variants = [
            Platform::Twitch,
            Platform::YouTube,
            Platform::Kick,
            Platform::Trovo,
        ];
        assert_eq!(variants.len(), 4);
    }

    #[test]
    fn chat_row_message_compiles() {
        let row_data = ChatRow {
            timestamp: "14:21".to_string(),
            platform: Platform::Twitch,
            badges: vec![BadgeKind::Moderator],
            username: "testuser".to_string(),
            username_color: CATPPUCCIN_MOCHA.brand,
            body: ChatBody::Message("hello world".to_string()),
        };
        let _: Element<'_, ()> = chat_row(&CATPPUCCIN_MOCHA, &row_data);
    }

    #[test]
    fn chat_row_event_compiles() {
        let row_data = ChatRow {
            timestamp: "14:22".to_string(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "subscriber".to_string(),
            username_color: CATPPUCCIN_MOCHA.brand,
            body: ChatBody::Event {
                kind: "subscribed (Tier 1)".to_string(),
                detail: "3 months".to_string(),
            },
        };
        let _: Element<'_, ()> = chat_row(&CATPPUCCIN_MOCHA, &row_data);
    }

    #[test]
    fn filter_chip_active_compiles() {
        let _: Element<'_, ()> =
            filter_chip(&CATPPUCCIN_MOCHA, "All", CATPPUCCIN_MOCHA.brand, true, ());
    }

    #[test]
    fn filter_chip_inactive_compiles() {
        let _: Element<'_, ()> = filter_chip(
            &CATPPUCCIN_MOCHA,
            "Twitch",
            CATPPUCCIN_MOCHA.brand,
            false,
            (),
        );
    }

    #[test]
    fn input_bar_compiles_with_no_targets() {
        let _: Element<'_, String> = input_bar(
            &CATPPUCCIN_MOCHA,
            "",
            "Send a message...",
            vec![],
            |s: String| s,
            String::new(),
        );
    }

    #[test]
    fn input_bar_compiles_with_twitch_target() {
        let _: Element<'_, ()> = input_bar(
            &CATPPUCCIN_MOCHA,
            "hello",
            "Send...",
            vec![PlatformTarget {
                platform: Platform::Twitch,
                active: true,
                on_press: Some(Box::new(|| ())),
            }],
            |_| (),
            (),
        );
    }

    #[test]
    fn all_platforms_have_distinct_colors() {
        let p = CATPPUCCIN_MOCHA;
        let colors = [
            Platform::Twitch.color(&p),
            Platform::YouTube.color(&p),
            Platform::Kick.color(&p),
            Platform::Trovo.color(&p),
        ];
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(colors[i], colors[j]);
            }
        }
    }
}
