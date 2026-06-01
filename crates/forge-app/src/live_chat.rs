use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

use forge_events::{Event, EventSource};
use forge_types::{
    ChatEventDetail, ChatSegment, ChatSource, EventId, ModerationMarks, PlatformId, UnifiedChatRow,
    UserBadge,
};
use iced::Task;
use time::OffsetDateTime;

use crate::Message;
use crate::live_chat_view::platform_id_to_key;
use crate::message::LiveChatMsg;
use crate::runtime_view::RuntimeView;

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

pub fn chat_scroll_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("forge:chat_scroll")
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use forge_runtime::{EventBus, NullEventLogRepo};
    use forge_storage::{CredentialsRepo, DataProvider};
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{
        ChatSegment, ChatSource, EventId, ModerationMarks, PlatformId, UnifiedChatRow,
    };
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
            body_segments: vec![ChatSegment::Text {
                text: "test msg".to_owned(),
            }],
            badges: vec![],
            is_event,
            event_detail: None,
            moderation: ModerationMarks::default(),
        }
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
            youtube_flow: None,
            trovo_flow: None,
            kick_flow: None,
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
