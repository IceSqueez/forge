use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    BORDER_THIN, BadgeKind, BreadcrumbCrumb, ChatBody, ChatRow, ChipGlyph, Density, FONT_MD,
    FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, MenuPlacement, Platform,
    PlatformKind, Radius, ResizeEdge, ResizeRange, SearchState, Spacing, TextInput, ToastKind,
    avatar_tile, badge, badge_color, badge_label, body_family, chat_gap_row, chat_row, chip,
    context_menu, empty_state, hash_accent, icon, install_resize, menu_button, menu_divider,
    menu_header, menu_item, mono_family, page_frame, platform_color, radius, spacing, status_dot,
    tr,
};
use forge_runtime::ActionEngineHandle;
use forge_speak_queue::SpeakQueueHandle;
use forge_storage::{ChatHistoryRepo, Viewer, ViewerRepo, VoiceAliasRepo};
use forge_types::{Shared, SubActionStep, Variant, is_bot_account};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, Entity, FontWeight, ListAlignment, ListState,
    MouseButton, MouseDownEvent, Pixels, Point, Rgba, SharedString, Subscription, Window, div,
    list, prelude::*, px, uniform_list,
};

use crate::async_bridge;
use crate::chat_author::AuthorKey;
use crate::chat_drawer::{
    DASH, SubStatus, ViewerDirectory, ViewerSummary, author_summary, current_name,
    displayed_viewer, drawer_matches, selected_summary,
};
use crate::chat_feed::{ChatFeed, ChatMessage};
use crate::home_stats::HomeStats;
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;
use crate::window_presence::PresenceGate;

mod ban_labels;
mod banned_panel;
mod banned_tab;
mod composer;
mod platform_gate;
mod send_plan;
mod viewer_actions;
mod viewer_follow;
mod viewer_history;
mod viewer_tts;

use banned_tab::BannedHost;
pub use composer::ChatComposer;
use viewer_actions::{ViewerAction, ViewerTarget};
use viewer_follow::{FollowLookups, follow_display};
use viewer_history::{ViewerHistory, ViewerHistoryDismissed};
use viewer_tts::TtsVoiceHost;

const LIST_OVERDRAW: Pixels = px(240.0);
const PILL_BOTTOM_LIFT: Pixels = px(16.0);
const SEARCH_W: Pixels = px(240.0);
const VIEWER_DOT: Pixels = px(6.0);
const CHIP_DIVIDER_W: Pixels = px(0.5);
const CHIP_DIVIDER_H: Pixels = px(14.0);
const ICON_BTN_PAD: Pixels = px(5.0);
const ICON_BTN_RADIUS: Pixels = px(5.0);
const EXPORT_HEADER_RULE: usize = 48;

const DRAWER_WIDTH: Pixels = px(320.0);
const DRAWER_MIN: Pixels = px(260.0);
const DRAWER_MAX: Pixels = px(520.0);
const AVATAR_DETAIL: Pixels = px(38.0);
const AVATAR_ROW: Pixels = px(22.0);
const ROW_STRIPE: Pixels = px(2.0);
const DRAWER_ICON: Pixels = px(11.0);
const BADGE_DETAIL: Pixels = px(9.0);
const BADGE_ROW: Pixels = px(8.5);
const VIEWER_REFRESH: Duration = Duration::from_secs(15);
const INFINITY_GLYPH: &str = "\u{221e}";
const DISABLED_OPACITY: f32 = 0.5;
const DRAWER_TIMEOUT_SECONDS: i64 = 600;
const CTX_TIMEOUT_10M: i64 = 600;
const CTX_TIMEOUT_1H: i64 = 3600;
const CTX_TIMEOUT_2W: i64 = 1_209_600;

fn build_reply_step(username: &str, message: &str, parent_message_id: &str) -> SubActionStep {
    let mut config = BTreeMap::new();
    config.insert("message".to_owned(), Variant::String(message.to_owned()));
    config.insert(
        "parent_message_id".to_owned(),
        Variant::String(parent_message_id.to_owned()),
    );
    SubActionStep {
        kind_id: "twitch.chat.reply".to_owned(),
        config,
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: Some(format!("Reply to {username}")),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlatformFilter {
    All,
    Single(Platform),
}

#[derive(Clone)]
struct UserMenuTarget {
    position: Point<Pixels>,
    viewer: ViewerTarget,
    message_id: String,
}

#[derive(Clone)]
struct ReplyTarget {
    username: String,
    message_id: String,
}

struct DrawerResizeDrag;

struct ChatViewerCount {
    home_stats: Entity<HomeStats>,
    _stats_obs: Subscription,
}

impl ChatViewerCount {
    fn new(home_stats: Entity<HomeStats>, cx: &mut Context<Self>) -> Self {
        let stats_obs = cx.observe(&home_stats, |_, _, cx| cx.notify());
        Self {
            home_stats,
            _stats_obs: stats_obs,
        }
    }
}

impl Render for ChatViewerCount {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let viewer_count = self.home_stats.read(cx).viewers_display();
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(status_dot(palette.success, VIEWER_DOT))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(format!("{} {}", viewer_count, tr!("chat_viewers_unit"))),
            )
    }
}

pub struct ChatView {
    feed: Entity<ChatFeed>,
    viewer_count: Entity<ChatViewerCount>,
    rt_handle: tokio::runtime::Handle,
    action_engine: ActionEngineHandle,
    voice_alias_repo: Arc<dyn VoiceAliasRepo>,
    speak: Option<SpeakQueueHandle>,
    composer: Entity<ChatComposer>,
    search: SearchState,
    platform_filter: PlatformFilter,
    events_only: bool,
    hide_bots: bool,
    bot_accounts: Shared<Vec<String>>,
    visible: Rc<VecDeque<u64>>,
    appended_through: u64,
    drawer_width: Pixels,
    drawer_search: SearchState,
    drawer_menu_open: Option<Point<Pixels>>,
    selected_viewer: Option<AuthorKey>,
    follows: FollowLookups,
    chat_history_repo: Arc<dyn ChatHistoryRepo>,
    viewer_history: Option<ViewerHistoryHost>,
    tts_voice: Option<TtsVoiceHost>,
    viewers: ViewerDirectory,
    drawer_keys: Vec<AuthorKey>,
    whisper_open: bool,
    whisper_input: Entity<TextInput>,
    reply_target: Option<ReplyTarget>,
    reply_input: Entity<TextInput>,
    user_menu: Option<UserMenuTarget>,
    auto_scroll: bool,
    unread: usize,
    last_seen_seq: u64,
    chat_list: ListState,
    _feed_obs: Subscription,
    _search_sub: Subscription,
    _drawer_search_sub: Subscription,
    _whisper_sub: Subscription,
    _reply_sub: Subscription,
    lifecycle: Option<Entity<IntegrationLifecycle>>,
    _lifecycle_obs: Option<Subscription>,
    banned: BannedHost,
}

struct ViewerHistoryHost {
    view: Entity<ViewerHistory>,
    _dismissed: Subscription,
}

fn platform_display_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Twitch => "Twitch",
        Platform::YouTube => "YouTube",
        Platform::Kick => "Kick",
    }
}

impl ChatView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        feed: Entity<ChatFeed>,
        home_stats: Entity<HomeStats>,
        rt_handle: tokio::runtime::Handle,
        viewer_repo: Arc<dyn ViewerRepo>,
        chat_history_repo: Arc<dyn ChatHistoryRepo>,
        action_engine: ActionEngineHandle,
        voice_alias_repo: Arc<dyn VoiceAliasRepo>,
        speak: Option<SpeakQueueHandle>,
        bot_accounts: Shared<Vec<String>>,
        palette: ForgePalette,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = SearchState::new(cx, palette, tr!("chat_search_placeholder"));
        let drawer_search =
            SearchState::on_surface(cx, palette, tr!("chat_drawer_search_placeholder"));
        let whisper_input = cx.new(|cx| {
            TextInput::new(tr!("chat_drawer_whisper_placeholder"), cx).with_palette(palette)
        });
        let reply_input =
            cx.new(|cx| TextInput::new(tr!("chat_reply_placeholder"), cx).with_palette(palette));

        let feed_obs = cx.observe(&feed, Self::on_feed_changed);
        let composer =
            cx.new(|cx| ChatComposer::new(home_stats.clone(), rt_handle.clone(), palette, cx));
        let viewer_count = cx.new(|cx| ChatViewerCount::new(home_stats, cx));
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);
        let drawer_search_sub = cx.subscribe(drawer_search.field(), Self::on_drawer_search_event);
        let whisper_sub = cx.subscribe(&whisper_input, Self::on_whisper_event);
        let reply_sub = cx.subscribe(&reply_input, Self::on_reply_event);

        let last_seen_seq = feed.read(cx).end_seq();

        let chat_list = ListState::new(0, ListAlignment::Top, LIST_OVERDRAW);
        let list_entity = cx.entity();
        chat_list.set_scroll_handler(move |event, _window, app| {
            let at_bottom = event.visible_range.end >= event.count;
            list_entity.update(app, |this, cx| {
                this.auto_scroll = at_bottom;
                if at_bottom {
                    this.unread = 0;
                    this.last_seen_seq = this.feed.read(cx).end_seq();
                }
                cx.notify();
            });
        });

        Self::spawn_viewer_refresh(viewer_repo, rt_handle.clone(), cx);

        let mut this = Self {
            feed,
            viewer_count,
            rt_handle,
            action_engine,
            voice_alias_repo,
            speak,
            composer,
            search,
            platform_filter: PlatformFilter::All,
            events_only: false,
            hide_bots: false,
            bot_accounts,
            visible: Rc::new(VecDeque::new()),
            appended_through: last_seen_seq,
            drawer_width: DRAWER_WIDTH,
            drawer_search,
            drawer_menu_open: None,
            selected_viewer: None,
            follows: FollowLookups::default(),
            chat_history_repo,
            viewer_history: None,
            tts_voice: None,
            viewers: ViewerDirectory::default(),
            drawer_keys: Vec::new(),
            whisper_open: false,
            whisper_input,
            reply_target: None,
            reply_input,
            user_menu: None,
            auto_scroll: true,
            unread: 0,
            last_seen_seq,
            chat_list,
            _feed_obs: feed_obs,
            _search_sub: search_sub,
            _drawer_search_sub: drawer_search_sub,
            _whisper_sub: whisper_sub,
            _reply_sub: reply_sub,
            lifecycle: None,
            _lifecycle_obs: None,
            banned: BannedHost::default(),
        };
        this.rebuild_visible(cx);
        this.chat_list.reset(this.visible.len());
        this.refresh_drawer_keys(cx);
        this
    }

    fn spawn_viewer_refresh(
        repo: Arc<dyn ViewerRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        let mut presence = PresenceGate::of(cx);
        cx.spawn(async move |this, cx| {
            loop {
                presence.until_visible().await;
                let repo = Arc::clone(&repo);
                let (tx, rx) = tokio::sync::oneshot::channel();
                rt_handle.spawn(async move {
                    let _ = tx.send(repo.list().await);
                });
                match rx.await {
                    Ok(Ok(list)) => {
                        if this
                            .update(cx, |this, cx| this.apply_viewers(list, cx))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(Err(err)) => {
                        eprintln!("forge-desktop: viewer snapshot load failed: {err}");
                    }
                    Err(_) => {}
                }
                cx.background_executor().timer(VIEWER_REFRESH).await;
            }
        })
        .detach();
    }

    fn apply_viewers(&mut self, viewers: Vec<Viewer>, cx: &mut Context<Self>) {
        if self.viewers.viewers() != viewers.as_slice() {
            self.viewers = ViewerDirectory::new(viewers);
            cx.notify();
        }
    }

    fn on_feed_changed(&mut self, feed: Entity<ChatFeed>, cx: &mut Context<Self>) {
        let end = feed.read(cx).end_seq();
        self.sync_visible(cx);
        if self.auto_scroll {
            self.chat_list.scroll_to_end();
            self.unread = 0;
        } else {
            let arrived =
                usize::try_from(end.saturating_sub(self.last_seen_seq)).unwrap_or(usize::MAX);
            self.unread = self.unread.saturating_add(arrived);
        }
        self.last_seen_seq = end;
        self.refresh_drawer_keys(cx);
        cx.notify();
    }

    fn open_viewer_history(&mut self, key: AuthorKey, viewer_name: String, cx: &mut Context<Self>) {
        let feed = self.feed.clone();
        let repo = Arc::clone(&self.chat_history_repo);
        let rt_handle = self.rt_handle.clone();
        let view =
            cx.new(|cx| ViewerHistory::new(key, feed, repo, rt_handle, cx).titled(viewer_name));
        let dismissed = cx.subscribe(&view, Self::on_viewer_history_dismissed);
        self.viewer_history = Some(ViewerHistoryHost {
            view,
            _dismissed: dismissed,
        });
        cx.notify();
    }

    fn on_viewer_history_dismissed(
        &mut self,
        _view: Entity<ViewerHistory>,
        _event: &ViewerHistoryDismissed,
        cx: &mut Context<Self>,
    ) {
        self.viewer_history = None;
        cx.notify();
    }

    fn rebuild_visible(&mut self, cx: &mut Context<Self>) {
        let feed = self.feed.read(cx);
        let visible: VecDeque<u64> = (feed.start_seq()..feed.end_seq())
            .filter(|seq| feed.get(*seq).is_some_and(|m| self.row_visible(m)))
            .collect();
        self.appended_through = feed.end_seq();
        self.visible = Rc::new(visible);
    }

    fn sync_visible(&mut self, cx: &mut Context<Self>) {
        let feed = self.feed.read(cx);
        let start = feed.start_seq();
        let end = feed.end_seq();
        let fresh: Vec<u64> = (self.appended_through.max(start)..end)
            .filter(|seq| feed.get(*seq).is_some_and(|m| self.row_visible(m)))
            .collect();
        self.appended_through = end;

        let visible = Rc::make_mut(&mut self.visible);
        let evicted = visible.iter().take_while(|seq| **seq < start).count();
        visible.drain(..evicted);
        let added = fresh.len();
        visible.extend(fresh);

        if evicted > 0 {
            self.chat_list.splice(0..evicted, 0);
        }
        if added > 0 {
            let current = self.chat_list.item_count();
            self.chat_list.splice(current..current, added);
        }
    }

    fn reset_chat_list(&mut self, cx: &mut Context<Self>) {
        self.rebuild_visible(cx);
        self.chat_list.reset(self.visible.len());
        self.auto_scroll = true;
        self.unread = 0;
        self.last_seen_seq = self.feed.read(cx).end_seq();
    }

    fn refresh_drawer_keys(&mut self, cx: &mut Context<Self>) {
        let search = self.drawer_search.query();
        self.drawer_keys = self
            .feed
            .read(cx)
            .authors()
            .newest_first()
            .filter(|(_, activity)| drawer_matches(&activity.name, search))
            .map(|(key, _)| key.clone())
            .collect();
    }

    fn on_search_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if self.search.on_changed(event) {
            cx.notify();
        }
    }

    fn set_platform_filter(&mut self, filter: PlatformFilter, cx: &mut Context<Self>) {
        self.platform_filter = filter;
        self.reset_chat_list(cx);
        cx.notify();
    }

    fn toggle_events(&mut self, cx: &mut Context<Self>) {
        self.events_only = !self.events_only;
        self.reset_chat_list(cx);
        cx.notify();
    }

    fn toggle_hide_bots(&mut self, cx: &mut Context<Self>) {
        self.hide_bots = !self.hide_bots;
        self.reset_chat_list(cx);
        cx.notify();
    }

    fn set_drawer_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        if self.drawer_width != width {
            self.drawer_width = width;
            cx.notify();
        }
    }

    fn open_viewer(&mut self, key: AuthorKey, cx: &mut Context<Self>) {
        self.selected_viewer = Some(key);
        self.request_selected_follow(cx);
        cx.notify();
    }

    fn on_drawer_search_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if self.drawer_search.on_changed(event) {
            self.refresh_drawer_keys(cx);
            cx.notify();
        }
    }

    fn select_viewer(&mut self, key: AuthorKey, cx: &mut Context<Self>) {
        self.selected_viewer = Some(key);
        self.request_selected_follow(cx);
        cx.notify();
    }

    fn displayed_target(&self, cx: &App) -> Option<ViewerTarget> {
        let authors = self.feed.read(cx).authors();
        let key = displayed_viewer(self.selected_viewer.as_ref(), authors)?;
        let name = current_name(&key, authors, &self.viewers)?;
        Some(ViewerTarget::new(&key, name.to_string()))
    }

    fn toggle_drawer_menu(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.drawer_menu_open = if self.drawer_menu_open.is_some() {
            None
        } else {
            Some(position)
        };
        cx.notify();
    }

    fn close_drawer_menu(&mut self, cx: &mut Context<Self>) {
        if self.drawer_menu_open.is_some() {
            self.drawer_menu_open = None;
            cx.notify();
        }
    }

    fn run_viewer_action(
        &self,
        target: &ViewerTarget,
        action: ViewerAction,
        toast: impl FnOnce(Result<(), String>) -> (ToastKind, String) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(step) = target.step(&action) else {
            return;
        };
        let moderated = matches!(
            action,
            ViewerAction::Ban | ViewerAction::Unban | ViewerAction::Timeout { .. }
        )
        .then_some(target.platform);
        self.dispatch_quick_action_then(
            step,
            target.builtin_id(),
            target.label(&action),
            toast,
            move |this, outcome_ok, cx| {
                if let (true, Some(platform)) = (outcome_ok, moderated) {
                    this.note_moderated(platform, cx);
                }
            },
            cx,
        );
    }

    fn dispatch_quick_action(
        &self,
        step: SubActionStep,
        builtin_id: String,
        label: String,
        toast: impl FnOnce(Result<(), String>) -> (ToastKind, String) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_quick_action_then(step, builtin_id, label, toast, |_, _, _| {}, cx);
    }

    fn dispatch_quick_action_then(
        &self,
        step: SubActionStep,
        builtin_id: String,
        label: String,
        toast: impl FnOnce(Result<(), String>) -> (ToastKind, String) + 'static,
        after: impl FnOnce(&mut Self, bool, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let engine = self.action_engine.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let outcome = async_bridge::run_quick_step(engine, step, builtin_id, label).await;
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| {
            let outcome = rx
                .await
                .unwrap_or_else(|_| Err(tr!("chat_dispatch_cancelled")));
            let _ = this.update(cx, |this, cx| {
                let succeeded = outcome.is_ok();
                let (kind, message) = toast(outcome);
                cx.push_toast(kind, message);
                after(this, succeeded, cx);
            });
        })
        .detach();
    }

    fn shoutout_viewer(&mut self, target: ViewerTarget, cx: &mut Context<Self>) {
        self.drawer_menu_open = None;
        self.run_viewer_action(
            &target,
            ViewerAction::Shoutout,
            |outcome| match outcome {
                Ok(()) => (ToastKind::Success, tr!("chat_drawer_shoutout_sent")),
                Err(e) => (
                    ToastKind::Error,
                    tr!("chat_drawer_shoutout_failed", error = e),
                ),
            },
            cx,
        );
        cx.notify();
    }

    fn timeout_viewer(&mut self, target: ViewerTarget, cx: &mut Context<Self>) {
        self.drawer_menu_open = None;
        self.run_viewer_action(
            &target,
            ViewerAction::Timeout {
                seconds: DRAWER_TIMEOUT_SECONDS,
            },
            |outcome| match outcome {
                Ok(()) => (ToastKind::Success, tr!("chat_drawer_timeout_sent")),
                Err(e) => (
                    ToastKind::Error,
                    tr!("chat_drawer_timeout_failed", error = e),
                ),
            },
            cx,
        );
        cx.notify();
    }

    fn ban_viewer(&mut self, target: ViewerTarget, cx: &mut Context<Self>) {
        self.drawer_menu_open = None;
        self.run_viewer_action(
            &target,
            ViewerAction::Ban,
            |outcome| match outcome {
                Ok(()) => (ToastKind::Success, tr!("chat_drawer_ban_sent")),
                Err(e) => (ToastKind::Error, tr!("chat_drawer_ban_failed", error = e)),
            },
            cx,
        );
        cx.notify();
    }

    fn open_user_menu(
        &mut self,
        position: Point<Pixels>,
        viewer: ViewerTarget,
        message_id: String,
        cx: &mut Context<Self>,
    ) {
        self.user_menu = Some(UserMenuTarget {
            position,
            viewer,
            message_id,
        });
        cx.notify();
    }

    fn close_user_menu(&mut self, cx: &mut Context<Self>) {
        if self.user_menu.is_some() {
            self.user_menu = None;
            cx.notify();
        }
    }

    fn ctx_timeout_viewer(&mut self, seconds: i64, cx: &mut Context<Self>) {
        let Some(target) = self.user_menu.take() else {
            cx.notify();
            return;
        };
        self.run_viewer_action(
            &target.viewer,
            ViewerAction::Timeout { seconds },
            |outcome| match outcome {
                Ok(()) => (ToastKind::Success, tr!("chat_ctx_timeout_sent")),
                Err(e) => (
                    ToastKind::Error,
                    tr!("chat_drawer_timeout_failed", error = e),
                ),
            },
            cx,
        );
        cx.notify();
    }

    fn ctx_ban_viewer(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.user_menu.take() else {
            cx.notify();
            return;
        };
        self.run_viewer_action(
            &target.viewer,
            ViewerAction::Ban,
            |outcome| match outcome {
                Ok(()) => (ToastKind::Success, tr!("chat_drawer_ban_sent")),
                Err(e) => (ToastKind::Error, tr!("chat_drawer_ban_failed", error = e)),
            },
            cx,
        );
        cx.notify();
    }

    fn open_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.user_menu.take() else {
            cx.notify();
            return;
        };
        if target.viewer.platform != Platform::Twitch {
            cx.notify();
            return;
        }
        self.reply_target = Some(ReplyTarget {
            username: target.viewer.name,
            message_id: target.message_id,
        });
        self.reply_input.update(cx, |input, cx| {
            input.clear(cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn cancel_reply(&mut self, cx: &mut Context<Self>) {
        self.reply_target = None;
        self.reply_input.update(cx, |input, cx| input.clear(cx));
        cx.notify();
    }

    fn send_reply(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.reply_target.clone() else {
            return;
        };
        let message = self.reply_input.read(cx).content().trim().to_owned();
        if message.is_empty() {
            return;
        }
        self.reply_target = None;
        self.reply_input.update(cx, |input, cx| input.clear(cx));
        let step = build_reply_step(&target.username, &message, &target.message_id);
        let username = target.username;
        self.dispatch_quick_action(
            step,
            platform_gate::platform_integration(Platform::Twitch)
                .id_str()
                .to_owned(),
            format!("Reply to {username}"),
            |outcome| match outcome {
                Ok(()) => (ToastKind::Success, tr!("chat_reply_sent")),
                Err(e) => (ToastKind::Error, tr!("chat_reply_failed", error = e)),
            },
            cx,
        );
        cx.notify();
    }

    fn on_reply_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Submitted(_) => self.send_reply(cx),
            InputEvent::Cancelled => self.cancel_reply(cx),
            InputEvent::Changed(_) | InputEvent::Blurred(_) => {}
        }
    }

    fn render_user_menu(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let target = self.user_menu.as_ref()?;
        let position = target.position;
        let viewer = &target.viewer;
        let can_reply = viewer.platform == Platform::Twitch;
        let view = cx.entity();

        let mut items = vec![menu_header(SharedString::from(viewer.name.clone()))];
        if can_reply {
            items.push(
                menu_item(
                    "chat-ctx-reply",
                    tr!("chat_reply"),
                    cx.listener(|this, _: &ClickEvent, window, cx| this.open_reply(window, cx)),
                )
                .icon(Icon::MessageCircle)
                .into(),
            );
        }
        let timeouts = [
            (
                "chat-ctx-timeout-10m",
                tr!("chat_ctx_timeout_10m"),
                CTX_TIMEOUT_10M,
                None,
            ),
            (
                "chat-ctx-timeout-1h",
                tr!("chat_ctx_timeout_1h"),
                CTX_TIMEOUT_1H,
                None,
            ),
            (
                "chat-ctx-timeout-2w",
                tr!("chat_ctx_timeout_2w"),
                CTX_TIMEOUT_2W,
                Some(palette.warning),
            ),
        ];
        let mut moderation = Vec::new();
        for (id, label, seconds, color) in timeouts {
            if !viewer.supports(&ViewerAction::Timeout { seconds }) {
                continue;
            }
            let mut item = menu_item(
                id,
                label,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.ctx_timeout_viewer(seconds, cx)
                }),
            )
            .icon(Icon::Clock);
            if let Some(color) = color {
                item = item.color(color);
            }
            moderation.push(item.into());
        }
        if viewer.supports(&ViewerAction::Ban) {
            if !moderation.is_empty() {
                moderation.push(menu_divider());
            }
            moderation.push(
                menu_item(
                    "chat-ctx-ban",
                    tr!("chat_ctx_ban"),
                    cx.listener(|this, _: &ClickEvent, _, cx| this.ctx_ban_viewer(cx)),
                )
                .icon(Icon::BellOff)
                .color(palette.random)
                .into(),
            );
        }
        if can_reply && !moderation.is_empty() {
            items.push(menu_divider());
        }
        items.extend(moderation);

        Some(
            context_menu(position, palette)
                .items(items)
                .on_dismiss(move |_window, cx| {
                    view.update(cx, |this, cx| this.close_user_menu(cx));
                })
                .into_any_element(),
        )
    }

    fn open_whisper(&mut self, key: AuthorKey, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_viewer = Some(key);
        self.request_selected_follow(cx);
        self.drawer_menu_open = None;
        self.whisper_open = true;
        self.whisper_input.update(cx, |input, cx| {
            input.clear(cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn cancel_whisper(&mut self, cx: &mut Context<Self>) {
        self.whisper_open = false;
        self.whisper_input.update(cx, |input, cx| input.clear(cx));
        cx.notify();
    }

    fn send_whisper(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.displayed_target(cx) else {
            return;
        };
        let message = self.whisper_input.read(cx).content().trim().to_owned();
        if message.is_empty() {
            return;
        }
        self.whisper_open = false;
        self.whisper_input.update(cx, |input, cx| input.clear(cx));
        self.run_viewer_action(
            &target,
            ViewerAction::Whisper(message),
            |outcome| match outcome {
                Ok(()) => (ToastKind::Success, tr!("chat_drawer_whisper_sent")),
                Err(e) => (
                    ToastKind::Error,
                    tr!("chat_drawer_whisper_failed", error = e),
                ),
            },
            cx,
        );
        cx.notify();
    }

    fn on_whisper_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Submitted(_) => self.send_whisper(cx),
            InputEvent::Cancelled => self.cancel_whisper(cx),
            InputEvent::Changed(_) | InputEvent::Blurred(_) => {}
        }
    }

    fn jump_to_latest(&mut self, cx: &mut Context<Self>) {
        self.auto_scroll = true;
        self.chat_list.scroll_to_end();
        self.unread = 0;
        self.last_seen_seq = self.feed.read(cx).end_seq();
        cx.notify();
    }

    fn export_chat_log(&self, cx: &mut Context<Self>) {
        let feed = self.feed.read(cx);
        let mut lines: Vec<String> = Vec::new();
        for msg in feed.messages().iter().filter(|m| self.row_visible(m)) {
            let text = body_export_text(&msg.body);
            if msg.username.is_empty() {
                lines.push(format!("[{}] {}", msg.timestamp, text));
            } else {
                lines.push(format!("[{}] {}: {}", msg.timestamp, msg.username, text));
            }
        }

        let stamp = time::OffsetDateTime::now_utc();
        let when = stamp
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| stamp.unix_timestamp().to_string());
        let header = format!(
            "Forge chat log · exported {} · {} lines\n{}\n",
            when,
            lines.len(),
            "-".repeat(EXPORT_HEADER_RULE),
        );
        let contents = format!("{header}{}", lines.join("\n"));
        let filename = format!("forge-chat-{}.txt", stamp.unix_timestamp());

        self.rt_handle.spawn(async move {
            let path = forge_platform_core::paths::data_dir().join(filename);
            match tokio::fs::write(&path, contents).await {
                Ok(()) => tracing::info!(path = %path.display(), "chat log exported"),
                Err(e) => tracing::warn!(error = %e, "chat log export failed"),
            }
        });
    }

    fn row_visible(&self, msg: &ChatMessage) -> bool {
        let platform_ok = match self.platform_filter {
            PlatformFilter::All => true,
            PlatformFilter::Single(p) => msg.platform == p,
        };
        let events_ok = !self.events_only || msg.is_event;
        let bots_ok = !self.hide_bots
            || !(msg.is_bot || is_bot_account(&msg.username, &self.bot_accounts.load()));
        platform_ok && events_ok && bots_ok
    }

    fn username_color(msg: &ChatMessage, palette: &ForgePalette) -> Rgba {
        if let Some(color) = msg.author_color {
            color
        } else if !msg.username.is_empty() {
            hash_accent(&msg.username, palette)
        } else {
            match msg.platform {
                Platform::Twitch => platform_color(PlatformKind::Twitch, palette),
                Platform::YouTube => platform_color(PlatformKind::YouTube, palette),
                Platform::Kick => platform_color(PlatformKind::Kick, palette),
            }
        }
    }

    fn render_header_right(&self) -> impl IntoElement + use<> {
        self.viewer_count.clone()
    }

    fn render_filter_left(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let platform_chips = [
            (
                "chat-chip-all",
                tr!("chat_filter_all"),
                PlatformFilter::All,
                palette.brand,
            ),
            (
                "chat-chip-twitch",
                "Twitch".to_owned(),
                PlatformFilter::Single(Platform::Twitch),
                palette.brand,
            ),
            (
                "chat-chip-youtube",
                "YouTube".to_owned(),
                PlatformFilter::Single(Platform::YouTube),
                palette.random,
            ),
            (
                "chat-chip-kick",
                "Kick".to_owned(),
                PlatformFilter::Single(Platform::Kick),
                palette.info,
            ),
        ];

        let mut chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density));
        for (id, label, filter, dot) in platform_chips {
            if !self.filter_offered(filter, cx) {
                continue;
            }
            let active = self.platform_filter == filter;
            chips = chips.child(
                chip(label, ChipGlyph::Dot(dot), active, palette)
                    .density(density)
                    .on_click(
                        id,
                        cx.listener(move |this, _, _, cx| this.set_platform_filter(filter, cx)),
                    ),
            );
        }
        chips = chips.child(
            div()
                .w(CHIP_DIVIDER_W)
                .h(CHIP_DIVIDER_H)
                .bg(palette.border_regular),
        );
        chips = chips.child(
            chip(
                tr!("chat_filter_events"),
                ChipGlyph::None,
                self.events_only,
                palette,
            )
            .density(density)
            .on_click(
                "chat-chip-events",
                cx.listener(|this, _, _, cx| this.toggle_events(cx)),
            ),
        );
        chips = chips.child(
            chip(
                tr!("chat_filter_hide_bots"),
                ChipGlyph::Icon(Icon::BellOff, palette.text_faint),
                self.hide_bots,
                palette,
            )
            .density(density)
            .on_click(
                "chat-chip-hide-bots",
                cx.listener(|this, _, _, cx| this.toggle_hide_bots(cx)),
            ),
        );

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(div().w(SEARCH_W).child(self.search.field().clone()))
            .child(chips)
            .into_any_element()
    }

    fn render_filter_right(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let surf = palette.surface_overlay;
        let green = palette.success;

        div()
            .id("chat-export")
            .flex()
            .items_center()
            .justify_center()
            .p(ICON_BTN_PAD)
            .rounded(ICON_BTN_RADIUS)
            .cursor_pointer()
            .hover(move |s| s.bg(surf))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.export_chat_log(cx)))
            .child(icon(Icon::Download, px(14.0), green))
            .into_any_element()
    }

    fn render_chat_area(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let query = self.search.query().to_string();
        let search_active = !query.is_empty();

        let snapshot: Rc<VecDeque<u64>> = self.visible.clone();
        let empty = snapshot.is_empty();
        let feed = self.feed.clone();

        let row_gap = spacing(Spacing::Xxs, density);
        let pal = *palette;
        let view = cx.entity();
        let list_el = list(self.chat_list.clone(), move |ix, _window, app| {
            let Some(seq) = snapshot.get(ix).copied() else {
                return div().into_any_element();
            };
            let previous = ix
                .checked_sub(1)
                .and_then(|prev| snapshot.get(prev).copied());
            let feed_state = feed.read(app);
            let Some(msg) = feed_state.get(seq) else {
                return div().into_any_element();
            };
            let gap = feed_state.gap_before(previous, seq);
            let data = ChatRow {
                id: msg.id.clone(),
                timestamp: msg.timestamp.clone(),
                platform: msg.platform,
                badges: msg.badges.clone(),
                username: msg.username.clone(),
                username_color: Self::username_color(msg, &pal),
                body: msg.body.clone(),
                moderated: msg.moderated,
                reply: msg.reply.clone(),
            };
            let author_key = msg.author_key();
            let menu_view = view.clone();
            let menu_target = author_key
                .as_ref()
                .map(|key| ViewerTarget::new(key, msg.username.to_string()));
            let menu_message_id = msg.id.clone();
            let view = view.clone();
            let row = chat_row(&pal, data).on_username_click(
                (gpui::ElementId::from("chat-username"), msg.id.clone()),
                move |_: &ClickEvent, _, app| {
                    if let Some(key) = author_key.clone() {
                        view.update(app, |this, cx| this.open_viewer(key, cx));
                    }
                },
            );
            let mut framed = div().pb(row_gap);
            if !gap.is_empty() {
                framed = framed.child(div().pb(row_gap).child(chat_gap_row(
                    gap.label(),
                    &pal,
                    density,
                )));
            }
            framed = framed.child(row);
            if let Some(menu_target) = menu_target {
                framed = framed.on_mouse_down(
                    MouseButton::Right,
                    move |event: &MouseDownEvent, _window, app| {
                        let position = event.position;
                        let viewer = menu_target.clone();
                        let message_id = menu_message_id.to_string();
                        menu_view.update(app, |this, cx| {
                            this.open_user_menu(position, viewer, message_id, cx)
                        });
                    },
                );
            }
            if search_active && !msg.matches_query(&query) {
                framed.opacity(0.3).into_any_element()
            } else {
                framed.into_any_element()
            }
        })
        .flex_1()
        .min_h(px(0.0))
        .py(spacing(Spacing::Sm, density))
        .px(spacing(Spacing::Md, density));

        let body: AnyElement = if empty {
            div()
                .w_full()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(empty_state(tr!("chat_no_filter_matches"), palette).density(density))
                .into_any_element()
        } else {
            list_el.into_any_element()
        };

        let mut area = div()
            .relative()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(body);

        if self.unread > 0 {
            let label = if self.unread == 1 {
                tr!("chat_new_message")
            } else {
                tr!("chat_new_messages", count = self.unread as i64)
            };
            let pill = div()
                .id("chat-unread-pill")
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, density))
                .py(spacing(Spacing::Xs, density))
                .px(spacing(Spacing::Sm, density))
                .rounded(radius(Radius::Pill))
                .bg(palette.brand)
                .cursor_pointer()
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.jump_to_latest(cx)))
                .child(icon(Icon::ArrowDown, FONT_XS, palette.shell))
                .child(
                    div()
                        .font_family(mono_family())
                        .text_size(FONT_XS)
                        .text_color(palette.shell)
                        .child(label),
                );
            let overlay = div()
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_end()
                .pb(PILL_BOTTOM_LIFT)
                .child(pill);
            area = area.child(overlay);
        }

        area
    }

    fn render_drawer(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let authors = self.feed.read(cx).authors();
        let total = authors.len();
        let shown = self.drawer_keys.len();
        let detail = selected_summary(
            self.selected_viewer.as_ref(),
            authors,
            &self.viewers,
            palette,
        );
        let selected_key = detail.as_ref().map(|d| d.key.clone());

        let header = self.render_drawer_header(total, shown, palette, density);
        let detail_el = self.render_selected_detail(detail, palette, density, cx);
        let list_el = self.render_viewer_list(selected_key, shown, palette, density, cx);

        let panel = div()
            .w(self.drawer_width)
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(palette.shell)
            .border_l(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(header)
            .child(detail_el)
            .child(list_el);

        install_resize(
            panel,
            DrawerResizeDrag,
            "chat-drawer-resize",
            ResizeEdge::Left,
            ResizeRange {
                min: DRAWER_MIN,
                max: DRAWER_MAX,
            },
            palette,
            cx.listener(|this, width: &Pixels, _, cx| this.set_drawer_width(*width, cx)),
        )
    }

    fn render_drawer_header(
        &self,
        total: usize,
        shown: usize,
        palette: &ForgePalette,
        density: Density,
    ) -> impl IntoElement + use<> {
        let count = tr!(
            "chat_drawer_active_count",
            total = total as i64,
            shown = shown as i64
        );
        let title = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(Icon::Users, px(13.0), palette.brand))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("chat_viewers_title")),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(count),
            );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(title)
            .child(self.drawer_search.field().clone())
    }

    fn render_selected_detail(
        &self,
        detail: Option<ViewerSummary>,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let frame = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .p(spacing(Spacing::Sm, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular);

        let Some(summary) = detail else {
            return frame
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_faint)
                        .child(tr!("chat_drawer_click_hint")),
                )
                .into_any_element();
        };

        let avatar = viewer_avatar(
            summary.avatar_letter,
            summary.avatar_color,
            AVATAR_DETAIL,
            Radius::Md,
            FONT_MD,
            palette,
        );

        let mut name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(SharedString::from(summary.username.clone())),
            );
        if let Some(role) = summary.role {
            name_row = name_row.child(drawer_role_badge(role, BADGE_DETAIL, palette));
        }

        let last_seen = tr!(
            "chat_drawer_last_seen",
            when = summary.last_seen_label.clone()
        );
        let name_col = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(name_row)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(last_seen),
            );

        let info = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(avatar)
            .child(name_col);

        let (sub_value, sub_color) = sub_display(summary.sub, palette);
        let follow = follow_display(
            summary.key.platform,
            summary.role,
            self.follows.status_of(&summary.key),
            palette,
        );
        let tile_hover = palette.surface_overlay;
        let history_key = summary.key.clone();
        let history_name = summary.username.clone();
        let grid = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div().flex().gap(spacing(Spacing::Xs, density)).child(
                    stat_cell(
                        tr!("chat_stat_messages"),
                        summary.message_count.to_string(),
                        palette.text_primary,
                        palette,
                        density,
                    )
                    .id("chat-drawer-messages")
                    .cursor_pointer()
                    .hover(move |style| style.bg(tile_hover))
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            this.open_viewer_history(history_key.clone(), history_name.clone(), cx)
                        },
                    )),
                ),
            )
            .child(
                div()
                    .flex()
                    .gap(spacing(Spacing::Xs, density))
                    .child(stat_cell(
                        tr!("chat_stat_sub"),
                        sub_value,
                        sub_color,
                        palette,
                        density,
                    ))
                    .children(follow.map(|(follow_value, follow_color)| {
                        stat_cell(
                            tr!("chat_stat_follow"),
                            follow_value,
                            follow_color,
                            palette,
                            density,
                        )
                    })),
            );

        let target = ViewerTarget::new(&summary.key, summary.username.clone());
        let can_whisper = target.supports(&ViewerAction::Whisper(String::new()));
        let shoutout_target = target.clone();
        let whisper_key = summary.key.clone();
        let actions = div()
            .flex()
            .gap(spacing(Spacing::Xs, density))
            .child(drawer_ghost_button(
                "chat-drawer-shoutout",
                Icon::Bolt,
                tr!("chat_drawer_shoutout"),
                target.supports(&ViewerAction::Shoutout),
                palette,
                density,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.shoutout_viewer(shoutout_target.clone(), cx)
                }),
            ))
            .child(drawer_ghost_button(
                "chat-drawer-whisper",
                Icon::MessageCircle,
                tr!("chat_drawer_whisper"),
                can_whisper,
                palette,
                density,
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_whisper(whisper_key.clone(), window, cx)
                }),
            ))
            .child(self.render_drawer_menu(&summary.key, &target, palette, cx));

        let whisper = (self.whisper_open && can_whisper)
            .then(|| self.render_whisper_compose(summary.username.clone(), palette, density, cx));

        frame
            .child(info)
            .child(grid)
            .child(actions)
            .children(whisper)
            .into_any_element()
    }

    fn render_whisper_compose(
        &self,
        recipient: String,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let title = div()
            .font_family(body_family())
            .font_weight(FontWeight::MEDIUM)
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(tr!("chat_drawer_whisper_title", recipient = recipient));

        let border = palette.border_regular;
        let border_hover = palette.border_input;
        let surf = palette.surface_overlay;
        let text = palette.text_secondary;
        let text_hover = palette.text_primary;
        let cancel = div()
            .id("chat-drawer-whisper-cancel")
            .flex()
            .items_center()
            .justify_center()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(border)
            .cursor_pointer()
            .hover(move |s| s.bg(surf).border_color(border_hover).text_color(text_hover))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_whisper(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(text)
                    .child(tr!("chat_drawer_whisper_cancel")),
            );

        let brand = palette.brand;
        let shell = palette.shell;
        let send = div()
            .id("chat-drawer-whisper-send")
            .flex()
            .items_center()
            .justify_center()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(brand)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.send_whisper(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(shell)
                    .child(tr!("chat_drawer_whisper_send")),
            );

        let buttons = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, density))
            .child(cancel)
            .child(send);

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .p(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.elevated)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(title)
            .child(self.whisper_input.clone())
            .child(buttons)
    }

    fn render_reply_compose(
        &self,
        recipient: String,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let title = div()
            .font_family(body_family())
            .font_weight(FontWeight::MEDIUM)
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(tr!("chat_reply_title", recipient = recipient));

        let border = palette.border_regular;
        let border_hover = palette.border_input;
        let surf = palette.surface_overlay;
        let text = palette.text_secondary;
        let text_hover = palette.text_primary;
        let cancel = div()
            .id("chat-reply-cancel")
            .flex()
            .items_center()
            .justify_center()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(border)
            .cursor_pointer()
            .hover(move |s| s.bg(surf).border_color(border_hover).text_color(text_hover))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_reply(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(text)
                    .child(tr!("chat_drawer_whisper_cancel")),
            );

        let brand = palette.brand;
        let shell = palette.shell;
        let send = div()
            .id("chat-reply-send")
            .flex()
            .items_center()
            .justify_center()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(brand)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.send_reply(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(shell)
                    .child(tr!("chat_drawer_whisper_send")),
            );

        let buttons = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, density))
            .child(cancel)
            .child(send);

        let card = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .p(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.elevated)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(title)
            .child(self.reply_input.clone())
            .child(buttons);

        div()
            .w_full()
            .px(spacing(Spacing::Md, density))
            .pb(spacing(Spacing::Xs, density))
            .child(card)
    }

    fn render_drawer_menu(
        &self,
        key: &AuthorKey,
        target: &ViewerTarget,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let shoutout_target = target.clone();
        let whisper_key = key.clone();
        let block_target = target.clone();
        let voice_target = target.clone();
        let has_tts_key = target.tts_alias_key().is_some();
        let timeout_target = target.clone();
        let ban_target = target.clone();
        let items = vec![
            menu_item(
                "chat-drawer-menu-shoutout",
                tr!("chat_drawer_shoutout"),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.shoutout_viewer(shoutout_target.clone(), cx)
                }),
            )
            .icon(Icon::Flag)
            .disabled(!target.supports(&ViewerAction::Shoutout))
            .into(),
            menu_item(
                "chat-drawer-menu-whisper",
                tr!("chat_drawer_whisper"),
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_whisper(whisper_key.clone(), window, cx)
                }),
            )
            .icon(Icon::MessageCircle)
            .disabled(!target.supports(&ViewerAction::Whisper(String::new())))
            .into(),
            menu_item(
                "chat-drawer-menu-tts-voice",
                tr!("chat_drawer_set_tts_voice"),
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_tts_voice(voice_target.clone(), window, cx)
                }),
            )
            .icon(Icon::Pencil)
            .disabled(!has_tts_key)
            .into(),
            menu_divider(),
            menu_item(
                "chat-drawer-menu-block-tts",
                tr!("chat_drawer_block_tts"),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.block_tts_viewer(block_target.clone(), cx)
                }),
            )
            .color(palette.warning)
            .disabled(!has_tts_key)
            .into(),
            menu_item(
                "chat-drawer-menu-timeout",
                tr!("chat_drawer_timeout"),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.timeout_viewer(timeout_target.clone(), cx)
                }),
            )
            .color(palette.warning)
            .disabled(!target.supports(&ViewerAction::Timeout {
                seconds: DRAWER_TIMEOUT_SECONDS,
            }))
            .into(),
            menu_item(
                "chat-drawer-menu-ban",
                tr!("chat_drawer_ban"),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.ban_viewer(ban_target.clone(), cx)
                }),
            )
            .color(palette.random)
            .disabled(!target.supports(&ViewerAction::Ban))
            .into(),
        ];

        menu_button(Icon::DotsVertical, self.drawer_menu_open.is_some(), palette)
            .placement(MenuPlacement::TopRight)
            .open_at(self.drawer_menu_open)
            .items(items)
            .on_toggle(
                "chat-drawer-menu-trigger",
                cx.listener(|this, ev: &ClickEvent, _, cx| {
                    this.toggle_drawer_menu(ev.position(), cx)
                }),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_drawer_menu(cx));
            })
            .into_any_element()
    }

    fn render_viewer_list(
        &self,
        selected_key: Option<AuthorKey>,
        shown: usize,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let header = div()
            .flex_none()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!("chat_drawer_section_active", count = shown as i64));

        let body = if shown == 0 {
            div()
                .py(spacing(Spacing::Xs, density))
                .px(spacing(Spacing::Sm, density))
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(tr!("chat_drawer_no_matches"))
                .into_any_element()
        } else {
            let pal = *palette;
            uniform_list(
                "chat-drawer-list",
                shown,
                cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                    let mut rows = Vec::with_capacity(range.len());
                    for ix in range {
                        let Some(summary) = this.drawer_keys.get(ix).and_then(|key| {
                            author_summary(key, this.feed.read(cx).authors(), &this.viewers, &pal)
                        }) else {
                            continue;
                        };
                        let is_sel = selected_key.as_ref() == Some(&summary.key);
                        rows.push(
                            this.render_viewer_row(summary, is_sel, &pal, density, cx)
                                .into_any_element(),
                        );
                    }
                    rows
                }),
            )
            .flex_1()
            .min_h(px(0.0))
            .into_any_element()
        };

        div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(header)
            .child(body)
    }

    fn render_viewer_row(
        &self,
        summary: ViewerSummary,
        is_sel: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let key = summary.key.clone();
        let row_id = summary.key.element_id("chat-drawer-row");
        let stripe = if is_sel {
            palette.brand
        } else {
            with_transparent(palette.brand)
        };
        let selected_bg = palette.surface_overlay;
        let hover_bg = palette.elevated;

        let avatar = viewer_avatar(
            summary.avatar_letter,
            summary.avatar_color,
            AVATAR_ROW,
            Radius::Sm,
            FONT_XXS,
            palette,
        );

        let mut name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(SharedString::from(summary.username.clone())),
            );
        if let Some(role) = summary.role {
            name_row = name_row.child(drawer_role_badge(role, BADGE_ROW, palette));
        }

        let meta = format!("{} msg", summary.message_count);
        let name_col = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(name_row)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(meta),
            );

        let last_seen = div()
            .flex_none()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(SharedString::from(summary.last_seen_label.clone()));

        let mut row = div()
            .id(row_id)
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .border_l(ROW_STRIPE)
            .border_color(stripe)
            .cursor_pointer()
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.select_viewer(key.clone(), cx)),
            )
            .child(avatar)
            .child(name_col)
            .child(last_seen);

        if is_sel {
            row = row.bg(selected_bg);
        } else {
            row = row.hover(move |s| s.bg(hover_bg));
        }
        row
    }
}

fn viewer_avatar(
    letter: char,
    color: Rgba,
    size: Pixels,
    corner: Radius,
    font: Pixels,
    palette: &ForgePalette,
) -> impl IntoElement {
    avatar_tile(letter.to_string(), color, palette)
        .size(size)
        .corner(radius(corner))
        .font(font)
}

fn drawer_role_badge(kind: BadgeKind, size: Pixels, palette: &ForgePalette) -> impl IntoElement {
    badge(
        palette.surface_overlay,
        badge_color(kind, palette),
        badge_label(kind),
        false,
        size,
    )
}

fn stat_cell(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    color: Rgba,
    palette: &ForgePalette,
    density: Density,
) -> Div {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .p(spacing(Spacing::Xs, density))
        .rounded(radius(Radius::Sm))
        .bg(palette.elevated)
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(label.into()),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(color)
                .child(value.into()),
        )
}

fn sub_display(status: SubStatus, palette: &ForgePalette) -> (SharedString, Rgba) {
    match status {
        SubStatus::Unlimited => (INFINITY_GLYPH.into(), palette.success),
        SubStatus::Subscribed => (tr!("chat_stat_sub_yes").into(), palette.success),
        SubStatus::None => (DASH.into(), palette.text_faint),
    }
}

fn with_transparent(color: Rgba) -> Rgba {
    Rgba { a: 0.0, ..color }
}

fn drawer_ghost_button(
    id: &'static str,
    glyph: Icon,
    label: impl Into<SharedString>,
    enabled: bool,
    palette: &ForgePalette,
    density: Density,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let border = palette.border_regular;
    let border_hover = palette.border_input;
    let surf = palette.surface_overlay;
    let text = palette.text_secondary;
    let text_hover = palette.text_primary;
    let button = div()
        .id(id)
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .gap(spacing(Spacing::Xxs, density))
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .border(BORDER_THIN)
        .border_color(border)
        .child(icon(glyph, DRAWER_ICON, text))
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(text)
                .child(label.into()),
        );
    if enabled {
        button
            .cursor_pointer()
            .hover(move |s| s.bg(surf).border_color(border_hover).text_color(text_hover))
            .on_click(handler)
    } else {
        button.opacity(DISABLED_OPACITY)
    }
}

fn body_export_text(body: &ChatBody) -> String {
    match body {
        ChatBody::Message(text) => text.to_string(),
        ChatBody::Command { command, .. } => command.to_string(),
        ChatBody::Cheer { text, .. } => text.to_string(),
        ChatBody::Subscription {
            descriptor,
            message,
            ..
        } => message
            .as_ref()
            .map_or_else(|| descriptor.to_string(), |m| m.to_string()),
        ChatBody::Raid { descriptor, .. } => descriptor.to_string(),
    }
}

impl Render for ChatView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header_right = self.render_header_right();
        let tabs = self.render_tabs(&palette, cx);
        let banned_panel = self.banned.panel().cloned();
        let feed_tab = banned_panel.is_none();
        let filter_left = feed_tab.then(|| self.render_filter_left(&palette, density, cx));
        let filter_right = feed_tab.then(|| self.render_filter_right(&palette, cx));
        let drawer = self.render_drawer(&palette, density, cx);
        let user_menu = self.render_user_menu(&palette, cx);
        let reply_compose = self
            .reply_target
            .clone()
            .map(|target| self.render_reply_compose(target.username, &palette, density, cx));

        let main_column = match banned_panel {
            Some(panel) => div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(panel),
            None => div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(self.render_chat_area(&palette, density, cx))
                .children(reply_compose)
                .child(self.composer.clone()),
        };
        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_row()
            .overflow_hidden()
            .child(main_column)
            .child(drawer);

        let subheader_left = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(tabs)
            .children(filter_left);

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("chat_breadcrumb_audience")),
                BreadcrumbCrumb::leaf(tr!("chat_breadcrumb_chat")),
            ],
            &palette,
        )
        .header_right(header_right)
        .subheader_left(subheader_left)
        .subheader_right(div().children(filter_right))
        .density(density)
        .body(body);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(user_menu)
            .children(self.viewer_history.as_ref().map(|host| host.view.clone()))
            .children(self.tts_voice.as_ref().map(|host| host.view.clone()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use forge_components::{ChatBody, FORGE_DEFAULT, Platform};
    use forge_registry::SubActionRegistry;
    use forge_runtime::{ActionCancelRegistry, EventBus, spawn_action_engine};
    use forge_storage::Language;
    use forge_storage::chat_history::MockChatHistoryRepo;
    use forge_storage::viewer::MockViewerRepo;
    use forge_storage::voice_aliases::MockVoiceAliasRepo;
    use forge_types::EventId;
    use gpui::{App, AppContext as _, Context, Entity, TestAppContext, VisualTestContext};
    use time::OffsetDateTime;

    use super::{ChatView, PlatformFilter};
    use crate::chat_feed::{ChatFeed, ChatMessage};
    use crate::home_stats::{HomeStats, Integration};
    use crate::i18n::install_language;
    use crate::integration_lifecycle::IntegrationLifecycle;
    use crate::integration_supervisor::{LifecycleState, LifecycleStates};
    use crate::test_support::{
        StubActions, StubEventLog, StubHistory, install_presentation, runtime, switch_lifecycle,
    };
    use crate::toasts::Toasts;

    const CAP: usize = 5;
    const OVERFLOW: usize = 8;

    pub(super) fn message(ix: usize, is_bot: bool) -> ChatMessage {
        ChatMessage {
            id: format!("m{ix}").into(),
            event_id: EventId::new(),
            timestamp: "00:00:00".into(),
            received_at: OffsetDateTime::from_unix_timestamp(0).unwrap(),
            platform: Platform::Twitch,
            badges: vec![],
            username: format!("user{ix}").into(),
            author_id: None,
            author_color: None,
            body: ChatBody::Message("hi".into()),
            is_event: false,
            is_bot,
            moderated: false,
            reply: None,
        }
    }

    pub(super) fn mount(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
    ) -> (Entity<ChatFeed>, Entity<ChatView>) {
        mount_gated(cx, rt, None, MockChatHistoryRepo::new())
    }

    pub(super) fn mount_with_chat_history(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        chat_history: MockChatHistoryRepo,
    ) -> (Entity<ChatFeed>, Entity<ChatView>) {
        mount_gated(cx, rt, None, chat_history)
    }

    fn new_view(
        feed: Entity<ChatFeed>,
        rt: &tokio::runtime::Runtime,
        chat_history: MockChatHistoryRepo,
        voice_aliases: MockVoiceAliasRepo,
        cx: &mut Context<ChatView>,
    ) -> ChatView {
        let home_stats = cx.new(|_| HomeStats::new());
        let mut viewers = MockViewerRepo::new();
        viewers.expect_list().returning(|| Ok(Vec::new()));
        let engine = spawn_action_engine(
            EventBus::new(Arc::new(StubEventLog)),
            crate::test_support::stub_catalog(),
            Arc::new(StubActions),
            Arc::new(StubHistory),
            Arc::new(SubActionRegistry::new()),
            Arc::new(ActionCancelRegistry::new()),
        );
        ChatView::new(
            feed,
            home_stats,
            rt.handle().clone(),
            Arc::new(viewers),
            Arc::new(chat_history),
            engine,
            Arc::new(voice_aliases),
            None,
            forge_types::Shared::default(),
            FORGE_DEFAULT,
            cx,
        )
    }

    fn capped_feed(cx: &mut App) -> Entity<ChatFeed> {
        cx.new(|_| {
            let mut feed = ChatFeed::new();
            feed.set_capacity(CAP);
            feed
        })
    }

    pub(super) fn mount_gated(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        lifecycle: Option<Entity<IntegrationLifecycle>>,
        chat_history: MockChatHistoryRepo,
    ) -> (Entity<ChatFeed>, Entity<ChatView>) {
        let _enter = rt.enter();
        let feed = cx.update(capped_feed);
        let view = cx.new(|cx| {
            let view = new_view(
                feed.clone(),
                rt,
                chat_history,
                MockVoiceAliasRepo::new(),
                cx,
            );
            match lifecycle {
                Some(lifecycle) => view.with_lifecycle(lifecycle, cx),
                None => view,
            }
        });
        (feed, view)
    }

    pub(super) fn mount_in_window<'a>(
        cx: &'a mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        voice_aliases: MockVoiceAliasRepo,
    ) -> (Entity<ChatView>, &'a mut VisualTestContext) {
        install_language(Language::En);
        install_presentation(cx);
        cx.update(|cx| cx.set_global(Toasts::new()));
        let _enter = rt.enter();
        let feed = cx.update(capped_feed);
        let (view, vcx) = cx.add_window_view(|_window, cx| {
            new_view(feed, rt, MockChatHistoryRepo::new(), voice_aliases, cx)
        });
        vcx.update(|window, cx| {
            window.activate_window();
            forge_components::bind_text_input_keys(cx);
        });
        (view, vcx)
    }

    pub(super) fn push_each(
        cx: &mut TestAppContext,
        feed: &Entity<ChatFeed>,
        messages: Vec<ChatMessage>,
    ) {
        for message in messages {
            feed.update(cx, |feed, cx| {
                feed.push(message);
                cx.notify();
            });
            cx.run_until_parked();
        }
    }

    #[gpui::test]
    fn unread_keeps_counting_past_the_feed_cap_while_scrolled_up(cx: &mut TestAppContext) {
        let rt = runtime();
        let (feed, view) = mount(cx, &rt);
        view.update(cx, |view, _| view.auto_scroll = false);

        push_each(
            cx,
            &feed,
            (0..CAP + OVERFLOW).map(|ix| message(ix, false)).collect(),
        );

        assert_eq!(view.read_with(cx, |view, _| view.unread), CAP + OVERFLOW);
    }

    #[gpui::test]
    fn the_visible_rows_and_list_follow_eviction_under_an_active_filter(cx: &mut TestAppContext) {
        let rt = runtime();
        let (feed, view) = mount(cx, &rt);
        view.update(cx, |view, cx| {
            view.hide_bots = true;
            view.reset_chat_list(cx);
        });

        let total = CAP + OVERFLOW;
        push_each(
            cx,
            &feed,
            (0..total).map(|ix| message(ix, ix % 2 == 1)).collect(),
        );

        let start = (total - CAP) as u64;
        let expected: Vec<u64> = (start..total as u64).filter(|seq| seq % 2 == 0).collect();
        let (visible, list_len) = view.read_with(cx, |view, _| {
            (
                view.visible.iter().copied().collect::<Vec<u64>>(),
                view.chat_list.item_count(),
            )
        });
        assert_eq!(visible, expected);
        assert_eq!(list_len, expected.len());
    }

    pub(super) fn chat_states(entries: &[(Integration, LifecycleState)]) -> LifecycleStates {
        entries
            .iter()
            .map(|(integration, state)| (integration.builtin_id(), state.clone()))
            .collect()
    }

    fn offered(cx: &mut TestAppContext, view: &Entity<ChatView>) -> [bool; 4] {
        view.read_with(cx, |view, cx| {
            [
                PlatformFilter::All,
                PlatformFilter::Single(Platform::Twitch),
                PlatformFilter::Single(Platform::YouTube),
                PlatformFilter::Single(Platform::Kick),
            ]
            .map(|filter| view.filter_offered(filter, cx))
        })
    }

    #[gpui::test]
    fn without_a_lifecycle_every_platform_filter_is_offered(cx: &mut TestAppContext) {
        let rt = runtime();
        let (_feed, view) = mount(cx, &rt);

        assert_eq!(offered(cx, &view), [true; 4]);
    }

    #[gpui::test]
    fn only_platforms_the_user_keeps_on_are_offered_as_filters(cx: &mut TestAppContext) {
        let rt = runtime();
        let lifecycle = cx.new(|_| {
            IntegrationLifecycle::new(chat_states(&[
                (Integration::Twitch, LifecycleState::Running),
                (
                    Integration::YouTube,
                    LifecycleState::Failed("quota".to_owned()),
                ),
                (Integration::Kick, LifecycleState::Disabled),
            ]))
        });
        let (_feed, view) = mount_gated(cx, &rt, Some(lifecycle), MockChatHistoryRepo::new());

        assert_eq!(offered(cx, &view), [true, true, true, false]);
    }

    fn filtering_kick(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
    ) -> (Entity<IntegrationLifecycle>, Entity<ChatView>) {
        let lifecycle = cx.new(|_| {
            IntegrationLifecycle::new(chat_states(&[
                (Integration::Twitch, LifecycleState::Running),
                (Integration::Kick, LifecycleState::Running),
            ]))
        });
        let (_feed, view) =
            mount_gated(cx, rt, Some(lifecycle.clone()), MockChatHistoryRepo::new());
        view.update(cx, |view, cx| {
            view.set_platform_filter(PlatformFilter::Single(Platform::Kick), cx)
        });
        (lifecycle, view)
    }

    #[gpui::test]
    fn switching_off_the_filtered_platform_resets_the_filter_to_all(cx: &mut TestAppContext) {
        let rt = runtime();
        let (lifecycle, view) = filtering_kick(cx, &rt);

        switch_lifecycle(
            cx,
            &lifecycle,
            chat_states(&[
                (Integration::Twitch, LifecycleState::Running),
                (Integration::Kick, LifecycleState::Stopping),
            ]),
        );

        assert!(view.read_with(cx, |view, _| view.platform_filter == PlatformFilter::All));
    }

    #[gpui::test]
    fn switching_off_another_platform_keeps_the_active_filter(cx: &mut TestAppContext) {
        let rt = runtime();
        let (lifecycle, view) = filtering_kick(cx, &rt);

        switch_lifecycle(
            cx,
            &lifecycle,
            chat_states(&[
                (Integration::Twitch, LifecycleState::Disabled),
                (Integration::Kick, LifecycleState::Running),
            ]),
        );

        assert!(view.read_with(cx, |view, _| {
            view.platform_filter == PlatformFilter::Single(Platform::Kick)
        }));
    }
}
