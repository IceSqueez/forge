use std::collections::VecDeque;

use forge_events::{Event, EventSource};
use forge_widgets::{BadgeKind, ChatBody, ChatRow, ForgePalette, Platform, PlatformTarget};
use iced::{Color, Element, Length};

use crate::Message;

pub const CHAT_LOG_MAX: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ChatFilter {
    #[default]
    All,
    TwitchOnly,
    HideBots,
}

pub struct LiveChatState {
    pub chat_log: VecDeque<ChatRow>,
    pub chat_input: String,
    pub chat_filter: ChatFilter,
}

impl LiveChatState {
    pub fn new() -> Self {
        Self {
            chat_log: VecDeque::new(),
            chat_input: String::new(),
            chat_filter: ChatFilter::default(),
        }
    }
}

impl Default for LiveChatState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn chat_row_from_event(event: &Event) -> Option<ChatRow> {
    if event.source != EventSource::Twitch || event.kind != "chat.message" {
        return None;
    }

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

    let timestamp = {
        let secs = event.timestamp.unix_timestamp();
        let h = (secs / 3600) % 24;
        let m = (secs / 60) % 60;
        let s = secs % 60;
        format!("{h:02}:{m:02}:{s:02}")
    };

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
    filter: &ChatFilter,
) -> impl Iterator<Item = &'a ChatRow> {
    entries.iter().filter(move |row| match filter {
        ChatFilter::All => true,
        ChatFilter::TwitchOnly => row.platform == Platform::Twitch,
        ChatFilter::HideBots => !row.badges.contains(&BadgeKind::Bot),
    })
}

pub fn live_chat_view<'a>(
    state: &'a LiveChatState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
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

    iced::widget::column![filter_bar, chat_area, bar]
        .height(Length::Fill)
        .into()
}

fn build_filter_bar<'a>(
    state: &'a LiveChatState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let chips = iced::widget::row![
        forge_widgets::filter_chip(
            palette,
            "All",
            palette.brand,
            state.chat_filter == ChatFilter::All,
            Message::ChatFilterChanged(ChatFilter::All),
        ),
        forge_widgets::filter_chip(
            palette,
            "Twitch",
            palette.brand,
            state.chat_filter == ChatFilter::TwitchOnly,
            Message::ChatFilterChanged(ChatFilter::TwitchOnly),
        ),
        forge_widgets::filter_chip(
            palette,
            "YouTube",
            palette.random,
            false,
            Message::ChatFilterChanged(ChatFilter::All),
        ),
        forge_widgets::filter_chip(
            palette,
            "Kick",
            palette.info,
            false,
            Message::ChatFilterChanged(ChatFilter::All),
        ),
        forge_widgets::filter_chip(
            palette,
            "Hide bots",
            palette.disabled,
            state.chat_filter == ChatFilter::HideBots,
            Message::ChatFilterChanged(ChatFilter::HideBots),
        ),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

    let p = *palette;
    iced::widget::container(chips)
        .width(Length::Fill)
        .padding([8, 16])
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.elevated)),
            border: iced::Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn build_chat_area<'a>(
    state: &'a LiveChatState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let visible: Vec<Element<'a, Message>> = filter_log(&state.chat_log, &state.chat_filter)
        .map(|row| forge_widgets::chat_row(palette, row))
        .collect();

    let content: Element<'a, Message> = if visible.is_empty() {
        let msg = match &state.chat_filter {
            ChatFilter::All => "Not connected — go to Settings → Platforms to connect Twitch.",
            ChatFilter::TwitchOnly => "No Twitch messages yet.",
            ChatFilter::HideBots => "No non-bot messages yet.",
        };
        iced::widget::container(forge_widgets::empty_state(
            "No messages",
            msg,
            None::<(&str, Message)>,
            palette,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        let col = iced::widget::column(visible)
            .spacing(6)
            .width(Length::Fill)
            .padding([10, 16]);
        iced::widget::scrollable(col).height(Length::Fill).into()
    };

    let p = *palette;
    iced::widget::container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.base)),
            ..iced::widget::container::Style::default()
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
    fn chat_row_from_event_ignores_non_chat_kind() {
        let ev = Event::new(
            EventSource::Twitch,
            "channel.subscribe",
            serde_json::json!({ "chatter_user_name": "x", "message": { "text": "y" } }),
        );
        assert!(chat_row_from_event(&ev).is_none());
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
        assert_eq!(filter_log(&log, &ChatFilter::All).count(), 2);
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
        let result: Vec<_> = filter_log(&log, &ChatFilter::TwitchOnly).collect();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].platform, Platform::Twitch);
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
        let result: Vec<_> = filter_log(&log, &ChatFilter::HideBots).collect();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].username, "viewer");
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
