use std::collections::VecDeque;

use forge_events::{Event, EventSource};
use forge_widgets::{
    BadgeKind, ChatBody, ChatRow, ForgePalette, Icon, Platform, PlatformTarget, tabler_icon,
};
use iced::{Color, Element, Length};

use crate::Message;

pub const CHAT_LOG_MAX: usize = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlatformFilter {
    #[default]
    All,
    Twitch,
    YouTube,
    Kick,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChatFilters {
    pub platform: PlatformFilter,
    pub events_only: bool,
    pub hide_bots: bool,
}

pub struct LiveChatState {
    pub chat_log: VecDeque<ChatRow>,
    pub chat_input: String,
    pub chat_filter: ChatFilters,
    pub drawer_open: bool,
}

impl LiveChatState {
    pub fn new() -> Self {
        let mut chat_log: VecDeque<ChatRow> = VecDeque::new();

        chat_log.push_back(ChatRow {
            timestamp: "14:21:00".into(),
            platform: Platform::Twitch,
            badges: vec![BadgeKind::Moderator],
            username: "haash_".into(),
            username_color: Color::from_rgb8(0xcb, 0xa6, 0xf7),
            body: ChatBody::Message("welcome to the stream everyone, GTNH grind continues".into()),
        });
        chat_log.push_back(ChatRow {
            timestamp: "14:21:16".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "danylo_ua".into(),
            username_color: Color::from_rgb8(0xfa, 0xb3, 0x87),
            body: ChatBody::Subscription {
                tier: 1,
                months: Some(3),
                message: Some("Дякую за стрім, GTNH топ!".into()),
                triggered_action: Some("Welcome new subscriber".into()),
            },
        });
        chat_log.push_back(ChatRow {
            timestamp: "14:21:30".into(),
            platform: Platform::YouTube,
            badges: vec![],
            username: "olena_lv".into(),
            username_color: Color::from_rgb8(0xf5, 0xc2, 0xe7),
            body: ChatBody::Message("aluminum bottleneck знов :(".into()),
        });
        chat_log.push_back(ChatRow {
            timestamp: "14:21:55".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "koval_dev".into(),
            username_color: Color::from_rgb8(0xa6, 0xe3, 0xa1),
            body: ChatBody::Command {
                command: "!quote".into(),
                action_name: Some("!quote".into()),
                action_duration_ms: Some(18),
            },
        });
        chat_log.push_back(ChatRow {
            timestamp: "14:22:12".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "stream_fan_kyiv".into(),
            username_color: Color::from_rgb8(0xfa, 0xb3, 0x87),
            body: ChatBody::Cheer {
                bits: 500,
                text: "keep going! love the UA stream".into(),
            },
        });
        chat_log.push_back(ChatRow {
            timestamp: "14:22:29".into(),
            platform: Platform::Kick,
            badges: vec![],
            username: "ostap_pl".into(),
            username_color: Color::from_rgb8(0x94, 0xe2, 0xd5),
            body: ChatBody::Message("ти вже відкрив stainless steel?".into()),
        });
        chat_log.push_back(ChatRow {
            timestamp: "14:22:48".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "factorio_streamer".into(),
            username_color: Color::from_rgb8(0xf3, 0x8b, 0xa8),
            body: ChatBody::Raid {
                viewers: 42,
                triggered_action: Some("Raid welcome + OBS scene".into()),
            },
        });

        Self {
            chat_log,
            chat_input: String::new(),
            chat_filter: ChatFilters::default(),
            drawer_open: false,
        }
    }
}

impl Default for LiveChatState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn chat_row_from_event(event: &Event) -> Option<ChatRow> {
    if event.source != EventSource::Twitch {
        return None;
    }
    match event.kind.as_str() {
        "chat.message" => parse_chat_message(event),
        "channel.subscribe" | "channel.subscription.message" => parse_subscription(event),
        "channel.cheer" => parse_cheer(event),
        "channel.raid" => parse_raid(event),
        _ => None,
    }
}

fn parse_chat_message(event: &Event) -> Option<ChatRow> {
    let payload = event.payload.as_object()?;

    let username = payload
        .get("chatter_user_name")
        .or_else(|| payload.get("username"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_owned();

    let message_text = payload
        .get("message")
        .and_then(|m| {
            if let Some(obj) = m.as_object() {
                obj.get("text").and_then(|t| t.as_str()).map(str::to_owned)
            } else {
                m.as_str().map(str::to_owned)
            }
        })
        .unwrap_or_default();

    let badges = extract_badges(payload);
    let is_bot = badges.contains(&BadgeKind::Bot);
    let timestamp = format_timestamp(event);
    let username_color = username_color_from_payload(payload);

    Some(ChatRow {
        timestamp,
        platform: Platform::Twitch,
        badges: if is_bot { vec![BadgeKind::Bot] } else { badges },
        username,
        username_color,
        body: ChatBody::Message(message_text),
    })
}

fn parse_subscription(event: &Event) -> Option<ChatRow> {
    let payload = event.payload.as_object()?;

    let username = payload
        .get("user_name")
        .or_else(|| payload.get("chatter_user_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_owned();

    let tier = payload
        .get("tier")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u32>().ok())
        .map(|t| {
            if t >= 3000 {
                3u8
            } else if t >= 2000 {
                2u8
            } else {
                1u8
            }
        })
        .unwrap_or(1);

    let months = payload
        .get("cumulative_months")
        .or_else(|| payload.get("duration_months"))
        .and_then(|v| v.as_u64())
        .map(|m| m as u32);

    let message = payload
        .get("message")
        .and_then(|m| {
            if let Some(obj) = m.as_object() {
                obj.get("text").and_then(|t| t.as_str()).map(str::to_owned)
            } else {
                m.as_str().map(str::to_owned)
            }
        })
        .filter(|s| !s.is_empty());

    let username_color = username_color_from_payload(payload);

    Some(ChatRow {
        timestamp: format_timestamp(event),
        platform: Platform::Twitch,
        badges: vec![],
        username,
        username_color,
        body: ChatBody::Subscription {
            tier,
            months,
            message,
            triggered_action: None,
        },
    })
}

fn parse_cheer(event: &Event) -> Option<ChatRow> {
    let payload = event.payload.as_object()?;

    let username = payload
        .get("user_name")
        .and_then(|v| v.as_str())
        .unwrap_or("anonymous")
        .to_owned();

    let bits = payload.get("bits").and_then(|v| v.as_u64()).unwrap_or(0);

    let cheer_text = payload
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();

    let username_color = username_color_from_payload(payload);

    Some(ChatRow {
        timestamp: format_timestamp(event),
        platform: Platform::Twitch,
        badges: vec![],
        username,
        username_color,
        body: ChatBody::Cheer {
            bits,
            text: cheer_text,
        },
    })
}

fn parse_raid(event: &Event) -> Option<ChatRow> {
    let payload = event.payload.as_object()?;

    let username = payload
        .get("from_broadcaster_user_name")
        .or_else(|| payload.get("raider_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_owned();

    let viewers = payload.get("viewers").and_then(|v| v.as_u64()).unwrap_or(0);

    Some(ChatRow {
        timestamp: format_timestamp(event),
        platform: Platform::Twitch,
        badges: vec![],
        username,
        username_color: Color::from_rgb8(0xf3, 0x8b, 0xa8),
        body: ChatBody::Raid {
            viewers,
            triggered_action: None,
        },
    })
}

fn format_timestamp(event: &Event) -> String {
    let secs = event.timestamp.unix_timestamp();
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn extract_badges(payload: &serde_json::Map<String, serde_json::Value>) -> Vec<BadgeKind> {
    let Some(badges_val) = payload.get("badges") else {
        return Vec::new();
    };
    let Some(arr) = badges_val.as_array() else {
        return Vec::new();
    };

    arr.iter()
        .filter_map(|b| {
            let set_id = b
                .as_object()
                .and_then(|o| o.get("set_id"))
                .and_then(|v| v.as_str())?;
            match set_id {
                "moderator" => Some(BadgeKind::Moderator),
                "vip" => Some(BadgeKind::Vip),
                "subscriber" => Some(BadgeKind::Subscriber),
                "broadcaster" => Some(BadgeKind::Broadcaster),
                _ => None,
            }
        })
        .collect()
}

fn username_color_from_payload(payload: &serde_json::Map<String, serde_json::Value>) -> Color {
    let Some(color_str) = payload.get("color").and_then(|v| v.as_str()) else {
        return Color::from_rgb(0.4, 0.7, 1.0);
    };
    if color_str.len() != 7 || !color_str.starts_with('#') {
        return Color::from_rgb(0.4, 0.7, 1.0);
    }
    let Ok(r) = u8::from_str_radix(&color_str[1..3], 16) else {
        return Color::from_rgb(0.4, 0.7, 1.0);
    };
    let Ok(g) = u8::from_str_radix(&color_str[3..5], 16) else {
        return Color::from_rgb(0.4, 0.7, 1.0);
    };
    let Ok(b) = u8::from_str_radix(&color_str[5..7], 16) else {
        return Color::from_rgb(0.4, 0.7, 1.0);
    };
    Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

pub fn filter_log<'a>(
    entries: &'a VecDeque<ChatRow>,
    filter: &ChatFilters,
) -> impl Iterator<Item = &'a ChatRow> {
    entries.iter().filter(move |row| {
        let platform_ok = match filter.platform {
            PlatformFilter::All => true,
            PlatformFilter::Twitch => row.platform == Platform::Twitch,
            PlatformFilter::YouTube => row.platform == Platform::YouTube,
            PlatformFilter::Kick => row.platform == Platform::Kick,
        };
        let events_ok = if filter.events_only {
            !matches!(row.body, ChatBody::Message(_))
        } else {
            true
        };
        let bots_ok = if filter.hide_bots {
            !row.badges.contains(&BadgeKind::Bot)
        } else {
            true
        };
        platform_ok && events_ok && bots_ok
    })
}

pub fn live_chat_view<'a>(
    state: &'a LiveChatState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let meta_bar = build_meta_bar(state, palette);
    let filter_bar = build_filter_bar(state, palette);
    let chat_area = build_chat_area(state, palette);
    let bar = forge_widgets::input_bar(
        palette,
        &state.chat_input,
        "Send to Twitch chat...",
        vec![
            PlatformTarget {
                platform: Platform::Twitch,
                active: true,
                on_press: Some(Box::new(|| Message::ChatSubmit)),
            },
            PlatformTarget {
                platform: Platform::YouTube,
                active: false,
                on_press: None,
            },
            PlatformTarget {
                platform: Platform::Kick,
                active: false,
                on_press: None,
            },
        ],
        Message::ChatInputChanged,
        Message::ChatSubmit,
    );

    iced::widget::column![meta_bar, filter_bar, chat_area, bar]
        .height(Length::Fill)
        .into()
}

fn build_meta_bar<'a>(state: &'a LiveChatState, palette: &'a ForgePalette) -> Element<'a, Message> {
    use forge_widgets::chat::chip_bg;
    use forge_widgets::tokens::{FONT_XS, Radius, radius};
    use forge_widgets::{FontRole, font};
    use iced::widget::{button, container, row, text};
    use iced::{Background, Border, Color, Length};

    let p = *palette;

    let viewer_row = row![
        container(iced::widget::Space::new().width(6.0).height(6.0))
            .width(6.0)
            .height(6.0)
            .style(move |_: &iced::Theme| container::Style {
                background: Some(Background::Color(p.success)),
                border: Border {
                    radius: 3.0.into(),
                    color: Color::TRANSPARENT,
                    width: 0.0,
                },
                ..container::Style::default()
            }),
        text("—")
            .size(FONT_XS)
            .color(palette.text_secondary)
            .font(font(FontRole::Body)),
        text(" viewers")
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Body)),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

    let sep = text("·")
        .size(FONT_XS)
        .color(palette.text_faint)
        .font(font(FontRole::Body));

    let clock_icon = tabler_icon(Icon::Clock, 11.0, palette.text_muted);

    let duration_row = row![
        clock_icon,
        text("—")
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Body)),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    let left_group = row![viewer_row, sep, duration_row]
        .spacing(10)
        .align_y(iced::Alignment::Center);

    let drawer_label = if state.drawer_open {
        "Hide viewers"
    } else {
        "Show viewers"
    };

    let drawer_bg = chip_bg(false, palette);
    let drawer_btn = button(
        row![
            tabler_icon(Icon::LayoutSidebar, 11.0, palette.text_secondary),
            text(drawer_label)
                .size(FONT_XS)
                .color(palette.text_secondary)
                .font(font(FontRole::Body)),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center),
    )
    .on_press(Message::ChatToggleDrawer)
    .padding([4, 10])
    .style(
        move |_: &iced::Theme, _status| iced::widget::button::Style {
            background: Some(Background::Color(drawer_bg)),
            border: Border {
                radius: radius(Radius::Sm).into(),
                color: p.border_regular,
                width: 0.5,
            },
            text_color: p.text_secondary,
            shadow: iced::Shadow::default(),
            snap: false,
        },
    );

    let right_group = container(drawer_btn).align_x(iced::Alignment::End);

    let inner = row![container(left_group).width(Length::Fill), right_group,]
        .align_y(iced::Alignment::Center);

    container(inner)
        .width(Length::Fill)
        .padding([6, 16])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.elevated)),
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn build_filter_bar<'a>(
    state: &'a LiveChatState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{container, row};
    use iced::{Background, Border, Length};

    let f = &state.chat_filter;
    let p = *palette;

    let platform_chips = row![
        forge_widgets::filter_chip(
            palette,
            "All",
            palette.brand,
            f.platform == PlatformFilter::All,
            Message::ChatPlatformFilter(PlatformFilter::All),
        ),
        forge_widgets::filter_chip(
            palette,
            "Twitch",
            palette.brand,
            f.platform == PlatformFilter::Twitch,
            Message::ChatPlatformFilter(PlatformFilter::Twitch),
        ),
        forge_widgets::filter_chip(
            palette,
            "YouTube",
            palette.random,
            f.platform == PlatformFilter::YouTube,
            Message::ChatPlatformFilter(PlatformFilter::YouTube),
        ),
        forge_widgets::filter_chip(
            palette,
            "Kick",
            palette.info,
            f.platform == PlatformFilter::Kick,
            Message::ChatPlatformFilter(PlatformFilter::Kick),
        ),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

    let divider = container(iced::widget::Space::new().width(0.5_f32).height(14.0))
        .width(0.5_f32)
        .height(14.0)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.border_regular)),
            ..container::Style::default()
        });

    let toggle_chips = row![
        forge_widgets::filter_chip(
            palette,
            "Events only",
            palette.disabled,
            f.events_only,
            Message::ChatToggleEventsOnly,
        ),
        forge_widgets::filter_chip(
            palette,
            "Hide bots",
            palette.disabled,
            f.hide_bots,
            Message::ChatToggleHideBots,
        ),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

    let chips_row = row![platform_chips, divider, toggle_chips]
        .spacing(6)
        .align_y(iced::Alignment::Center);

    container(chips_row)
        .width(Length::Fill)
        .padding([8, 16])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.elevated)),
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn build_chat_area<'a>(
    state: &'a LiveChatState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{container, scrollable};
    use iced::{Background, Border, Length};

    let visible: Vec<Element<'a, Message>> = filter_log(&state.chat_log, &state.chat_filter)
        .map(|row| forge_widgets::chat_row(palette, row))
        .collect();

    let empty_msg = if state.chat_filter.events_only {
        "No subscription, cheer, or raid events yet."
    } else {
        match state.chat_filter.platform {
            PlatformFilter::All => "Not connected — go to Settings → Platforms to connect Twitch.",
            PlatformFilter::Twitch => "No Twitch messages yet.",
            PlatformFilter::YouTube => "No YouTube messages yet.",
            PlatformFilter::Kick => "No Kick messages yet.",
        }
    };

    let content: Element<'a, Message> = if visible.is_empty() {
        container(forge_widgets::empty_state(
            "No messages",
            empty_msg,
            None::<(&str, Message)>,
            palette,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        let col = iced::widget::column(visible)
            .spacing(4)
            .width(Length::Fill)
            .padding([10, 16]);
        scrollable(col).height(Length::Fill).into()
    };

    let p = *palette;
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.base)),
            border: Border::default(),
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};

    fn make_chat_event(
        username: &str,
        message: &str,
        badges: serde_json::Value,
        color: &str,
    ) -> Event {
        Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({
                "chatter_user_name": username,
                "message": { "text": message },
                "badges": badges,
                "color": color,
            }),
        )
    }

    fn make_bot_event(username: &str) -> Event {
        Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({
                "chatter_user_name": username,
                "message": { "text": "beep boop" },
                "badges": [{ "set_id": "moderator" }],
                "color": "#ff0000",
                "is_bot": true,
            }),
        )
    }

    fn make_sub_event(username: &str, tier: &str, months: u64) -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.subscribe",
            serde_json::json!({
                "user_name": username,
                "tier": tier,
                "cumulative_months": months,
                "message": { "text": "Thanks!" },
                "color": "#a6e3a1",
            }),
        )
    }

    fn make_cheer_event(username: &str, bits: u64) -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.cheer",
            serde_json::json!({
                "user_name": username,
                "bits": bits,
                "message": "go go go!",
                "color": "#f9e2af",
            }),
        )
    }

    fn make_raid_event(raider: &str, viewers: u64) -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.raid",
            serde_json::json!({
                "from_broadcaster_user_name": raider,
                "viewers": viewers,
            }),
        )
    }

    #[test]
    fn chat_row_from_event_parses_twitch_message() {
        let ev = make_chat_event(
            "danylo_ua",
            "hello stream",
            serde_json::json!([{ "set_id": "moderator" }]),
            "#89dceb",
        );
        let row = chat_row_from_event(&ev).unwrap();
        assert_eq!(row.username, "danylo_ua");
        assert_eq!(row.body, ChatBody::Message("hello stream".to_owned()));
        assert_eq!(row.platform, Platform::Twitch);
        assert!(row.badges.contains(&BadgeKind::Moderator));
    }

    #[test]
    fn chat_row_from_event_ignores_non_twitch() {
        let ev = Event::new(
            EventSource::Core,
            "chat.message",
            serde_json::json!({ "chatter_user_name": "x", "message": { "text": "y" } }),
        );
        assert!(chat_row_from_event(&ev).is_none());
    }

    #[test]
    fn chat_row_from_event_ignores_unknown_kind() {
        let ev = Event::new(
            EventSource::Twitch,
            "channel.point_redemption",
            serde_json::json!({ "user_name": "x" }),
        );
        assert!(chat_row_from_event(&ev).is_none());
    }

    #[test]
    fn chat_row_from_event_parses_subscription() {
        let ev = make_sub_event("danylo_ua", "1000", 3);
        let row = chat_row_from_event(&ev).unwrap();
        assert_eq!(row.username, "danylo_ua");
        assert!(matches!(
            row.body,
            ChatBody::Subscription {
                tier: 1,
                months: Some(3),
                ..
            }
        ));
    }

    #[test]
    fn chat_row_from_event_parses_tier3_subscription() {
        let ev = make_sub_event("big_supporter", "3000", 12);
        let row = chat_row_from_event(&ev).unwrap();
        assert!(matches!(row.body, ChatBody::Subscription { tier: 3, .. }));
    }

    #[test]
    fn chat_row_from_event_parses_cheer() {
        let ev = make_cheer_event("viewer_x", 500);
        let row = chat_row_from_event(&ev).unwrap();
        assert_eq!(row.username, "viewer_x");
        assert!(matches!(row.body, ChatBody::Cheer { bits: 500, .. }));
    }

    #[test]
    fn chat_row_from_event_parses_raid() {
        let ev = make_raid_event("factorio_streamer", 42);
        let row = chat_row_from_event(&ev).unwrap();
        assert_eq!(row.username, "factorio_streamer");
        assert!(matches!(row.body, ChatBody::Raid { viewers: 42, .. }));
    }

    #[test]
    fn filter_log_all_returns_all_entries() {
        let mut log = VecDeque::new();
        log.push_back(ChatRow {
            timestamp: "00:00:00".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "a".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("hi".into()),
        });
        log.push_back(ChatRow {
            timestamp: "00:00:01".into(),
            platform: Platform::YouTube,
            badges: vec![],
            username: "b".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("yo".into()),
        });
        let f = ChatFilters::default();
        assert_eq!(filter_log(&log, &f).count(), 2);
    }

    #[test]
    fn filter_log_twitch_only_keeps_twitch_rows() {
        let mut log = VecDeque::new();
        log.push_back(ChatRow {
            timestamp: "00:00:00".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "a".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("t".into()),
        });
        log.push_back(ChatRow {
            timestamp: "00:00:01".into(),
            platform: Platform::YouTube,
            badges: vec![],
            username: "b".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("y".into()),
        });
        let f = ChatFilters {
            platform: PlatformFilter::Twitch,
            ..ChatFilters::default()
        };
        let result: Vec<_> = filter_log(&log, &f).collect();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].platform, Platform::Twitch);
    }

    #[test]
    fn filter_log_youtube_only_keeps_youtube_rows() {
        let mut log = VecDeque::new();
        log.push_back(ChatRow {
            timestamp: "00:00:00".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "a".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("t".into()),
        });
        log.push_back(ChatRow {
            timestamp: "00:00:01".into(),
            platform: Platform::YouTube,
            badges: vec![],
            username: "b".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("y".into()),
        });
        let f = ChatFilters {
            platform: PlatformFilter::YouTube,
            ..ChatFilters::default()
        };
        let result: Vec<_> = filter_log(&log, &f).collect();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].platform, Platform::YouTube);
    }

    #[test]
    fn filter_log_kick_only_keeps_kick_rows() {
        let mut log = VecDeque::new();
        log.push_back(ChatRow {
            timestamp: "00:00:00".into(),
            platform: Platform::Kick,
            badges: vec![],
            username: "k".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("kick chat".into()),
        });
        log.push_back(ChatRow {
            timestamp: "00:00:01".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "t".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("twitch chat".into()),
        });
        let f = ChatFilters {
            platform: PlatformFilter::Kick,
            ..ChatFilters::default()
        };
        let result: Vec<_> = filter_log(&log, &f).collect();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].platform, Platform::Kick);
    }

    #[test]
    fn filter_log_hide_bots_removes_bot_rows() {
        let mut log = VecDeque::new();
        log.push_back(ChatRow {
            timestamp: "00:00:00".into(),
            platform: Platform::Twitch,
            badges: vec![BadgeKind::Bot],
            username: "nightbot".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("beep".into()),
        });
        log.push_back(ChatRow {
            timestamp: "00:00:01".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "viewer".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("hi".into()),
        });
        let f = ChatFilters {
            hide_bots: true,
            ..ChatFilters::default()
        };
        let result: Vec<_> = filter_log(&log, &f).collect();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].username, "viewer");
    }

    #[test]
    fn filter_log_events_only_hides_messages() {
        let mut log = VecDeque::new();
        log.push_back(ChatRow {
            timestamp: "00:00:00".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "msg_user".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("regular chat".into()),
        });
        log.push_back(ChatRow {
            timestamp: "00:00:01".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "sub_user".into(),
            username_color: Color::WHITE,
            body: ChatBody::Subscription {
                tier: 1,
                months: None,
                message: None,
                triggered_action: None,
            },
        });
        log.push_back(ChatRow {
            timestamp: "00:00:02".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "raider".into(),
            username_color: Color::WHITE,
            body: ChatBody::Raid {
                viewers: 10,
                triggered_action: None,
            },
        });
        let f = ChatFilters {
            events_only: true,
            ..ChatFilters::default()
        };
        let result: Vec<_> = filter_log(&log, &f).collect();
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0].body, ChatBody::Subscription { .. }));
        assert!(matches!(result[1].body, ChatBody::Raid { .. }));
    }

    #[test]
    fn filter_log_events_only_and_hide_bots_combine() {
        let mut log = VecDeque::new();
        log.push_back(ChatRow {
            timestamp: "00:00:00".into(),
            platform: Platform::Twitch,
            badges: vec![BadgeKind::Bot],
            username: "nightbot".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("bot msg".into()),
        });
        log.push_back(ChatRow {
            timestamp: "00:00:01".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "cheerer".into(),
            username_color: Color::WHITE,
            body: ChatBody::Cheer {
                bits: 100,
                text: "gg".into(),
            },
        });
        let f = ChatFilters {
            events_only: true,
            hide_bots: true,
            ..ChatFilters::default()
        };
        let result: Vec<_> = filter_log(&log, &f).collect();
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].body, ChatBody::Cheer { bits: 100, .. }));
    }

    #[test]
    fn chat_filters_default_state() {
        let f = ChatFilters::default();
        assert_eq!(f.platform, PlatformFilter::All);
        assert!(!f.events_only);
        assert!(!f.hide_bots);
    }

    #[test]
    fn platform_filter_default_is_all() {
        assert_eq!(PlatformFilter::default(), PlatformFilter::All);
    }

    #[test]
    fn live_chat_state_new_has_seed_rows() {
        let state = LiveChatState::new();
        assert!(!state.chat_log.is_empty());
        assert!(!state.drawer_open);
    }

    #[test]
    fn chat_log_bounded_at_max() {
        let mut log: VecDeque<ChatRow> = VecDeque::new();
        for i in 0..=CHAT_LOG_MAX {
            log.push_back(ChatRow {
                timestamp: format!("{i:08}"),
                platform: Platform::Twitch,
                badges: vec![],
                username: format!("user{i}"),
                username_color: Color::WHITE,
                body: ChatBody::Message(format!("msg{i}")),
            });
            if log.len() > CHAT_LOG_MAX {
                log.pop_front();
            }
        }
        assert_eq!(log.len(), CHAT_LOG_MAX);
    }

    #[test]
    fn chat_row_from_event_handles_missing_color_gracefully() {
        let ev = Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({
                "chatter_user_name": "nocoloruser",
                "message": { "text": "hey" },
                "badges": [],
            }),
        );
        let row = chat_row_from_event(&ev).unwrap();
        assert_eq!(row.username, "nocoloruser");
    }

    #[test]
    fn chat_row_from_event_handles_invalid_hex_color_gracefully() {
        let ev = make_chat_event("badcolor", "msg", serde_json::json!([]), "#ZZZZZZ");
        let row = chat_row_from_event(&ev).unwrap();
        assert_eq!(row.username, "badcolor");
    }

    #[test]
    fn chat_row_from_event_assigns_mod_badge() {
        let ev = make_chat_event(
            "mod_user",
            "modding",
            serde_json::json!([{ "set_id": "moderator" }]),
            "#00ff00",
        );
        let row = chat_row_from_event(&ev).unwrap();
        assert!(row.badges.contains(&BadgeKind::Moderator));
    }

    #[test]
    fn chat_row_from_event_marks_bot_badge() {
        let ev = make_bot_event("coolbot");
        let row = chat_row_from_event(&ev).unwrap();
        assert_eq!(row.username, "coolbot");
    }
}
