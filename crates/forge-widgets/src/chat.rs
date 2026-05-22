use iced::{
    Background, Border, Color, Element, Length, Padding,
    widget::{button, column, container, row, text, text_input},
};

use crate::icons::{Icon, tabler_icon};
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

    fn name(self) -> &'static str {
        match self {
            Platform::Twitch => "Twitch",
            Platform::YouTube => "YouTube",
            Platform::Kick => "Kick",
            Platform::Trovo => "Trovo",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatBody {
    Message(String),
    Subscription {
        tier: u8,
        months: Option<u32>,
        message: Option<String>,
        triggered_action: Option<String>,
    },
    Cheer {
        bits: u64,
        text: String,
    },
    Raid {
        viewers: u64,
        triggered_action: Option<String>,
    },
    Command {
        command: String,
        action_name: Option<String>,
        action_duration_ms: Option<u64>,
    },
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
            radius: radius(Radius::Sm).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        text_color: Some(shell),
        ..container::Style::default()
    })
    .into()
}

fn platform_badge<'a, Msg: 'a>(platform: Platform, palette: &ForgePalette) -> Element<'a, Msg> {
    let color = platform.color(palette);
    let name = platform.name();
    let shell = palette.shell;
    container(
        text(name)
            .size(FONT_XS)
            .color(shell)
            .font(font(FontRole::Body)),
    )
    .padding([1, 6])
    .align_y(iced::Alignment::Center)
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    })
    .into()
}

fn inline_badge<'a, Msg: 'a>(
    label: &str,
    bg: Color,
    fg: Color,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let _ = palette;
    container(
        text(label.to_owned())
            .size(FONT_XS)
            .color(fg)
            .font(font(FontRole::Monospace)),
    )
    .padding([1, 6])
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    })
    .into()
}

fn clickable_username_style(
    username_color: Color,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: if hovered {
                Some(Background::Color(Color {
                    a: 0.10,
                    ..username_color
                }))
            } else {
                None
            },
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: radius(Radius::Sm).into(),
            },
            text_color: username_color,
            shadow: iced::Shadow::default(),
            snap: false,
        }
    }
}

fn money_event_icon<'a, Msg: 'a>(icon: Icon, color: Color) -> Element<'a, Msg> {
    container(tabler_icon(icon, 13.0, color))
        .padding(Padding {
            top: 2.0,
            ..Padding::ZERO
        })
        .into()
}

fn row_chrome<'a, Msg: 'a>(
    content: Element<'a, Msg>,
    stripe_color: Color,
    body_bg: Color,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let sep_color = palette.border_regular;
    let stripe = container(iced::widget::Space::new().width(2).height(Length::Fill))
        .width(2)
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(stripe_color)),
            ..container::Style::default()
        });

    let body = container(content)
        .width(Length::Fill)
        .padding([6, 10])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(body_bg)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: iced::border::left(0.0)
                    .top_right(radius(Radius::Sm))
                    .bottom_right(radius(Radius::Sm)),
            },
            ..container::Style::default()
        });

    let main = row![stripe, body].spacing(0);
    let separator = container(iced::widget::Space::new().height(0.5).width(Length::Fill))
        .height(0.5)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(sep_color)),
            ..container::Style::default()
        });

    column![main, separator].spacing(0).into()
}

fn triggered_badge<'a, Msg: 'a>(action: &str, palette: &ForgePalette) -> Element<'a, Msg> {
    let label = format!("Triggered: {action}");
    let bg = Color {
        a: 0.20,
        ..palette.success
    };
    container(
        text(label)
            .size(FONT_XS)
            .color(palette.success)
            .font(font(FontRole::Body)),
    )
    .padding([2, 8])
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    })
    .into()
}

fn timestamp_top<'a, Msg: 'a>(ts: &str, palette: &ForgePalette) -> Element<'a, Msg> {
    text(ts.to_owned())
        .size(FONT_XS)
        .color(palette.text_faint)
        .font(font(FontRole::Monospace))
        .into()
}

#[allow(clippy::too_many_arguments)]
pub fn chat_row_msg<'a, Msg: Clone + 'a>(
    palette: &ForgePalette,
    timestamp: &str,
    platform: Platform,
    badges: &[BadgeKind],
    username: &str,
    username_color: Color,
    text_body: &str,
    on_user_click: Option<fn(String) -> Msg>,
) -> Element<'a, Msg> {
    let ts_top = timestamp_top(timestamp, palette);
    let p_badge = platform_badge(platform, palette);

    let mut top_items: Vec<Element<'a, Msg>> = vec![ts_top, p_badge];
    for &b in badges {
        top_items.push(badge_pill::<Msg>(b, palette));
    }
    let top_row = row(top_items).spacing(6).align_y(iced::Alignment::Center);

    let username_el: Element<'a, Msg> = match on_user_click {
        Some(on_click) => {
            let uname = username.to_owned();
            button(
                text(uname.clone())
                    .size(FONT_SM)
                    .color(username_color)
                    .font(font(FontRole::Body)),
            )
            .on_press(on_click(uname))
            .padding([0, 2])
            .style(clickable_username_style(username_color))
            .into()
        }
        None => text(username.to_owned())
            .size(FONT_SM)
            .color(username_color)
            .font(font(FontRole::Body))
            .into(),
    };

    let bottom_row = row(vec![
        username_el,
        text(": ")
            .size(FONT_SM)
            .color(palette.text_secondary)
            .font(font(FontRole::Body))
            .into(),
        text(text_body.to_owned())
            .size(FONT_SM)
            .color(palette.text_primary)
            .font(font(FontRole::Body))
            .into(),
    ])
    .spacing(4)
    .align_y(iced::Alignment::Center)
    .wrap();

    let inner = column![top_row, bottom_row].spacing(2);

    row_chrome(
        inner.into(),
        Color::TRANSPARENT,
        Color::TRANSPARENT,
        palette,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn chat_row_sub<'a, Msg: Clone + 'a>(
    palette: &ForgePalette,
    timestamp: &str,
    platform: Platform,
    username: &str,
    username_color: Color,
    tier: u8,
    months: Option<u32>,
    sub_message: Option<&str>,
    triggered_action: Option<&str>,
    on_user_click: Option<fn(String) -> Msg>,
) -> Element<'a, Msg> {
    let ts_top = timestamp_top(timestamp, palette);
    let icon = money_event_icon(Icon::Star, palette.brand);
    let p_badge = platform_badge(platform, palette);
    let p = *palette;

    let tier_label = format!("subscribed (Tier {tier})");

    let username_el: Element<'a, Msg> = match on_user_click {
        Some(on_click) => {
            let uname = username.to_owned();
            button(
                text(uname.clone())
                    .size(FONT_SM)
                    .color(username_color)
                    .font(font(FontRole::Body)),
            )
            .on_press(on_click(uname))
            .padding([0, 2])
            .style(clickable_username_style(username_color))
            .into()
        }
        None => text(username.to_owned())
            .size(FONT_SM)
            .color(username_color)
            .font(font(FontRole::Body))
            .into(),
    };

    let mut first_row_items: Vec<Element<'a, Msg>> = vec![
        username_el,
        text(format!(" {tier_label}"))
            .size(FONT_SM)
            .color(palette.text_secondary)
            .font(font(FontRole::Body))
            .into(),
    ];
    if let Some(mo) = months {
        let label = format!("{mo} mo");
        let bg = Color {
            a: 0.15,
            ..palette.warning
        };
        first_row_items.push(inline_badge(&label, bg, palette.warning, palette));
    }

    let first_row = row(first_row_items)
        .spacing(6)
        .align_y(iced::Alignment::Center);

    let mut body_col_items: Vec<Element<'a, Msg>> = vec![first_row.into()];

    if let Some(msg_text) = sub_message {
        body_col_items.push(
            text(msg_text.to_owned())
                .size(FONT_SM)
                .color(palette.text_muted)
                .font(font(FontRole::Body))
                .into(),
        );
    }

    let body_col = column(body_col_items).spacing(3).width(Length::Fill);

    let mut top_items: Vec<Element<'a, Msg>> = vec![ts_top, p_badge];
    if let Some(action) = triggered_action {
        top_items.push(iced::widget::Space::new().width(Length::Fill).into());
        top_items.push(triggered_badge(action, palette));
    }
    let top_row = row(top_items).spacing(6).align_y(iced::Alignment::Center);
    let bottom_row = row![icon, body_col]
        .spacing(8)
        .align_y(iced::Alignment::Start);
    let inner = column![top_row, bottom_row].spacing(2);

    row_chrome(inner.into(), p.brand, p.elevated, palette)
}

#[allow(clippy::too_many_arguments)]
pub fn chat_row_cheer<'a, Msg: Clone + 'a>(
    palette: &ForgePalette,
    timestamp: &str,
    platform: Platform,
    username: &str,
    username_color: Color,
    bits: u64,
    cheer_text: &str,
    on_user_click: Option<fn(String) -> Msg>,
) -> Element<'a, Msg> {
    let ts_top = timestamp_top(timestamp, palette);
    let icon = money_event_icon(Icon::Bolt, palette.warning);
    let p_badge = platform_badge(platform, palette);
    let p = *palette;

    let bits_label = format!("{bits} bits");
    let bits_bg = Color {
        a: 0.20,
        ..palette.warning
    };

    let username_el: Element<'a, Msg> = match on_user_click {
        Some(on_click) => {
            let uname = username.to_owned();
            button(
                text(uname.clone())
                    .size(FONT_SM)
                    .color(username_color)
                    .font(font(FontRole::Body)),
            )
            .on_press(on_click(uname))
            .padding([0, 2])
            .style(clickable_username_style(username_color))
            .into()
        }
        None => text(username.to_owned())
            .size(FONT_SM)
            .color(username_color)
            .font(font(FontRole::Body))
            .into(),
    };

    let first_row: Element<'a, Msg> = row(vec![
        username_el,
        text(" cheered")
            .size(FONT_SM)
            .color(palette.text_secondary)
            .font(font(FontRole::Body))
            .into(),
        inline_badge(&bits_label, bits_bg, palette.warning, palette),
    ])
    .spacing(6)
    .align_y(iced::Alignment::Center)
    .into();

    let msg_line = text(cheer_text.to_owned())
        .size(FONT_SM)
        .color(palette.text_primary)
        .font(font(FontRole::Body));

    let body_col = column![first_row, msg_line].spacing(3).width(Length::Fill);

    let top_row = row![ts_top, p_badge]
        .spacing(6)
        .align_y(iced::Alignment::Center);
    let bottom_row = row![icon, body_col]
        .spacing(8)
        .align_y(iced::Alignment::Start);
    let inner = column![top_row, bottom_row].spacing(2);

    row_chrome(inner.into(), p.warning, p.elevated, palette)
}

pub fn chat_row_raid<'a, Msg: Clone + 'a>(
    palette: &ForgePalette,
    timestamp: &str,
    platform: Platform,
    username: &str,
    viewers: u64,
    triggered_action: Option<&str>,
) -> Element<'a, Msg> {
    let ts_top = timestamp_top(timestamp, palette);
    let icon = money_event_icon(Icon::Flag, palette.random);
    let p_badge = platform_badge(platform, palette);
    let p = *palette;

    let viewers_label = format!("{viewers} viewers");
    let viewers_bg = Color {
        a: 0.20,
        ..palette.random
    };

    let first_row: Element<'a, Msg> = row![
        text(username.to_owned())
            .size(FONT_SM)
            .color(palette.random)
            .font(font(FontRole::Body)),
        text(" is raiding with")
            .size(FONT_SM)
            .color(palette.text_secondary)
            .font(font(FontRole::Body)),
        inline_badge(&viewers_label, viewers_bg, palette.random, palette),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center)
    .into();

    let body_col = column![first_row].spacing(3).width(Length::Fill);

    let mut top_items: Vec<Element<'a, Msg>> = vec![ts_top, p_badge];
    if let Some(action) = triggered_action {
        top_items.push(iced::widget::Space::new().width(Length::Fill).into());
        top_items.push(triggered_badge(action, palette));
    }
    let top_row = row(top_items).spacing(6).align_y(iced::Alignment::Center);
    let bottom_row = row![icon, body_col]
        .spacing(8)
        .align_y(iced::Alignment::Start);
    let inner = column![top_row, bottom_row].spacing(2);

    row_chrome(inner.into(), p.random, p.elevated, palette)
}

#[allow(clippy::too_many_arguments)]
pub fn chat_row_cmd<'a, Msg: Clone + 'a>(
    palette: &ForgePalette,
    timestamp: &str,
    platform: Platform,
    badges: &[BadgeKind],
    username: &str,
    username_color: Color,
    command: &str,
    action_name: Option<&str>,
    action_duration_ms: Option<u64>,
    on_user_click: Option<fn(String) -> Msg>,
) -> Element<'a, Msg> {
    let ts_top = timestamp_top(timestamp, palette);
    let p_badge = platform_badge(platform, palette);

    let cmd_bg = Color {
        a: 0.25,
        ..palette.surface_overlay
    };
    let cmd_text = container(
        text(command.to_owned())
            .size(FONT_XS)
            .color(palette.brand)
            .font(font(FontRole::Monospace)),
    )
    .padding([1, 5])
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(cmd_bg)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    });

    let username_el: Element<'a, Msg> = match on_user_click {
        Some(on_click) => {
            let uname = username.to_owned();
            button(
                text(uname.clone())
                    .size(FONT_SM)
                    .color(username_color)
                    .font(font(FontRole::Body)),
            )
            .on_press(on_click(uname))
            .padding([0, 2])
            .style(clickable_username_style(username_color))
            .into()
        }
        None => text(username.to_owned())
            .size(FONT_SM)
            .color(username_color)
            .font(font(FontRole::Body))
            .into(),
    };

    let first_row = row(vec![
        username_el,
        text(": ")
            .size(FONT_SM)
            .color(palette.text_secondary)
            .font(font(FontRole::Body))
            .into(),
        cmd_text.into(),
    ])
    .spacing(4)
    .align_y(iced::Alignment::Center);

    let outcome_badge: Option<Element<'a, Msg>> = match (action_name, action_duration_ms) {
        (Some(name), Some(ms)) => Some(triggered_badge(&format!("{name} · {ms}ms"), palette)),
        (Some(name), None) => Some(triggered_badge(name, palette)),
        _ => None,
    };

    let body_col = column![first_row].spacing(3).width(Length::Fill);

    let mut top_items: Vec<Element<'a, Msg>> = vec![ts_top, p_badge];
    for &b in badges {
        top_items.push(badge_pill::<Msg>(b, palette));
    }
    if let Some(badge) = outcome_badge {
        top_items.push(iced::widget::Space::new().width(Length::Fill).into());
        top_items.push(badge);
    }
    let top_row = row(top_items).spacing(6).align_y(iced::Alignment::Center);

    let inner = column![top_row, body_col].spacing(2);

    row_chrome(
        inner.into(),
        Color::TRANSPARENT,
        Color::TRANSPARENT,
        palette,
    )
}

pub fn chat_row<'a, Msg: Clone + 'a>(
    palette: &'a ForgePalette,
    row_data: &'a ChatRow,
    on_user_click: Option<fn(String) -> Msg>,
) -> Element<'a, Msg> {
    match &row_data.body {
        ChatBody::Message(msg) => chat_row_msg(
            palette,
            &row_data.timestamp,
            row_data.platform,
            &row_data.badges,
            &row_data.username,
            row_data.username_color,
            msg,
            on_user_click,
        ),
        ChatBody::Subscription {
            tier,
            months,
            message,
            triggered_action,
        } => chat_row_sub(
            palette,
            &row_data.timestamp,
            row_data.platform,
            &row_data.username,
            row_data.username_color,
            *tier,
            *months,
            message.as_deref(),
            triggered_action.as_deref(),
            on_user_click,
        ),
        ChatBody::Cheer { bits, text } => chat_row_cheer(
            palette,
            &row_data.timestamp,
            row_data.platform,
            &row_data.username,
            row_data.username_color,
            *bits,
            text,
            on_user_click,
        ),
        ChatBody::Raid {
            viewers,
            triggered_action,
        } => chat_row_raid(
            palette,
            &row_data.timestamp,
            row_data.platform,
            &row_data.username,
            *viewers,
            triggered_action.as_deref(),
        ),
        ChatBody::Command {
            command,
            action_name,
            action_duration_ms,
        } => chat_row_cmd(
            palette,
            &row_data.timestamp,
            row_data.platform,
            &row_data.badges,
            &row_data.username,
            row_data.username_color,
            command,
            action_name.as_deref(),
            *action_duration_ms,
            on_user_click,
        ),
    }
}

pub fn chip_bg(active: bool, palette: &ForgePalette) -> Color {
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
            radius: radius(Radius::Sm).into(),
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
            radius: radius(Radius::Md).into(),
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
    fn chat_row_msg_compiles_with_sample_data() {
        let _: Element<'_, ()> = chat_row_msg(
            &CATPPUCCIN_MOCHA,
            "14:21:05",
            Platform::Twitch,
            &[BadgeKind::Moderator],
            "testuser",
            CATPPUCCIN_MOCHA.brand,
            "hello world",
            None,
        );
    }

    #[test]
    fn chat_row_sub_compiles_with_all_fields() {
        let _: Element<'_, ()> = chat_row_sub(
            &CATPPUCCIN_MOCHA,
            "14:22:10",
            Platform::Twitch,
            "danylo_ua",
            CATPPUCCIN_MOCHA.bits,
            1,
            Some(3),
            Some("Дякую за стрім!"),
            Some("Welcome new subscriber"),
            None,
        );
    }

    #[test]
    fn chat_row_sub_compiles_minimal() {
        let _: Element<'_, ()> = chat_row_sub(
            &CATPPUCCIN_MOCHA,
            "14:23:00",
            Platform::Twitch,
            "viewer_x",
            CATPPUCCIN_MOCHA.success,
            1,
            None,
            None,
            None,
            None,
        );
    }

    #[test]
    fn chat_row_cheer_compiles_with_sample_data() {
        let _: Element<'_, ()> = chat_row_cheer(
            &CATPPUCCIN_MOCHA,
            "14:24:30",
            Platform::Twitch,
            "viewer_x",
            CATPPUCCIN_MOCHA.success,
            500,
            "keep going!",
            None,
        );
    }

    #[test]
    fn chat_row_raid_compiles_with_triggered_action() {
        let _: Element<'_, ()> = chat_row_raid(
            &CATPPUCCIN_MOCHA,
            "14:25:00",
            Platform::Twitch,
            "factorio_streamer",
            42,
            Some("Raid welcome + OBS scene"),
        );
    }

    #[test]
    fn chat_row_raid_compiles_without_triggered_action() {
        let _: Element<'_, ()> = chat_row_raid(
            &CATPPUCCIN_MOCHA,
            "14:25:00",
            Platform::Twitch,
            "factorio_streamer",
            42,
            None,
        );
    }

    #[test]
    fn chat_row_cmd_compiles_with_outcome() {
        let _: Element<'_, ()> = chat_row_cmd(
            &CATPPUCCIN_MOCHA,
            "14:26:15",
            Platform::Twitch,
            &[],
            "koval_dev",
            CATPPUCCIN_MOCHA.success,
            "!quote",
            Some("!quote"),
            Some(18),
            None,
        );
    }

    #[test]
    fn chat_row_cmd_compiles_without_outcome() {
        let _: Element<'_, ()> = chat_row_cmd(
            &CATPPUCCIN_MOCHA,
            "14:26:15",
            Platform::Twitch,
            &[BadgeKind::Vip],
            "koval_dev",
            CATPPUCCIN_MOCHA.success,
            "!so",
            None,
            None,
            None,
        );
    }

    #[test]
    fn chat_row_dispatcher_handles_message_body() {
        let row_data = ChatRow {
            timestamp: "14:21".to_string(),
            platform: Platform::Twitch,
            badges: vec![BadgeKind::Moderator],
            username: "testuser".to_string(),
            username_color: CATPPUCCIN_MOCHA.brand,
            body: ChatBody::Message("hello world".to_string()),
        };
        let _: Element<'_, ()> = chat_row(&CATPPUCCIN_MOCHA, &row_data, None);
    }

    #[test]
    fn chat_row_dispatcher_handles_subscription_body() {
        let row_data = ChatRow {
            timestamp: "14:22".to_string(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "subscriber".to_string(),
            username_color: CATPPUCCIN_MOCHA.brand,
            body: ChatBody::Subscription {
                tier: 1,
                months: Some(3),
                message: Some("Thanks!".to_string()),
                triggered_action: Some("Welcome".to_string()),
            },
        };
        let _: Element<'_, ()> = chat_row(&CATPPUCCIN_MOCHA, &row_data, None);
    }

    #[test]
    fn chat_row_dispatcher_handles_cheer_body() {
        let row_data = ChatRow {
            timestamp: "14:23".to_string(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "cheerer".to_string(),
            username_color: CATPPUCCIN_MOCHA.warning,
            body: ChatBody::Cheer {
                bits: 100,
                text: "nice stream".to_string(),
            },
        };
        let _: Element<'_, ()> = chat_row(&CATPPUCCIN_MOCHA, &row_data, None);
    }

    #[test]
    fn chat_row_dispatcher_handles_raid_body() {
        let row_data = ChatRow {
            timestamp: "14:24".to_string(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "raider".to_string(),
            username_color: CATPPUCCIN_MOCHA.random,
            body: ChatBody::Raid {
                viewers: 42,
                triggered_action: None,
            },
        };
        let _: Element<'_, ()> = chat_row(&CATPPUCCIN_MOCHA, &row_data, None);
    }

    #[test]
    fn chat_row_dispatcher_handles_command_body() {
        let row_data = ChatRow {
            timestamp: "14:25".to_string(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "user".to_string(),
            username_color: CATPPUCCIN_MOCHA.success,
            body: ChatBody::Command {
                command: "!quote".to_string(),
                action_name: Some("!quote".to_string()),
                action_duration_ms: Some(18),
            },
        };
        let _: Element<'_, ()> = chat_row(&CATPPUCCIN_MOCHA, &row_data, None);
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
