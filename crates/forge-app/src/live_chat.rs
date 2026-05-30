use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

use forge_events::{Event, EventSource};
use forge_types::{
    ChatEventDetail, ChatSegment, ChatSource, EventId, ModerationMarks, PlatformId, UnifiedChatRow,
    UserBadge,
};
use forge_widgets::{
    BadgeKind, ChatBody, ChatRow, ForgePalette, Icon, Platform, PlatformTarget, tabler_icon,
    tokens::{Radius, Spacing, radius, sp, spf},
};
use iced::{Color, Element, Length, Task};
use time::OffsetDateTime;

use crate::Message;
use crate::message::LiveChatMsg;
use crate::runtime_view::RuntimeView;
use crate::viewers::ViewersState;

pub const CHAT_LOG_MAX: usize = 2_000;

pub type SendId = u64;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PlatformFilter {
    #[default]
    All,
    Single(PlatformId),
    Custom(BTreeSet<PlatformId>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EventsFilter {
    #[default]
    All,
    OnlyMessages,
    OnlyEvents,
}

#[derive(Debug, Clone)]
pub struct PendingSendState {
    pub target: PlatformId,
    pub started_at: OffsetDateTime,
    pub status: PendingSendStatus,
}

#[derive(Debug, Clone)]
pub enum PendingSendStatus {
    InFlight,
    Ok,
    Failed(String),
}

pub struct LiveChatState {
    pub rows: VecDeque<UnifiedChatRow>,
    pub platform_filter: PlatformFilter,
    pub events_filter: EventsFilter,
    pub hide_bots: bool,
    pub search_query: String,
    pub auto_scroll: bool,
    pub scroll_position: f32,
    pub input_buffer: String,
    pub cross_post: bool,
    pub primary_send_target: Option<PlatformId>,
    pub secondary_send_targets: Vec<PlatformId>,
    pub pending_sends: HashMap<SendId, PendingSendState>,
    pub connected_platforms: Vec<PlatformId>,
    pub next_send_id: SendId,
    pub drawer_open: bool,
    pub drawer_width: Option<f32>,
    pub drawer_menu_open: bool,
    pub drawer_search: String,
    pub selected_viewer: Option<String>,
    pub unread_count: u32,
    pub emoji_picker_open: bool,
    pub next_chat_seq: u64,
}

impl LiveChatState {
    pub fn new() -> Self {
        let mut rows: VecDeque<UnifiedChatRow> = VecDeque::new();
        let base = OffsetDateTime::now_utc();

        rows.push_back(UnifiedChatRow {
            id: "seed-0".to_owned(),
            event_id: EventId::new(),
            source: ChatSource::Twitch,
            received_at: base,
            author: "haash_".to_owned(),
            author_color: Some([0xcb, 0xa6, 0xf7]),
            body_segments: vec![ChatSegment::Text {
                text: "welcome to the stream everyone, GTNH grind continues".to_owned(),
            }],
            badges: vec![UserBadge::Moderator],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        });
        rows.push_back(UnifiedChatRow {
            id: "seed-1".to_owned(),
            event_id: EventId::new(),
            source: ChatSource::Twitch,
            received_at: base,
            author: "danylo_ua".to_owned(),
            author_color: Some([0xfa, 0xb3, 0x87]),
            body_segments: vec![],
            badges: vec![],
            is_event: true,
            event_detail: Some(ChatEventDetail::Subscription {
                tier: 1,
                months: Some(3),
                message: Some("Дякую за стрім, GTNH топ!".into()),
            }),
            moderation: ModerationMarks::default(),
        });
        rows.push_back(UnifiedChatRow {
            id: "seed-2".to_owned(),
            event_id: EventId::new(),
            source: ChatSource::YouTube,
            received_at: base,
            author: "olena_lv".to_owned(),
            author_color: Some([0xf5, 0xc2, 0xe7]),
            body_segments: vec![ChatSegment::Text {
                text: "aluminum bottleneck знов :(".to_owned(),
            }],
            badges: vec![],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        });
        rows.push_back(UnifiedChatRow {
            id: "seed-3".to_owned(),
            event_id: EventId::new(),
            source: ChatSource::Twitch,
            received_at: base,
            author: "koval_dev".to_owned(),
            author_color: Some([0xa6, 0xe3, 0xa1]),
            body_segments: vec![ChatSegment::Text {
                text: "!quote".to_owned(),
            }],
            badges: vec![],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        });
        rows.push_back(UnifiedChatRow {
            id: "seed-4".to_owned(),
            event_id: EventId::new(),
            source: ChatSource::Twitch,
            received_at: base,
            author: "stream_fan_kyiv".to_owned(),
            author_color: Some([0xfa, 0xb3, 0x87]),
            body_segments: vec![ChatSegment::Text {
                text: "keep going! love the UA stream".to_owned(),
            }],
            badges: vec![],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        });
        rows.push_back(UnifiedChatRow {
            id: "seed-5".to_owned(),
            event_id: EventId::new(),
            source: ChatSource::Kick,
            received_at: base,
            author: "ostap_pl".to_owned(),
            author_color: Some([0x94, 0xe2, 0xd5]),
            body_segments: vec![ChatSegment::Text {
                text: "ти вже відкрив stainless steel?".to_owned(),
            }],
            badges: vec![],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        });
        rows.push_back(UnifiedChatRow {
            id: "seed-6".to_owned(),
            event_id: EventId::new(),
            source: ChatSource::Twitch,
            received_at: base,
            author: "factorio_streamer".to_owned(),
            author_color: Some([0xf3, 0x8b, 0xa8]),
            body_segments: vec![],
            badges: vec![],
            is_event: true,
            event_detail: Some(ChatEventDetail::Raid { viewer_count: 42 }),
            moderation: ModerationMarks::default(),
        });

        Self {
            rows,
            platform_filter: PlatformFilter::All,
            events_filter: EventsFilter::All,
            hide_bots: false,
            search_query: String::new(),
            auto_scroll: true,
            scroll_position: 0.0,
            input_buffer: String::new(),
            cross_post: false,
            primary_send_target: Some(PlatformId::Twitch),
            secondary_send_targets: Vec::new(),
            pending_sends: HashMap::new(),
            connected_platforms: vec![PlatformId::Twitch],
            next_send_id: 0,
            drawer_open: false,
            drawer_width: None,
            drawer_menu_open: false,
            drawer_search: String::new(),
            selected_viewer: None,
            unread_count: 0,
            emoji_picker_open: false,
            next_chat_seq: 7,
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
        LiveChatMsg::RowReceived(row) => {
            state.rows.push_back(row);
            if state.rows.len() > CHAT_LOG_MAX {
                state.rows.pop_front();
            }
            if state.auto_scroll && state.search_query.is_empty() {
                iced::widget::operation::snap_to_end(chat_scroll_id())
            } else {
                state.unread_count = state.unread_count.saturating_add(1);
                Task::none()
            }
        }
        LiveChatMsg::PlatformFilterChanged(f) => {
            state.platform_filter = f;
            Task::none()
        }
        LiveChatMsg::EventsFilterToggled(f) => {
            state.events_filter = f;
            Task::none()
        }
        LiveChatMsg::HideBotsToggled => {
            state.hide_bots = !state.hide_bots;
            Task::none()
        }
        LiveChatMsg::SearchChanged(q) => {
            let was_empty = state.search_query.is_empty();
            state.search_query = q;
            if was_empty && !state.search_query.is_empty() {
                state.auto_scroll = false;
            }
            Task::none()
        }
        LiveChatMsg::AutoScrollToggled => {
            state.auto_scroll = !state.auto_scroll;
            Task::none()
        }
        LiveChatMsg::InputChanged(s) => {
            state.input_buffer = s;
            Task::none()
        }
        LiveChatMsg::CrossPostToggled => {
            state.cross_post = !state.cross_post;
            Task::none()
        }
        LiveChatMsg::PrimarySendTargetChanged(p) => {
            state.primary_send_target = Some(p);
            let dp: Arc<dyn forge_storage::SettingsRepo> =
                Arc::clone(&rt.backend) as Arc<dyn forge_storage::SettingsRepo>;
            let key_str = platform_id_to_key(p).to_owned();
            Task::perform(
                async move {
                    dp.set_string("live_chat:primary_send_target", &key_str)
                        .await
                        .ok();
                },
                |_| Message::Noop,
            )
        }
        LiveChatMsg::SecondarySendTargetToggled(p) => {
            if let Some(pos) = state.secondary_send_targets.iter().position(|&t| t == p) {
                state.secondary_send_targets.remove(pos);
            } else {
                state.secondary_send_targets.push(p);
            }
            Task::none()
        }
        LiveChatMsg::SendPressed => {
            let body = std::mem::take(&mut state.input_buffer);
            let body = body.trim().to_owned();
            if body.is_empty() {
                return Task::none();
            }
            let mut targets: Vec<PlatformId> = Vec::new();
            if let Some(primary) = state.primary_send_target {
                targets.push(primary);
            }
            if state.cross_post {
                for &t in &state.secondary_send_targets {
                    if !targets.contains(&t) {
                        targets.push(t);
                    }
                }
            }
            if targets.is_empty()
                && let Some(&first) = state.connected_platforms.first()
            {
                targets.push(first);
            }
            let mut tasks: Vec<Task<Message>> = Vec::new();
            for target in targets {
                let send_id = state.next_send_id;
                state.next_send_id = state.next_send_id.wrapping_add(1);
                state.pending_sends.insert(
                    send_id,
                    PendingSendState {
                        target,
                        started_at: OffsetDateTime::now_utc(),
                        status: PendingSendStatus::InFlight,
                    },
                );
                let body_clone = body.clone();
                let bus = Arc::clone(&rt.bus);
                tasks.push(Task::perform(
                    async move {
                        match target {
                            PlatformId::Twitch => {
                                bus.publish(Event::new(
                                    EventSource::Core,
                                    "chat.send.request",
                                    serde_json::json!({
                                        "target": "twitch",
                                        "message": body_clone,
                                    }),
                                ));
                                Ok::<(), String>(())
                            }
                            _ => Err("not yet wired".to_owned()),
                        }
                    },
                    move |r| Message::LiveChat(LiveChatMsg::SendCompleted(send_id, r)),
                ));
            }
            Task::batch(tasks)
        }
        LiveChatMsg::SendCompleted(id, result) => {
            if let Some(ps) = state.pending_sends.get_mut(&id) {
                ps.status = match result {
                    Ok(()) => PendingSendStatus::Ok,
                    Err(e) => PendingSendStatus::Failed(e),
                };
            }
            Task::none()
        }
        LiveChatMsg::ConnectedPlatformsUpdated(platforms) => {
            state.connected_platforms = platforms;
            let current_valid = state
                .primary_send_target
                .map(|p| state.connected_platforms.contains(&p))
                .unwrap_or(false);
            if !current_valid {
                state.primary_send_target = state.connected_platforms.first().copied();
            }
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
        LiveChatMsg::LoadDrawerWidth => {
            let dp: Arc<dyn forge_storage::SettingsRepo> =
                Arc::clone(&rt.backend) as Arc<dyn forge_storage::SettingsRepo>;
            Task::perform(
                async move { crate::ui_settings::sheet_width(&*dp, "viewers_drawer").await },
                |r| Message::LiveChat(LiveChatMsg::DrawerWidthLoaded(r.ok().flatten())),
            )
        }
        LiveChatMsg::DrawerWidthLoaded(width) => {
            state.drawer_width = width;
            Task::none()
        }
        LiveChatMsg::SheetResized(w) => {
            state.drawer_width = Some(w);
            let dp: Arc<dyn forge_storage::SettingsRepo> =
                Arc::clone(&rt.backend) as Arc<dyn forge_storage::SettingsRepo>;
            Task::perform(
                async move { crate::ui_settings::set_sheet_width(&*dp, "viewers_drawer", w).await },
                |_| Message::Noop,
            )
        }
    }
}

pub fn chat_scroll_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("forge:chat_scroll")
}

pub fn row_match_opacity(row: &UnifiedChatRow, query: &str) -> f32 {
    if query.is_empty() {
        return 1.0;
    }
    let lower = query.to_ascii_lowercase();
    let body = row.body_text().to_ascii_lowercase();
    let author = row.author.to_ascii_lowercase();
    if body.contains(&lower) || author.contains(&lower) {
        1.0
    } else {
        0.3
    }
}

fn row_matches_platform_filter(row: &UnifiedChatRow, filter: &PlatformFilter) -> bool {
    match filter {
        PlatformFilter::All => true,
        PlatformFilter::Single(id) => chat_source_to_platform_id(row.source) == *id,
        PlatformFilter::Custom(ids) => ids.contains(&chat_source_to_platform_id(row.source)),
    }
}

fn row_matches_events_filter(row: &UnifiedChatRow, filter: EventsFilter) -> bool {
    match filter {
        EventsFilter::All => true,
        EventsFilter::OnlyMessages => !row.is_event,
        EventsFilter::OnlyEvents => row.is_event,
    }
}

fn row_is_bot(row: &UnifiedChatRow) -> bool {
    row.badges.iter().any(|b| matches!(b, UserBadge::Bot))
}

pub fn chat_source_to_platform_id(source: ChatSource) -> PlatformId {
    match source {
        ChatSource::Twitch => PlatformId::Twitch,
        ChatSource::YouTube => PlatformId::YouTube,
        ChatSource::Kick => PlatformId::Kick,
        ChatSource::Trovo => PlatformId::Trovo,
    }
}

fn platform_id_to_widget(id: PlatformId) -> Platform {
    match id {
        PlatformId::Twitch => Platform::Twitch,
        PlatformId::YouTube => Platform::YouTube,
        PlatformId::Kick => Platform::Kick,
        PlatformId::Trovo => Platform::Trovo,
    }
}

fn platform_id_to_key(id: PlatformId) -> &'static str {
    match id {
        PlatformId::Twitch => "twitch",
        PlatformId::YouTube => "youtube",
        PlatformId::Kick => "kick",
        PlatformId::Trovo => "trovo",
    }
}

fn unified_badge_to_kind(badge: &UserBadge) -> BadgeKind {
    match badge {
        UserBadge::Broadcaster => BadgeKind::Broadcaster,
        UserBadge::Moderator => BadgeKind::Moderator,
        UserBadge::Vip => BadgeKind::Vip,
        UserBadge::Subscriber { .. } => BadgeKind::Subscriber,
        UserBadge::Member { .. } => BadgeKind::Subscriber,
        UserBadge::Bot => BadgeKind::Bot,
    }
}

fn author_color_to_iced(color: Option<[u8; 3]>, fallback: Color) -> Color {
    match color {
        Some([r, g, b]) => Color::from_rgb8(r, g, b),
        None => fallback,
    }
}

fn format_row_timestamp(dt: OffsetDateTime) -> String {
    let secs = dt.unix_timestamp();
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn unified_to_chat_row(row: &UnifiedChatRow, seq: u64) -> ChatRow {
    let platform = match row.source {
        ChatSource::Twitch => Platform::Twitch,
        ChatSource::YouTube => Platform::YouTube,
        ChatSource::Kick => Platform::Kick,
        ChatSource::Trovo => Platform::Trovo,
    };
    let badges: Vec<BadgeKind> = row.badges.iter().map(unified_badge_to_kind).collect();
    let username_color = author_color_to_iced(row.author_color, Color::from_rgb(0.4, 0.7, 1.0));
    let timestamp = format_row_timestamp(row.received_at);
    let body = match &row.event_detail {
        Some(ChatEventDetail::Subscription {
            tier,
            months,
            message,
        }) => ChatBody::Subscription {
            tier: *tier,
            months: *months,
            message: message.clone(),
            triggered_action: None,
        },
        Some(ChatEventDetail::Raid { viewer_count }) => ChatBody::Raid {
            viewers: *viewer_count,
            triggered_action: None,
        },
        Some(ChatEventDetail::SuperChat {
            amount_micros,
            message,
            ..
        }) => ChatBody::Cheer {
            bits: amount_micros / 10_000,
            text: message.clone().unwrap_or_default(),
        },
        Some(ChatEventDetail::NewMember { .. }) | Some(ChatEventDetail::MemberMilestone { .. }) => {
            ChatBody::Subscription {
                tier: 1,
                months: None,
                message: None,
                triggered_action: None,
            }
        }
        None => ChatBody::Message(row.body_text()),
    };
    ChatRow {
        seq,
        timestamp,
        platform,
        badges,
        username: row.author.clone(),
        username_color,
        body,
    }
}

fn select_viewer_msg(name: String) -> Message {
    Message::LiveChat(LiveChatMsg::DrawerSelectViewer(name))
}

pub fn live_chat_view<'a>(
    state: &'a LiveChatState,
    viewers: &'a ViewersState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let page_header = live_chat_page_header(state, palette);
    let chat_area = build_chat_area(state, palette);

    let targets: Vec<PlatformTarget<'a, Message>> = state
        .connected_platforms
        .iter()
        .map(|&p| {
            let is_primary = state.primary_send_target == Some(p);
            PlatformTarget {
                platform: platform_id_to_widget(p),
                active: is_primary,
                on_press: Some(Box::new(move || {
                    Message::LiveChat(LiveChatMsg::PrimarySendTargetChanged(p))
                })),
            }
        })
        .collect();

    let send_placeholder = if state.connected_platforms.is_empty() {
        "Connect a platform to send..."
    } else {
        "Send to chat..."
    };

    let bar = forge_widgets::input_bar(
        palette,
        &state.input_buffer,
        send_placeholder,
        targets,
        |s| Message::LiveChat(LiveChatMsg::InputChanged(s)),
        Message::LiveChat(LiveChatMsg::SendPressed),
        state.emoji_picker_open,
        Message::LiveChat(LiveChatMsg::ToggleEmoji),
    );

    let chat_column = iced::widget::column![chat_area, bar]
        .width(Length::Fill)
        .height(Length::Fill);

    let panel_content = crate::live_chat_drawer::drawer_panel(state, viewers, palette);
    let sheet = forge_widgets::SideSheet::new(panel_content)
        .open(state.drawer_open)
        .palette(palette)
        .width(forge_widgets::SheetWidth::new(
            state.drawer_width.unwrap_or(360.0).clamp(280.0, 560.0),
            280.0,
            560.0,
        ))
        .resizable(true)
        .sheet_key("viewers_drawer")
        .header(forge_widgets::SheetHeader {
            title: std::borrow::Cow::Borrowed("Viewers"),
            subtitle: None,
            on_close: Some(Message::LiveChat(LiveChatMsg::ToggleDrawer)),
        })
        .on_close(Message::LiveChat(LiveChatMsg::ToggleDrawer))
        .on_resize(|w| Message::LiveChat(LiveChatMsg::SheetResized(w)));

    let body: Element<'a, Message> = iced::widget::stack![chat_column, sheet].into();

    iced::widget::column![page_header, body]
        .height(Length::Fill)
        .into()
}

fn platform_filter_chip_color(id: PlatformId, palette: &ForgePalette) -> Color {
    match id {
        PlatformId::Twitch => palette.brand,
        PlatformId::YouTube => palette.random,
        PlatformId::Kick => palette.info,
        PlatformId::Trovo => palette.success,
    }
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
    .spacing(spf(Spacing::Xs))
    .align_y(iced::alignment::Vertical::Center);

    let chip_all = forge_widgets::filter_chip(
        palette,
        "All",
        p.brand,
        state.platform_filter == PlatformFilter::All,
        Message::LiveChat(LiveChatMsg::PlatformFilterChanged(PlatformFilter::All)),
    );

    let mut filter_chips: Vec<Element<'a, Message>> = vec![chip_all];
    for &pid in &state.connected_platforms {
        let color = platform_filter_chip_color(pid, palette);
        let label = match pid {
            PlatformId::Twitch => "Twitch",
            PlatformId::YouTube => "YouTube",
            PlatformId::Kick => "Kick",
            PlatformId::Trovo => "Trovo",
        };
        let is_active = state.platform_filter == PlatformFilter::Single(pid);
        filter_chips.push(forge_widgets::filter_chip(
            palette,
            label,
            color,
            is_active,
            Message::LiveChat(LiveChatMsg::PlatformFilterChanged(PlatformFilter::Single(
                pid,
            ))),
        ));
    }

    let events_chip = forge_widgets::filter_chip(
        palette,
        "Events",
        p.warning,
        state.events_filter == EventsFilter::OnlyEvents,
        Message::LiveChat(LiveChatMsg::EventsFilterToggled(
            if state.events_filter == EventsFilter::OnlyEvents {
                EventsFilter::All
            } else {
                EventsFilter::OnlyEvents
            },
        )),
    );
    filter_chips.push(events_chip);

    let chips = iced::widget::row(filter_chips).spacing(spf(Spacing::Xxs));

    let divider = container(iced::widget::Space::new().width(0.5).height(16.0))
        .width(0.5)
        .height(16.0)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.border_regular)),
            ..container::Style::default()
        });

    let match_count = if state.search_query.is_empty() {
        state.rows.len()
    } else {
        let q = state.search_query.as_str();
        state
            .rows
            .iter()
            .filter(|r| row_match_opacity(r, q) >= 1.0)
            .count()
    };

    let viewer_count_str = if state.search_query.is_empty() {
        format!("{} messages", state.rows.len())
    } else {
        format!("{} matches", match_count)
    };
    let viewer_info = row![
        text(viewer_count_str)
            .size(FONT_XS)
            .color(p.text_secondary)
            .font(mono),
    ]
    .spacing(spf(Spacing::Xs))
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
        .spacing(spf(Spacing::Xxs))
        .align_y(iced::alignment::Vertical::Center),
    )
    .on_press(Message::LiveChat(LiveChatMsg::ToggleDrawer))
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
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
    });

    let right_side = row![chips, divider, viewer_info, drawer_btn]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::alignment::Vertical::Center);

    let inner = row![
        crumbs_left,
        iced::widget::Space::new().width(Length::Fill),
        right_side,
    ]
    .align_y(iced::alignment::Vertical::Center);

    container(inner)
        .width(Length::Fill)
        .padding([sp(Spacing::Xs), sp(Spacing::Md)])
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

fn build_chat_area<'a>(
    state: &'a LiveChatState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::{
        FontRole, font,
        tokens::{FONT_XS, Radius, radius},
    };
    use iced::widget::{button, container, scrollable, text};
    use iced::{Background, Border, Padding};

    let palette_copy = *palette;
    let query = state.search_query.clone();

    let visible: Vec<Element<'a, Message>> = state
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            row_matches_platform_filter(row, &state.platform_filter)
                && row_matches_events_filter(row, state.events_filter)
                && !(state.hide_bots && row_is_bot(row))
        })
        .map(|(idx, row)| {
            let opacity = row_match_opacity(row, &query);
            let chat_row = unified_to_chat_row(row, state.next_chat_seq.wrapping_add(idx as u64));
            let seq = chat_row.seq;
            let row_el: Element<'a, Message> = iced::widget::lazy(seq, move |_: &u64| {
                forge_widgets::ChatRowWidget::new(
                    palette_copy,
                    chat_row.clone(),
                    Some(select_viewer_msg),
                )
            })
            .into();
            if opacity < 1.0 {
                let p = palette_copy;
                container(row_el)
                    .style(move |_: &iced::Theme| container::Style {
                        background: Some(Background::Color(iced::Color { a: 0.7, ..p.base })),
                        ..container::Style::default()
                    })
                    .into()
            } else {
                row_el
            }
        })
        .collect();

    let p = *palette;

    let empty_msg = if !state.search_query.is_empty() {
        "No messages match your search."
    } else {
        match state.events_filter {
            EventsFilter::OnlyEvents => "No events yet.",
            _ => "Not connected — go to Settings → Platforms to connect.",
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
            .spacing(spf(Spacing::Xxs))
            .width(Length::Fill)
            .padding([sp(Spacing::Xs), sp(Spacing::Md)]);

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
            .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use forge_runtime::{EventBus, NullEventLogRepo};
    use forge_storage::{CredentialsRepo, DataProvider};
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{ChatSource, EventId, ModerationMarks, PlatformId, UnifiedChatRow};
    use time::OffsetDateTime;

    use super::*;
    use crate::message::LiveChatMsg;
    use crate::runtime_view::RuntimeView;
    use crate::server_subsystem::ServerSubsystem;

    const TEST_KEY: [u8; 32] = [0xcd; 32];

    fn make_row(id: &str, source: ChatSource, author: &str, is_event: bool) -> UnifiedChatRow {
        UnifiedChatRow {
            id: id.to_owned(),
            event_id: EventId::new(),
            source,
            received_at: OffsetDateTime::now_utc(),
            author: author.to_owned(),
            author_color: None,
            body_segments: vec![forge_types::ChatSegment::Text {
                text: "test msg".to_owned(),
            }],
            badges: vec![],
            is_event,
            event_detail: None,
            moderation: ModerationMarks::default(),
        }
    }

    fn make_event_row(id: &str) -> UnifiedChatRow {
        let mut row = make_row(id, ChatSource::Twitch, "raider", true);
        row.event_detail = Some(ChatEventDetail::Raid { viewer_count: 10 });
        row
    }

    fn make_bot_row_for_filter(id: &str) -> UnifiedChatRow {
        let mut row = make_row(id, ChatSource::Twitch, "nightbot", false);
        row.badges = vec![UserBadge::Bot];
        row
    }

    fn test_rt() -> RuntimeView {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let backend = Arc::new(
            tokio_rt
                .block_on(SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY))
                .unwrap(),
        );
        let server_subsystem = Arc::new(ServerSubsystem::new(
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>
        ));
        let backend: Arc<dyn DataProvider> = backend;
        RuntimeView {
            actions: Arc::new(forge_runtime::actions::ActionsService::new(
                backend.action_repo(),
                backend.queue_repo(),
                backend.history_repo(),
                backend.trigger_instance_repo(),
                backend.soundboard_clips_repo(),
            )),
            backend,
            bus: EventBus::new(Arc::new(NullEventLogRepo)),
            script_registry: Arc::new(forge_runtime::ScriptRegistry::new()),
            server_subsystem,
            action_engine: None,
            scheduler: None,
            obs_client: None,
            speak_queue: None,
            sound_player: None,
            twitch_chat_handle: None,
            chat_send_bridge: None,
            twitch_flow: None,
            twitch_login: None,
            twitch_token_expires: None,
            twitch_reauth_required: false,
            sub_action_registry: Arc::new(forge_registry::SubActionRegistry::new()),
            trigger_registry: Arc::new(forge_registry::TriggerRegistry::new()),
        }
    }

    #[test]
    fn row_received_appends_to_rows() {
        let rt = test_rt();
        let mut state = LiveChatState::new();
        let initial_len = state.rows.len();
        let row = make_row("new-1", ChatSource::Twitch, "viewer1", false);
        let _ = update(&mut state, &rt, LiveChatMsg::RowReceived(row));
        assert_eq!(state.rows.len(), initial_len + 1);
        assert_eq!(state.rows.back().unwrap().id, "new-1");
    }

    #[test]
    fn row_received_evicts_oldest_when_at_cap() {
        let rt = test_rt();
        let mut state = LiveChatState::new();
        state.rows.clear();
        for i in 0..CHAT_LOG_MAX {
            state.rows.push_back(make_row(
                &format!("cap-{i}"),
                ChatSource::Twitch,
                "v",
                false,
            ));
        }
        assert_eq!(state.rows.front().unwrap().id, "cap-0");
        let new_row = make_row("overflow", ChatSource::Twitch, "v", false);
        let _ = update(&mut state, &rt, LiveChatMsg::RowReceived(new_row));
        assert_eq!(state.rows.len(), CHAT_LOG_MAX);
        assert_eq!(state.rows.front().unwrap().id, "cap-1");
        assert_eq!(state.rows.back().unwrap().id, "overflow");
    }

    #[test]
    fn platform_filter_single_keeps_only_matching() {
        let mut rows: VecDeque<UnifiedChatRow> = VecDeque::new();
        rows.push_back(make_row("t1", ChatSource::Twitch, "a", false));
        rows.push_back(make_row("y1", ChatSource::YouTube, "b", false));
        rows.push_back(make_row("k1", ChatSource::Kick, "c", false));

        let filter = PlatformFilter::Single(PlatformId::Twitch);
        let matching: Vec<_> = rows
            .iter()
            .filter(|r| row_matches_platform_filter(r, &filter))
            .collect();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].id, "t1");
    }

    #[test]
    fn events_filter_only_messages_skips_event_rows() {
        let rows = [
            make_row("msg-1", ChatSource::Twitch, "a", false),
            make_event_row("ev-1"),
            make_row("msg-2", ChatSource::YouTube, "b", false),
        ];
        let messages_only: Vec<_> = rows
            .iter()
            .filter(|r| row_matches_events_filter(r, EventsFilter::OnlyMessages))
            .collect();
        assert_eq!(messages_only.len(), 2);
        assert!(messages_only.iter().all(|r| !r.is_event));
    }

    #[test]
    fn search_query_dims_non_matching() {
        let mut row = make_row("s1", ChatSource::Twitch, "alice", false);
        row.body_segments = vec![forge_types::ChatSegment::Text {
            text: "hello world".to_owned(),
        }];
        assert_eq!(row_match_opacity(&row, "hello"), 1.0);
        assert_eq!(row_match_opacity(&row, "alice"), 1.0);
        assert_eq!(row_match_opacity(&row, "xyz"), 0.3);
        assert_eq!(row_match_opacity(&row, ""), 1.0);
    }

    #[test]
    fn send_pressed_dispatches_one_task_per_target() {
        let rt = test_rt();
        let mut state = LiveChatState::new();
        state.input_buffer = "hello chat".to_owned();
        state.primary_send_target = Some(PlatformId::Twitch);
        state.cross_post = false;
        let _ = update(&mut state, &rt, LiveChatMsg::SendPressed);
        assert!(state.input_buffer.is_empty());
        assert_eq!(state.pending_sends.len(), 1);
        assert!(
            state
                .pending_sends
                .values()
                .all(|ps| ps.target == PlatformId::Twitch)
        );
    }

    #[test]
    fn send_completed_updates_pending_state() {
        let rt = test_rt();
        let mut state = LiveChatState::new();
        state.pending_sends.insert(
            42,
            PendingSendState {
                target: PlatformId::Twitch,
                started_at: OffsetDateTime::now_utc(),
                status: PendingSendStatus::InFlight,
            },
        );
        let _ = update(&mut state, &rt, LiveChatMsg::SendCompleted(42, Ok(())));
        assert!(matches!(
            state.pending_sends[&42].status,
            PendingSendStatus::Ok
        ));

        let _ = update(
            &mut state,
            &rt,
            LiveChatMsg::SendCompleted(42, Err("oops".into())),
        );
        assert!(matches!(
            state.pending_sends[&42].status,
            PendingSendStatus::Failed(_)
        ));
    }

    #[test]
    fn primary_send_target_change_persists_to_settings() {
        let rt = test_rt();
        let mut state = LiveChatState::new();
        state.primary_send_target = None;
        let _ = update(
            &mut state,
            &rt,
            LiveChatMsg::PrimarySendTargetChanged(PlatformId::YouTube),
        );
        assert_eq!(state.primary_send_target, Some(PlatformId::YouTube));
    }

    #[test]
    fn hide_bots_filter_removes_bot_rows() {
        let rows = [
            make_bot_row_for_filter("b1"),
            make_row("m1", ChatSource::Twitch, "viewer", false),
        ];
        let visible: Vec<_> = rows.iter().filter(|r| !row_is_bot(r)).collect();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, "m1");
    }

    #[test]
    fn platform_filter_default_is_all() {
        assert_eq!(PlatformFilter::default(), PlatformFilter::All);
    }

    #[test]
    fn live_chat_state_new_has_seed_rows() {
        let state = LiveChatState::new();
        assert!(!state.rows.is_empty());
        assert!(!state.drawer_open);
        assert!(state.auto_scroll);
    }

    #[test]
    fn drawer_width_field_round_trip() {
        let mut state = LiveChatState::new();
        assert!(state.drawer_width.is_none());
        state.drawer_width = Some(420.0);
        assert_eq!(state.drawer_width, Some(420.0));
    }
}
