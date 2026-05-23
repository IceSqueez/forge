use std::collections::{HashSet, VecDeque};

use forge_events::{Event, EventSource};
use forge_widgets::{
    BadgeKind, ChatBody, ChatRow, ForgePalette, Icon, Platform, PlatformTarget, search_input,
    tabler_icon,
};
use iced::{Color, Element, Length, Task};
use time::OffsetDateTime;

use crate::Message;
use crate::message::LiveChatMsg;
use crate::runtime_view::RuntimeView;
use crate::viewers::ViewersState;

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
    pub drawer_menu_open: bool,
    pub drawer_search: String,
    pub selected_viewer: Option<String>,
    pub auto_scroll: bool,
    pub unread_count: u32,
    pub emoji_picker_open: bool,
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
            drawer_menu_open: false,
            drawer_search: String::new(),
            selected_viewer: None,
            auto_scroll: true,
            unread_count: 0,
            emoji_picker_open: false,
        }
    }
}

impl Default for LiveChatState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn update(state: &mut LiveChatState, rt: &RuntimeView, msg: LiveChatMsg) -> Task<Message> {
    match msg {
        LiveChatMsg::InputChanged(s) => {
            state.chat_input = s;
            Task::none()
        }
        LiveChatMsg::Submit => {
            let msg = std::mem::take(&mut state.chat_input);
            let msg = msg.trim().to_owned();
            if msg.is_empty() {
                return Task::none();
            }
            rt.bus.publish(Event::new(
                EventSource::Core,
                "chat.send.request",
                serde_json::json!({"target": "twitch", "message": msg}),
            ));
            Task::none()
        }
        LiveChatMsg::PlatformFilter(platform) => {
            state.chat_filter.platform = platform;
            Task::none()
        }
        LiveChatMsg::ToggleEventsOnly => {
            state.chat_filter.events_only = !state.chat_filter.events_only;
            Task::none()
        }
        LiveChatMsg::ToggleHideBots => {
            state.chat_filter.hide_bots = !state.chat_filter.hide_bots;
            Task::none()
        }
        LiveChatMsg::ToggleDrawer => {
            state.drawer_open = !state.drawer_open;
            Task::none()
        }
        LiveChatMsg::Scrolled(viewport) => {
            let rel = viewport.relative_offset();
            let at_bottom = rel.y >= 0.98;
            state.auto_scroll = at_bottom;
            if at_bottom {
                state.unread_count = 0;
            }
            Task::none()
        }
        LiveChatMsg::ScrollToBottom => {
            state.auto_scroll = true;
            state.unread_count = 0;
            iced::widget::operation::snap_to_end(chat_scroll_id())
        }
        LiveChatMsg::ToggleEmoji => {
            state.emoji_picker_open = !state.emoji_picker_open;
            Task::none()
        }
        LiveChatMsg::DrawerSearchChanged(s) => {
            state.drawer_search = s;
            Task::none()
        }
        LiveChatMsg::DrawerSelectViewer(name) => {
            state.selected_viewer = Some(name);
            state.drawer_open = true;
            Task::none()
        }
        LiveChatMsg::DrawerMenuToggle => {
            state.drawer_menu_open = !state.drawer_menu_open;
            Task::none()
        }
        LiveChatMsg::DrawerMenuDismiss => {
            state.drawer_menu_open = false;
            Task::none()
        }
    }
}

pub fn chat_scroll_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("forge:chat_scroll")
}

pub fn on_event(state: &mut LiveChatState, event: &Event) -> Task<Message> {
    let Some(row) = chat_row_from_event(event) else {
        return Task::none();
    };
    state.chat_log.push_back(row);
    if state.chat_log.len() > CHAT_LOG_MAX {
        state.chat_log.pop_front();
    }
    if state.auto_scroll {
        iced::widget::operation::snap_to_end(chat_scroll_id())
    } else {
        state.unread_count = state.unread_count.saturating_add(1);
        Task::none()
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

struct ViewerSummary {
    username: String,
    role: Option<BadgeKind>,
    message_count: u64,
    last_seen_label: String,
    avatar_letter: char,
    avatar_color: Color,
    watch_time: String,
    sub: String,
    follow: String,
}

fn drawer_matches(username: &str, search: &str) -> bool {
    if search.is_empty() {
        return true;
    }
    username.to_ascii_lowercase().contains(search)
}

fn synthesize_from_chat(
    username: &str,
    chat_log: &VecDeque<ChatRow>,
    palette: &ForgePalette,
) -> Option<ViewerSummary> {
    let count = chat_log.iter().filter(|r| r.username == username).count();
    if count == 0 {
        return None;
    }
    let last_entry = chat_log.iter().rev().find(|r| r.username == username)?;
    let role = last_entry.badges.first().copied();
    let avatar_letter = username
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('?');
    let avatar_color = viewer_hash_color(username, palette);

    let sub = if role == Some(BadgeKind::Broadcaster) {
        "∞".into()
    } else if role == Some(BadgeKind::Subscriber) {
        "Yes".into()
    } else {
        "No".into()
    };

    Some(ViewerSummary {
        username: username.to_owned(),
        role,
        message_count: count as u64,
        last_seen_label: "now".into(),
        avatar_letter,
        avatar_color,
        watch_time: "0m".into(),
        sub,
        follow: "Yes".into(),
    })
}

fn enrich_with_storage(mut summary: ViewerSummary, viewers: &ViewersState) -> ViewerSummary {
    if let Some(v) = viewers
        .viewers
        .iter()
        .find(|v| v.username == summary.username)
    {
        summary.message_count = v.message_count;
        summary.last_seen_label = viewer_last_seen(v.last_seen_at);

        let duration = v.last_seen_at - v.first_seen_at;
        let sec_count_based = v.message_count * 90;
        let seconds = duration.whole_seconds().max(sec_count_based as i64).max(0);

        summary.watch_time = if seconds < 60 {
            format!("{}s", seconds)
        } else if seconds < 3600 {
            format!("{}m", seconds / 60)
        } else {
            format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
        };

        let age = OffsetDateTime::now_utc() - v.first_seen_at;
        let days = age.whole_days().max(0);
        summary.follow = if v.message_count == 0 {
            "No".into()
        } else if days >= 365 {
            format!("{}y", days / 365)
        } else if days >= 30 {
            format!("{}mo", days / 30)
        } else if days >= 1 {
            format!("{}d", days)
        } else {
            let hours = age.whole_hours().max(0);
            if hours >= 1 {
                format!("{}h", hours)
            } else {
                let mins = age.whole_minutes().max(1);
                format!("{}m", mins)
            }
        };

        let is_broadcaster = summary.role == Some(BadgeKind::Broadcaster);
        let is_subscriber = summary.role == Some(BadgeKind::Subscriber);
        summary.sub = if is_broadcaster {
            "∞".into()
        } else if is_subscriber {
            let age_months = (days / 30).max(1);
            format!("{}mo", age_months)
        } else {
            "No".into()
        };
    }
    summary
}

fn effective_summary(
    state: &LiveChatState,
    viewers: &ViewersState,
    palette: &ForgePalette,
) -> Option<ViewerSummary> {
    if let Some(sel) = state.selected_viewer.as_deref()
        && let Some(s) = synthesize_from_chat(sel, &state.chat_log, palette)
    {
        return Some(enrich_with_storage(s, viewers));
    }
    let last = state.chat_log.back()?.username.as_str();
    synthesize_from_chat(last, &state.chat_log, palette).map(|s| enrich_with_storage(s, viewers))
}

fn viewer_hash_color(username: &str, palette: &ForgePalette) -> Color {
    let idx = username
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_add(u32::from(b))) as usize
        % 8;
    [
        palette.brand,
        palette.success,
        palette.warning,
        palette.info,
        palette.random,
        palette.bits,
        palette.accent_pink_light,
        palette.accent_teal,
    ][idx]
}

fn viewer_last_seen(dt: OffsetDateTime) -> String {
    let secs = (OffsetDateTime::now_utc() - dt).whole_seconds().max(0);
    if secs < 5 {
        "now".into()
    } else if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{} min", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

fn viewer_avatar<'a, Msg: 'a>(
    letter: char,
    color: Color,
    size: f32,
    border_radius: forge_widgets::tokens::Radius,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    use forge_widgets::tokens::radius;
    use forge_widgets::{FontRole, font};
    use iced::widget::{container, text};
    use iced::{Background, Border};

    let p = *palette;
    let r = radius(border_radius);
    container(
        text(letter.to_string())
            .font(font(FontRole::Body))
            .size(size * 0.45)
            .color(p.shell),
    )
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .center_x(Length::Fixed(size))
    .center_y(Length::Fixed(size))
    .style(move |_t: &iced::Theme| container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: r.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn viewer_stat<'a, Msg: 'a>(
    label: &str,
    value: &str,
    color: Color,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    use forge_widgets::{
        FontRole, font,
        tokens::{FONT_XS, Radius, radius},
    };
    use iced::widget::{column, container, text};
    use iced::{Background, Border};

    let l = label.to_owned();
    let v = value.to_owned();
    let p = *palette;

    let label_el = text(l)
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_muted);

    let value_el = text(v)
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(color);

    container(column![label_el, value_el].spacing(2))
        .padding([6u16, 8u16])
        .width(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(Background::Color(p.base)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: radius(Radius::Sm).into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn drawer_role_badge<'a>(kind: BadgeKind, palette: &ForgePalette) -> Element<'a, Message> {
    use forge_widgets::tokens::FONT_XS;
    use forge_widgets::{FontRole, font};
    use iced::widget::{container, text};
    use iced::{Background, Border};

    let (label, text_color) = match kind {
        BadgeKind::Broadcaster => ("LIVE", palette.warning),
        BadgeKind::Moderator => ("MOD", palette.success),
        BadgeKind::Vip => ("VIP", palette.brand),
        BadgeKind::Subscriber => ("SUB", palette.info),
        BadgeKind::Bot => ("BOT", palette.text_muted),
    };
    let p = *palette;
    container(
        text(label)
            .size(FONT_XS)
            .color(text_color)
            .font(font(FontRole::Body)),
    )
    .padding([1u16, 5u16])
    .style(move |_t: &iced::Theme| container::Style {
        background: Some(Background::Color(p.surface_overlay)),
        border: Border {
            color: p.border_regular,
            width: 0.5,
            radius: 3.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn section_sep<'a, Msg: 'a>(palette: &'a ForgePalette) -> Element<'a, Msg> {
    use iced::Background;
    use iced::widget::{Space, container};

    let p = *palette;
    container(Space::new())
        .height(0.5_f32)
        .width(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(Background::Color(p.border_regular)),
            ..container::Style::default()
        })
        .into()
}

fn drawer_header<'a>(state: &'a LiveChatState, palette: &'a ForgePalette) -> Element<'a, Message> {
    use forge_widgets::{FontRole, font, tokens::FONT_XS};
    use iced::widget::{column, container, text};
    use iced::{Background, Border, Length};

    let p = *palette;

    let mut seen = HashSet::new();
    let unique: Vec<&str> = state
        .chat_log
        .iter()
        .rev()
        .filter_map(|r| {
            if seen.insert(r.username.as_str()) {
                Some(r.username.as_str())
            } else {
                None
            }
        })
        .collect();
    let total_count = unique.len();
    let search_lower = state.drawer_search.to_ascii_lowercase();
    let shown_count = unique
        .iter()
        .filter(|u| drawer_matches(u, &search_lower))
        .count();

    let count_label = format!("{total_count} active · {shown_count} shown");

    let count_row = text(count_label)
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_faint);

    let search_box = search_input(
        "Search viewers...",
        &state.drawer_search,
        |s| Message::LiveChat(LiveChatMsg::DrawerSearchChanged(s)),
        palette,
    );

    let header_content = column![count_row, search_box].spacing(8);

    let body = container(header_content)
        .padding([10u16, 14u16])
        .width(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    iced::widget::column![body, section_sep::<Message>(palette)]
        .spacing(0)
        .into()
}

fn selected_viewer_detail<'a>(
    state: &'a LiveChatState,
    viewers: &'a ViewersState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::{
        FontRole, MenuItem, MenuPlacement, font,
        icons::Icon,
        menu_button, tabler_icon,
        tokens::{FONT_SM, FONT_XS, Radius, radius},
    };
    use iced::widget::{Space, button, column, container, row, text};
    use iced::{Background, Border};

    let p = *palette;

    let sep = section_sep::<Message>(palette);

    let Some(summary) = effective_summary(state, viewers, palette) else {
        let placeholder = container(
            text("Click a username in chat to see details")
                .font(font(FontRole::Body))
                .size(FONT_XS)
                .color(p.text_faint),
        )
        .padding([14u16, 14u16])
        .width(Length::Fill);
        return column![placeholder, sep].spacing(0).into();
    };

    let avatar_el = viewer_avatar::<Message>(
        summary.avatar_letter,
        summary.avatar_color,
        38.0,
        Radius::Md,
        palette,
    );

    let name_el = text(summary.username.clone())
        .font(font(FontRole::Body))
        .size(FONT_SM)
        .color(p.text_primary);

    let last_label = format!("Last seen {}", summary.last_seen_label);
    let last_el = text(last_label)
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_muted);

    let mut name_row_items: Vec<Element<'a, Message>> = vec![name_el.into()];
    if let Some(kind) = summary.role {
        name_row_items.push(drawer_role_badge(kind, palette));
    }
    let name_row = row(name_row_items)
        .spacing(6)
        .align_y(iced::Alignment::Center);

    let name_col = column![name_row, last_el].spacing(2);

    let info_row = row![avatar_el, name_col]
        .spacing(10)
        .align_y(iced::Alignment::Center);

    let msg_str = format!("{}", summary.message_count);

    let sub_color = if summary.sub == "No" {
        p.text_faint
    } else {
        p.text_primary
    };
    let follow_color = if summary.follow == "No" {
        p.text_faint
    } else {
        p.text_primary
    };
    let watch_color = if summary.watch_time == "0m" || summary.watch_time == "—" {
        p.text_faint
    } else {
        p.text_primary
    };

    let stat_grid = column![
        row![
            viewer_stat("WATCH TIME", &summary.watch_time, watch_color, palette),
            viewer_stat("MESSAGES", &msg_str, p.text_primary, palette),
        ]
        .spacing(6),
        row![
            viewer_stat("SUB", &summary.sub, sub_color, palette),
            viewer_stat("FOLLOW", &summary.follow, follow_color, palette),
        ]
        .spacing(6),
    ]
    .spacing(6);

    let btn_style = move |_t: &iced::Theme, s: button::Status| {
        let hovered = matches!(s, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border {
                color: if hovered {
                    p.border_input
                } else {
                    p.border_regular
                },
                width: 0.5,
                radius: radius(Radius::Sm).into(),
            },
            text_color: if hovered {
                p.text_primary
            } else {
                p.text_secondary
            },
            shadow: iced::Shadow::default(),
            snap: false,
        }
    };

    let shoutout_btn = button(
        container(
            row![
                tabler_icon(Icon::Bolt, 11.0, p.text_muted),
                text("Shoutout").font(font(FontRole::Body)).size(FONT_XS),
            ]
            .spacing(5)
            .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .align_x(iced::Alignment::Center),
    )
    .on_press(Message::Noop)
    .padding([5u16, 11u16])
    .width(Length::Fill)
    .style(btn_style);

    let whisper_btn = button(
        container(
            row![
                tabler_icon(Icon::MessageCircle, 11.0, p.text_muted),
                text("Whisper").font(font(FontRole::Body)).size(FONT_XS),
            ]
            .spacing(5)
            .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .align_x(iced::Alignment::Center),
    )
    .on_press(Message::Noop)
    .padding([5u16, 11u16])
    .width(Length::Fill)
    .style(btn_style);

    let menu_items: Vec<MenuItem<Message>> = vec![
        MenuItem::Item {
            label: "Shoutout".into(),
            on_press: Message::Noop,
            icon: Some(Icon::Flag),
            shortcut: None,
            color: None,
            disabled: false,
        },
        MenuItem::Item {
            label: "Whisper".into(),
            on_press: Message::Noop,
            icon: Some(Icon::MessageCircle),
            shortcut: None,
            color: None,
            disabled: false,
        },
        MenuItem::Item {
            label: "Set TTS voice\u{2026}".into(),
            on_press: Message::Noop,
            icon: None,
            shortcut: None,
            color: None,
            disabled: false,
        },
        MenuItem::Divider,
        MenuItem::Item {
            label: "Block from TTS".into(),
            on_press: Message::Noop,
            icon: None,
            shortcut: None,
            color: Some(p.warning),
            disabled: false,
        },
        MenuItem::Item {
            label: "Timeout 10 min".into(),
            on_press: Message::Noop,
            icon: None,
            shortcut: None,
            color: Some(p.warning),
            disabled: false,
        },
        MenuItem::Item {
            label: "Ban from channel".into(),
            on_press: Message::Noop,
            icon: None,
            shortcut: None,
            color: Some(p.random),
            disabled: false,
        },
    ];

    let more_btn = menu_button(
        Icon::DotsVertical,
        state.drawer_menu_open,
        Message::LiveChat(LiveChatMsg::DrawerMenuToggle),
        Message::LiveChat(LiveChatMsg::DrawerMenuDismiss),
        menu_items,
        MenuPlacement::TopRight,
        palette,
    );

    let actions_row = row![shoutout_btn, whisper_btn, more_btn].spacing(6);

    let detail_content = column![info_row, stat_grid, actions_row].spacing(8);

    let detail_box = container(detail_content)
        .padding([14u16, 14u16])
        .width(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

    column![detail_box, Space::new().height(0.0_f32), sep,]
        .spacing(0)
        .into()
}

fn drawer_viewer_row<'a>(
    summary: ViewerSummary,
    is_sel: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::{
        FontRole, font,
        tokens::{FONT_SM, FONT_XS, Radius},
    };
    use iced::widget::{Space, button, column, container, row, text};
    use iced::{Background, Border};

    let p = *palette;
    let stripe_color = if is_sel { p.brand } else { Color::TRANSPARENT };
    let selected_bg = p.elevated;
    let hover_bg = p.base;

    let stripe = container(Space::new().width(2).height(Length::Fill))
        .width(2)
        .height(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(Background::Color(stripe_color)),
            ..container::Style::default()
        });

    let avatar_el = viewer_avatar::<Message>(
        summary.avatar_letter,
        summary.avatar_color,
        22.0,
        Radius::Sm,
        palette,
    );

    let name_el = text(summary.username.clone())
        .font(font(FontRole::Body))
        .size(FONT_SM)
        .color(p.text_primary);

    let mut name_row_items: Vec<Element<'a, Message>> = vec![name_el.into()];
    if let Some(kind) = summary.role {
        name_row_items.push(drawer_role_badge(kind, palette));
    }
    let name_row = row(name_row_items)
        .spacing(5)
        .align_y(iced::Alignment::Center);

    let meta_el = text(format!(
        "{} · {} msg",
        summary.watch_time, summary.message_count
    ))
    .font(font(FontRole::Monospace))
    .size(FONT_XS)
    .color(p.text_muted);

    let name_col = column![name_row, meta_el].spacing(1);

    let last_el = text(summary.last_seen_label.clone())
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_faint);

    let row_content = row![
        avatar_el,
        name_col,
        Space::new().width(Length::Fill),
        last_el,
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);

    let username = summary.username.clone();
    let row_btn = button(row_content)
        .on_press(Message::LiveChat(LiveChatMsg::DrawerSelectViewer(username)))
        .padding([7u16, 14u16])
        .width(Length::Fill)
        .style(move |_t: &iced::Theme, s: button::Status| {
            let bg = if is_sel {
                selected_bg
            } else if matches!(s, button::Status::Hovered | button::Status::Pressed) {
                hover_bg
            } else {
                Color::TRANSPARENT
            };
            button::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 0.0.into(),
                },
                text_color: p.text_primary,
                shadow: iced::Shadow::default(),
                snap: false,
            }
        });

    row![stripe, row_btn].spacing(0).into()
}

fn viewer_list<'a>(
    state: &'a LiveChatState,
    viewers: &'a ViewersState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::tokens::FONT_XS;
    use forge_widgets::{FontRole, font};
    use iced::Background;
    use iced::widget::{Space, column, container, scrollable, text};

    let p = *palette;

    let search_lower = state.drawer_search.to_ascii_lowercase();

    let mut seen: HashSet<&str> = HashSet::new();
    let unique_usernames: Vec<&str> = state
        .chat_log
        .iter()
        .rev()
        .filter_map(|r| {
            if seen.insert(r.username.as_str()) {
                Some(r.username.as_str())
            } else {
                None
            }
        })
        .filter(|u| drawer_matches(u, &search_lower))
        .collect();

    let section_label = format!("ACTIVE NOW · {}", unique_usernames.len());

    let section_header = container(
        text(section_label)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(p.text_faint),
    )
    .padding([8u16, 14u16])
    .width(Length::Fill)
    .style(move |_t: &iced::Theme| container::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        ..container::Style::default()
    });

    let selected_name = state.selected_viewer.as_deref();

    let list_items: Vec<Element<'a, Message>> = unique_usernames
        .iter()
        .filter_map(|username| {
            synthesize_from_chat(username, &state.chat_log, palette)
                .map(|s| enrich_with_storage(s, viewers))
        })
        .map(|summary| {
            let is_sel = selected_name == Some(summary.username.as_str());
            drawer_viewer_row(summary, is_sel, palette)
        })
        .collect();

    let list_col = if list_items.is_empty() {
        column![
            Space::new().height(8.0_f32),
            container(
                text("No chat participants match the search")
                    .font(font(FontRole::Body))
                    .size(FONT_XS)
                    .color(p.text_faint),
            )
            .padding([0u16, 14u16]),
        ]
        .spacing(0)
    } else {
        iced::widget::column(list_items).spacing(0)
    };

    column![section_header, scrollable(list_col).height(Length::Fill)]
        .height(Length::Fill)
        .into()
}

fn drawer_panel<'a>(
    state: &'a LiveChatState,
    viewers: &'a ViewersState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::column;

    column![
        drawer_header(state, palette),
        selected_viewer_detail(state, viewers, palette),
        viewer_list(state, viewers, palette),
    ]
    .height(Length::Fill)
    .into()
}

pub fn live_chat_view<'a>(
    state: &'a LiveChatState,
    viewers: &'a ViewersState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let page_header = live_chat_page_header(state, palette);
    let chat_area = build_chat_area(state, palette);
    let bar = forge_widgets::input_bar(
        palette,
        &state.chat_input,
        "Send to Twitch chat...",
        vec![
            PlatformTarget {
                platform: Platform::Twitch,
                active: true,
                on_press: Some(Box::new(|| Message::LiveChat(LiveChatMsg::Submit))),
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
        |s| Message::LiveChat(LiveChatMsg::InputChanged(s)),
        Message::LiveChat(LiveChatMsg::Submit),
        state.emoji_picker_open,
        Message::LiveChat(LiveChatMsg::ToggleEmoji),
    );

    let chat_column = iced::widget::column![chat_area, bar]
        .width(Length::Fill)
        .height(Length::Fill);

    let body: Element<'a, Message> = if state.drawer_open {
        let panel_content = drawer_panel(state, viewers, palette);
        let chrome = crate::app::sheet_chrome(
            "Viewers",
            Message::LiveChat(LiveChatMsg::ToggleDrawer),
            panel_content,
            None,
            palette,
        );
        let sheet = forge_widgets::side_sheet(
            chrome,
            Message::LiveChat(LiveChatMsg::ToggleDrawer),
            forge_widgets::SheetEdge::Right,
            480.0,
            palette,
        );
        iced::widget::stack![chat_column, sheet].into()
    } else {
        chat_column.into()
    };

    iced::widget::column![page_header, body]
        .height(Length::Fill)
        .into()
}

fn live_chat_page_header<'a>(
    state: &'a LiveChatState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::tokens::{FONT_SM, FONT_XS};
    use iced::widget::{button, container, row, text};
    use iced::{Background, Border};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);

    let crumbs_left = row![
        tabler_icon(Icon::Home, 13.0, p.text_faint),
        tabler_icon(Icon::ChevronRight, 11.0, p.text_faint),
        text("Audience").size(FONT_SM).color(p.text_muted),
        tabler_icon(Icon::ChevronRight, 11.0, p.text_faint),
        text("Chat").size(FONT_SM).color(p.text_primary),
    ]
    .spacing(8)
    .align_y(iced::alignment::Vertical::Center);

    let chip_all = forge_widgets::filter_chip(
        palette,
        "All",
        p.brand,
        state.chat_filter.platform == PlatformFilter::All,
        Message::LiveChat(LiveChatMsg::PlatformFilter(PlatformFilter::All)),
    );
    let chip_twitch = forge_widgets::filter_chip(
        palette,
        "Twitch",
        p.brand,
        state.chat_filter.platform == PlatformFilter::Twitch,
        Message::LiveChat(LiveChatMsg::PlatformFilter(PlatformFilter::Twitch)),
    );
    let chip_youtube = forge_widgets::filter_chip(
        palette,
        "YouTube",
        p.random,
        state.chat_filter.platform == PlatformFilter::YouTube,
        Message::LiveChat(LiveChatMsg::PlatformFilter(PlatformFilter::YouTube)),
    );
    let chip_kick = forge_widgets::filter_chip(
        palette,
        "Kick",
        p.info,
        state.chat_filter.platform == PlatformFilter::Kick,
        Message::LiveChat(LiveChatMsg::PlatformFilter(PlatformFilter::Kick)),
    );
    let chips = row![chip_all, chip_twitch, chip_youtube, chip_kick].spacing(4);

    let divider = container(iced::widget::Space::new().width(0.5).height(16.0))
        .width(0.5)
        .height(16.0)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.border_regular)),
            ..container::Style::default()
        });

    let viewer_count_str = format!("{} viewers", state.chat_log.len());
    let viewer_info = row![
        text(viewer_count_str)
            .size(FONT_XS)
            .color(p.text_secondary)
            .font(mono),
    ]
    .spacing(6)
    .align_y(iced::alignment::Vertical::Center);

    let drawer_label = if state.drawer_open {
        "Hide viewers"
    } else {
        "Show viewers"
    };
    let drawer_btn = button(
        row![
            tabler_icon(Icon::Users, 11.0, p.text_secondary),
            text(drawer_label).size(FONT_XS).color(p.text_secondary),
        ]
        .spacing(4)
        .align_y(iced::alignment::Vertical::Center),
    )
    .on_press(Message::LiveChat(LiveChatMsg::ToggleDrawer))
    .padding([4_u16, 10_u16])
    .style(move |_: &iced::Theme, status| {
        let hovered = matches!(
            status,
            iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
        );
        iced::widget::button::Style {
            background: Some(Background::Color(iced::Color::TRANSPARENT)),
            border: Border {
                color: if hovered {
                    p.border_input
                } else {
                    p.border_regular
                },
                width: 0.5,
                radius: forge_widgets::radius(forge_widgets::Radius::Sm).into(),
            },
            text_color: if hovered {
                p.text_primary
            } else {
                p.text_secondary
            },
            shadow: iced::Shadow::default(),
            snap: false,
        }
    });

    let right_side = row![chips, divider, viewer_info, drawer_btn]
        .spacing(8)
        .align_y(iced::alignment::Vertical::Center);

    let inner = row![
        crumbs_left,
        iced::widget::Space::new().width(Length::Fill),
        right_side,
    ]
    .align_y(iced::alignment::Vertical::Center);

    container(inner)
        .width(Length::Fill)
        .padding([10_u16, 16_u16])
        .style(move |_: &iced::Theme| container::Style {
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

fn select_viewer_msg(name: String) -> Message {
    Message::LiveChat(LiveChatMsg::DrawerSelectViewer(name))
}

fn build_chat_area<'a>(
    state: &'a LiveChatState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::{
        FontRole, font,
        tokens::{FONT_XS, Radius, radius},
    };
    use iced::widget::{button, container, scrollable, text};
    use iced::{Background, Border, Length, Padding};

    let visible: Vec<Element<'a, Message>> = filter_log(&state.chat_log, &state.chat_filter)
        .map(|row| forge_widgets::chat_row(palette, row, Some(select_viewer_msg)))
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

    let p = *palette;
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

        let scrollable_chat = scrollable(col)
            .id(chat_scroll_id())
            .on_scroll(|vp| Message::LiveChat(LiveChatMsg::Scrolled(vp)))
            .height(Length::Fill);

        if state.unread_count > 0 {
            let label = if state.unread_count == 1 {
                "1 new message".to_owned()
            } else {
                format!("{} new messages", state.unread_count)
            };

            let bubble = button(
                text(label)
                    .size(FONT_XS)
                    .color(p.text_primary)
                    .font(font(FontRole::Body)),
            )
            .on_press(Message::LiveChat(LiveChatMsg::ScrollToBottom))
            .padding([6, 12])
            .style(move |_theme: &iced::Theme, status| {
                let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: Some(Background::Color(if hovered { p.elevated } else { p.base })),
                    border: Border {
                        color: p.brand,
                        width: 0.5,
                        radius: radius(Radius::Pill).into(),
                    },
                    text_color: p.text_primary,
                    shadow: iced::Shadow::default(),
                    snap: false,
                }
            });

            let floating_overlay = container(bubble)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::Alignment::Center)
                .align_y(iced::alignment::Vertical::Bottom)
                .padding(Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: 16.0,
                    left: 0.0,
                });

            iced::widget::stack![scrollable_chat, floating_overlay].into()
        } else {
            scrollable_chat.into()
        }
    };

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
    use forge_storage::{Viewer, ViewerPlatform};

    fn make_viewer(username: &str, msg_count: u64) -> Viewer {
        Viewer {
            viewer_id: username.to_owned(),
            platform: ViewerPlatform::Twitch,
            username: username.to_owned(),
            first_seen_at: OffsetDateTime::now_utc(),
            last_seen_at: OffsetDateTime::now_utc(),
            message_count: msg_count,
            custom_greeting: false,
        }
    }

    fn make_viewers_state(usernames: &[&str]) -> ViewersState {
        use crate::viewers::ViewersState;
        ViewersState {
            viewers: usernames.iter().map(|u| make_viewer(u, 0)).collect(),
        }
    }

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
    fn live_chat_state_new_has_empty_drawer_state() {
        let state = LiveChatState::new();
        assert!(state.drawer_search.is_empty());
        assert!(state.selected_viewer.is_none());
        assert!(!state.drawer_open);
        assert!(!state.drawer_menu_open);
    }

    #[test]
    fn summary_role_is_latest_badge_from_chat() {
        let mut log = VecDeque::new();
        log.push_back(ChatRow {
            timestamp: "00:00:00".into(),
            platform: Platform::Twitch,
            badges: vec![BadgeKind::Moderator],
            username: "danylo_ua".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("first".into()),
        });
        log.push_back(ChatRow {
            timestamp: "00:00:01".into(),
            platform: Platform::Twitch,
            badges: vec![BadgeKind::Vip],
            username: "danylo_ua".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("second".into()),
        });
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let summary = synthesize_from_chat("danylo_ua", &log, &palette).unwrap();
        assert_eq!(summary.role, Some(BadgeKind::Vip));
    }

    #[test]
    fn summary_none_when_username_absent_from_chat() {
        let log = VecDeque::new();
        let (_, palette) = forge_widgets::catppuccin_mocha();
        assert!(synthesize_from_chat("ghost_user", &log, &palette).is_none());
    }

    #[test]
    fn viewer_hash_color_is_deterministic() {
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let c1 = viewer_hash_color("danylo_ua", &palette);
        let c2 = viewer_hash_color("danylo_ua", &palette);
        assert_eq!(c1, c2);
    }

    #[test]
    fn effective_summary_returns_selected_when_in_chat() {
        let mut state = LiveChatState::new();
        state.selected_viewer = Some("haash_".to_owned());
        let vs = make_viewers_state(&[]);
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let result = effective_summary(&state, &vs, &palette);
        assert!(result.is_some());
        assert_eq!(result.unwrap().username, "haash_");
    }

    #[test]
    fn effective_summary_falls_back_to_last_chat_entry_when_selected_missing() {
        let mut state = LiveChatState::new();
        state.selected_viewer = Some("ghost_not_in_chat".to_owned());
        let vs = make_viewers_state(&["alice", "bob"]);
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let result = effective_summary(&state, &vs, &palette);
        assert!(result.is_some());
        assert_eq!(result.unwrap().username, "factorio_streamer");
    }

    #[test]
    fn synthesize_from_chat_counts_occurrences_and_uses_latest_badge() {
        let mut log = VecDeque::new();
        log.push_back(ChatRow {
            timestamp: "00:00:00".into(),
            platform: Platform::Twitch,
            badges: vec![BadgeKind::Moderator],
            username: "alice".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("first".into()),
        });
        log.push_back(ChatRow {
            timestamp: "00:00:01".into(),
            platform: Platform::Twitch,
            badges: vec![BadgeKind::Vip],
            username: "alice".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("second".into()),
        });
        log.push_back(ChatRow {
            timestamp: "00:00:02".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "bob".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("hi".into()),
        });
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let summary = synthesize_from_chat("alice", &log, &palette).unwrap();
        assert_eq!(summary.message_count, 2);
        assert_eq!(summary.role, Some(BadgeKind::Vip));
        assert_eq!(summary.username, "alice");
    }

    #[test]
    fn synthesize_from_chat_returns_none_when_username_absent() {
        let log = VecDeque::new();
        let (_, palette) = forge_widgets::catppuccin_mocha();
        assert!(synthesize_from_chat("nobody", &log, &palette).is_none());
    }

    #[test]
    fn enrich_with_storage_overwrites_message_count_when_viewer_present() {
        let mut log = VecDeque::new();
        log.push_back(ChatRow {
            timestamp: "00:00:00".into(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "alice".into(),
            username_color: Color::WHITE,
            body: ChatBody::Message("hi".into()),
        });
        let (_, palette) = forge_widgets::catppuccin_mocha();
        let summary = synthesize_from_chat("alice", &log, &palette).unwrap();
        assert_eq!(summary.message_count, 1);
        let vs = make_viewers_state(&["alice"]);
        let enriched = enrich_with_storage(summary, &vs);
        assert_eq!(enriched.message_count, 0);
    }

    #[test]
    fn drawer_search_filter_case_insensitive() {
        let usernames = ["Alice", "Bob", "alicetv"];
        let matches: Vec<&&str> = usernames
            .iter()
            .filter(|u| drawer_matches(u, "alice"))
            .collect();
        assert_eq!(matches.len(), 2);
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
