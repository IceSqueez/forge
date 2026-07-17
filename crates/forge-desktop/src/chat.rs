use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    BORDER_THIN, BadgeKind, BreadcrumbCrumb, ChatBody, ChatRow, ChipGlyph, DEFAULT_BODY_FAMILY,
    DEFAULT_MONO_FAMILY, Density, FONT_MD, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon,
    InputBar, InputBarEvent, InputEvent, MenuPlacement, Platform, Radius, ResizeEdge, ResizeRange,
    Spacing, TextInput, ToastKind, badge, badge_color, badge_label, breadcrumb, chat_row, chip,
    icon, install_resize, menu_button, menu_divider, menu_item, radius, search_input,
    search_input_on_surface, spacing, status_dot, tr,
};
use forge_runtime::ActionEngineHandle;
use forge_speak_queue::{SpeakCommand, SpeakQueueHandle};
use forge_storage::{Viewer, ViewerRepo, VoiceAliasRepo};
use forge_types::{SubActionStep, Variant};
use forge_voice::{AliasId, AliasState, EngineId, VoiceAlias, VoiceId};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, Pixels, Rgba, ScrollHandle,
    ScrollWheelEvent, SharedString, Subscription, Window, div, prelude::*, px,
};

use crate::chat_drawer::{
    DASH, SubStatus, ViewerSummary, drawer_matches, enrich_with_storage, selected_summary,
    synthesize_from_chat, unique_authors,
};
use crate::chat_feed::{ChatFeed, ChatMessage};
use crate::home_stats::HomeStats;
use crate::presentation::ActivePresentation;
use crate::runtime_status::RuntimeStatus;
use crate::toasts::PushToast;

const AT_BOTTOM_SLACK: f32 = 60.0;
const PILL_BOTTOM_LIFT: Pixels = px(16.0);
const SEARCH_FIELD_WIDTH: Pixels = px(220.0);
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
const DRAWER_TIMEOUT_SECONDS: i64 = 600;

fn build_shoutout_step(login: &str) -> SubActionStep {
    let mut config = BTreeMap::new();
    config.insert(
        "to_broadcaster_login".to_owned(),
        Variant::String(login.to_owned()),
    );
    SubActionStep {
        kind_id: "twitch.channel.send_shoutout".to_owned(),
        config,
        enabled: true,
        label: Some(format!("Shoutout {login}")),
    }
}

fn build_whisper_step(login: &str, message: &str) -> SubActionStep {
    let mut config = BTreeMap::new();
    config.insert(
        "to_user_login".to_owned(),
        Variant::String(login.to_owned()),
    );
    config.insert("message".to_owned(), Variant::String(message.to_owned()));
    SubActionStep {
        kind_id: "twitch.chat.send_whisper".to_owned(),
        config,
        enabled: true,
        label: Some(format!("Whisper {login}")),
    }
}

fn build_timeout_step(login: &str) -> SubActionStep {
    let mut config = BTreeMap::new();
    config.insert(
        "target_user_login".to_owned(),
        Variant::String(login.to_owned()),
    );
    config.insert(
        "duration_seconds".to_owned(),
        Variant::Int(DRAWER_TIMEOUT_SECONDS),
    );
    SubActionStep {
        kind_id: "twitch.moderation.timeout_user".to_owned(),
        config,
        enabled: true,
        label: Some(format!("Timeout {login}")),
    }
}

fn build_ban_step(login: &str) -> SubActionStep {
    let mut config = BTreeMap::new();
    config.insert(
        "target_user_login".to_owned(),
        Variant::String(login.to_owned()),
    );
    SubActionStep {
        kind_id: "twitch.moderation.ban_user".to_owned(),
        config,
        enabled: true,
        label: Some(format!("Ban {login}")),
    }
}

fn blocked_alias(viewer: &str) -> VoiceAlias {
    VoiceAlias {
        id: AliasId::new(),
        viewer_id: viewer.to_owned(),
        viewer_name: viewer.to_owned(),
        engine_id: EngineId(String::new()),
        voice_id: VoiceId(String::new()),
        pitch_semitones: None,
        rate_multiplier: None,
        state: AliasState::Blocked,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PlatformFilter {
    All,
    Single(Platform),
}

struct DrawerResizeDrag;

pub struct ChatView {
    feed: Entity<ChatFeed>,
    home_stats: Entity<HomeStats>,
    status: Entity<RuntimeStatus>,
    rt_handle: tokio::runtime::Handle,
    action_engine: ActionEngineHandle,
    voice_alias_repo: Arc<dyn VoiceAliasRepo>,
    speak: Option<SpeakQueueHandle>,
    input: Entity<InputBar>,
    search_field: Entity<TextInput>,
    platform_filter: PlatformFilter,
    events_only: bool,
    hide_bots: bool,
    search_open: bool,
    search_query: String,
    drawer_open: bool,
    drawer_width: Pixels,
    drawer_search: Entity<TextInput>,
    drawer_query: String,
    drawer_menu_open: bool,
    selected_viewer: Option<String>,
    viewers: Vec<Viewer>,
    whisper_open: bool,
    whisper_input: Entity<TextInput>,
    auto_scroll: bool,
    unread: usize,
    last_seen_len: usize,
    chat_scroll: ScrollHandle,
    _feed_obs: Subscription,
    _stats_obs: Subscription,
    _status_obs: Subscription,
    _input_sub: Subscription,
    _search_sub: Subscription,
    _drawer_search_sub: Subscription,
    _whisper_sub: Subscription,
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
        status: Entity<RuntimeStatus>,
        rt_handle: tokio::runtime::Handle,
        viewer_repo: Arc<dyn ViewerRepo>,
        action_engine: ActionEngineHandle,
        voice_alias_repo: Arc<dyn VoiceAliasRepo>,
        speak: Option<SpeakQueueHandle>,
        palette: ForgePalette,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputBar::new(tr!("chat_send_placeholder_connected"), palette, cx));
        let search_field = cx.new(|cx| search_input(tr!("chat_search_placeholder"), palette, cx));
        let drawer_search = cx
            .new(|cx| search_input_on_surface(tr!("chat_drawer_search_placeholder"), palette, cx));
        let whisper_input = cx.new(|cx| {
            TextInput::new(tr!("chat_drawer_whisper_placeholder"), cx).with_palette(palette)
        });

        let feed_obs = cx.observe(&feed, Self::on_feed_changed);
        let stats_obs = cx.observe(&home_stats, |_, _, cx| cx.notify());
        let status_obs = cx.observe(&status, |_, _, cx| cx.notify());
        let input_sub = cx.subscribe(&input, Self::on_input_event);
        let search_sub = cx.subscribe(&search_field, Self::on_search_event);
        let drawer_search_sub = cx.subscribe(&drawer_search, Self::on_drawer_search_event);
        let whisper_sub = cx.subscribe(&whisper_input, Self::on_whisper_event);

        let last_seen_len = feed.read(cx).messages().len();
        let chat_scroll = ScrollHandle::new();
        chat_scroll.scroll_to_bottom();

        Self::spawn_viewer_refresh(viewer_repo, rt_handle.clone(), cx);

        Self {
            feed,
            home_stats,
            status,
            rt_handle,
            action_engine,
            voice_alias_repo,
            speak,
            input,
            search_field,
            platform_filter: PlatformFilter::All,
            events_only: false,
            hide_bots: false,
            search_open: false,
            search_query: String::new(),
            drawer_open: true,
            drawer_width: DRAWER_WIDTH,
            drawer_search,
            drawer_query: String::new(),
            drawer_menu_open: false,
            selected_viewer: None,
            viewers: Vec::new(),
            whisper_open: false,
            whisper_input,
            auto_scroll: true,
            unread: 0,
            last_seen_len,
            chat_scroll,
            _feed_obs: feed_obs,
            _stats_obs: stats_obs,
            _status_obs: status_obs,
            _input_sub: input_sub,
            _search_sub: search_sub,
            _drawer_search_sub: drawer_search_sub,
            _whisper_sub: whisper_sub,
        }
    }

    fn spawn_viewer_refresh(
        repo: Arc<dyn ViewerRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            loop {
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
        if self.viewers != viewers {
            self.viewers = viewers;
            cx.notify();
        }
    }

    fn on_feed_changed(&mut self, feed: Entity<ChatFeed>, cx: &mut Context<Self>) {
        let len = feed.read(cx).messages().len();
        if self.auto_scroll {
            self.chat_scroll.scroll_to_bottom();
            self.unread = 0;
        } else {
            self.unread = self
                .unread
                .saturating_add(len.saturating_sub(self.last_seen_len));
        }
        self.last_seen_len = len;
        cx.notify();
    }

    fn on_input_event(
        &mut self,
        _input: Entity<InputBar>,
        event: &InputBarEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputBarEvent::Send { .. } => {
                self.input.update(cx, |bar, cx| bar.clear(cx));
                cx.notify();
            }
            InputBarEvent::TargetsChanged => self.refresh_send_placeholder(cx),
            InputBarEvent::EmojiToggled => {}
        }
    }

    fn refresh_send_placeholder(&mut self, cx: &mut Context<Self>) {
        let selected = self.input.read(cx).selected_targets();
        let placeholder = match selected.as_slice() {
            [only] => tr!(
                "chat_send_placeholder_to",
                platform = platform_display_name(*only)
            ),
            _ => tr!("chat_send_placeholder_connected"),
        };
        self.input
            .update(cx, |bar, cx| bar.set_placeholder(placeholder, cx));
    }

    fn on_search_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Changed(text) = event {
            self.search_query = text.to_string();
            cx.notify();
        }
    }

    fn set_platform_filter(&mut self, filter: PlatformFilter, cx: &mut Context<Self>) {
        self.platform_filter = filter;
        cx.notify();
    }

    fn toggle_events(&mut self, cx: &mut Context<Self>) {
        self.events_only = !self.events_only;
        cx.notify();
    }

    fn toggle_hide_bots(&mut self, cx: &mut Context<Self>) {
        self.hide_bots = !self.hide_bots;
        cx.notify();
    }

    fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = !self.search_open;
        if self.search_open {
            self.search_field.update(cx, |f, cx| f.focus(window, cx));
        }
        cx.notify();
    }

    fn toggle_drawer(&mut self, cx: &mut Context<Self>) {
        self.drawer_open = !self.drawer_open;
        cx.notify();
    }

    fn set_drawer_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        if self.drawer_width != width {
            self.drawer_width = width;
            cx.notify();
        }
    }

    fn open_viewer(&mut self, username: SharedString, cx: &mut Context<Self>) {
        self.selected_viewer = Some(username.to_string());
        self.drawer_open = true;
        cx.notify();
    }

    fn on_drawer_search_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Changed(text) = event {
            self.drawer_query = text.to_string();
            cx.notify();
        }
    }

    fn select_viewer(&mut self, username: String, cx: &mut Context<Self>) {
        self.selected_viewer = Some(username);
        cx.notify();
    }

    fn toggle_drawer_menu(&mut self, cx: &mut Context<Self>) {
        self.drawer_menu_open = !self.drawer_menu_open;
        cx.notify();
    }

    fn close_drawer_menu(&mut self, cx: &mut Context<Self>) {
        if self.drawer_menu_open {
            self.drawer_menu_open = false;
            cx.notify();
        }
    }

    fn dispatch_quick_action(
        &self,
        step: SubActionStep,
        label: String,
        toast: impl FnOnce(Result<(), String>) -> (ToastKind, String) + 'static,
        cx: &mut Context<Self>,
    ) {
        let engine = self.action_engine.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let outcome = engine
                .execute_quick_action(step, "twitch".to_owned(), label)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| {
            let outcome = rx
                .await
                .unwrap_or_else(|_| Err("dispatch cancelled".to_owned()));
            let _ = this.update(cx, |_this, cx| {
                let (kind, message) = toast(outcome);
                cx.push_toast(kind, message);
            });
        })
        .detach();
    }

    fn shoutout_viewer(&mut self, cx: &mut Context<Self>) {
        self.drawer_menu_open = false;
        let Some(login) = self.selected_viewer.clone() else {
            cx.notify();
            return;
        };
        let step = build_shoutout_step(&login);
        self.dispatch_quick_action(
            step,
            format!("Shoutout {login}"),
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

    fn timeout_viewer(&mut self, cx: &mut Context<Self>) {
        self.drawer_menu_open = false;
        let Some(login) = self.selected_viewer.clone() else {
            cx.notify();
            return;
        };
        let step = build_timeout_step(&login);
        self.dispatch_quick_action(
            step,
            format!("Timeout {login}"),
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

    fn ban_viewer(&mut self, cx: &mut Context<Self>) {
        self.drawer_menu_open = false;
        let Some(login) = self.selected_viewer.clone() else {
            cx.notify();
            return;
        };
        let step = build_ban_step(&login);
        self.dispatch_quick_action(
            step,
            format!("Ban {login}"),
            |outcome| match outcome {
                Ok(()) => (ToastKind::Success, tr!("chat_drawer_ban_sent")),
                Err(e) => (ToastKind::Error, tr!("chat_drawer_ban_failed", error = e)),
            },
            cx,
        );
        cx.notify();
    }

    fn open_whisper(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_viewer.is_none() {
            return;
        }
        self.drawer_menu_open = false;
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
        let Some(login) = self.selected_viewer.clone() else {
            return;
        };
        let message = self.whisper_input.read(cx).content().trim().to_owned();
        if message.is_empty() {
            return;
        }
        self.whisper_open = false;
        self.whisper_input.update(cx, |input, cx| input.clear(cx));
        let step = build_whisper_step(&login, &message);
        self.dispatch_quick_action(
            step,
            format!("Whisper {login}"),
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
            InputEvent::Changed(_) => {}
        }
    }

    fn block_tts_viewer(&mut self, cx: &mut Context<Self>) {
        self.drawer_menu_open = false;
        let Some(viewer) = self.selected_viewer.clone() else {
            cx.notify();
            return;
        };
        let alias = blocked_alias(&viewer);
        let repo = Arc::clone(&self.voice_alias_repo);
        let speak = self.speak.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let outcome = async move {
                repo.upsert(&alias).await.map_err(|e| e.to_string())?;
                if let Some(handle) = speak
                    && let Err(e) = handle.send(SpeakCommand::SetAlias(alias)).await
                {
                    tracing::warn!(error = %e, "voice alias hot-reload failed");
                }
                Ok::<(), String>(())
            }
            .await;
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| {
            let outcome = rx
                .await
                .unwrap_or_else(|_| Err("dispatch cancelled".to_owned()));
            let _ = this.update(cx, |_this, cx| match outcome {
                Ok(()) => cx.push_toast(ToastKind::Success, tr!("chat_drawer_block_tts_sent")),
                Err(e) => cx.push_toast(
                    ToastKind::Error,
                    tr!("chat_drawer_block_tts_failed", error = e),
                ),
            });
        })
        .detach();
        cx.notify();
    }

    fn jump_to_latest(&mut self, cx: &mut Context<Self>) {
        self.auto_scroll = true;
        self.chat_scroll.scroll_to_bottom();
        self.unread = 0;
        self.last_seen_len = self.feed.read(cx).messages().len();
        cx.notify();
    }

    fn on_wheel(
        &mut self,
        _event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let remaining = self.chat_scroll.max_offset().y + self.chat_scroll.offset().y;
        let at_bottom = remaining <= px(AT_BOTTOM_SLACK);
        self.auto_scroll = at_bottom;
        if at_bottom {
            self.unread = 0;
            self.last_seen_len = self.feed.read(cx).messages().len();
        }
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
        let bots_ok = !self.hide_bots || !msg.is_bot;
        platform_ok && events_ok && bots_ok
    }

    fn username_color(msg: &ChatMessage, palette: &ForgePalette) -> Rgba {
        if let Some(color) = msg.author_color {
            color
        } else if !msg.username.is_empty() {
            hashed_username_color(&msg.username, palette)
        } else {
            match msg.platform {
                Platform::Twitch => palette.brand,
                Platform::YouTube => palette.random,
                Platform::Kick => palette.info,
            }
        }
    }

    fn render_header(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let viewer_count = self.home_stats.read(cx).viewers_display();
        let uptime_text = self.status.read(cx).uptime_human();

        let viewers = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(status_dot(palette.success, VIEWER_DOT))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(format!("{} {}", viewer_count, tr!("chat_viewers_unit"))),
            );

        let separator = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_faint)
            .child("·");

        let uptime = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(icon(Icon::Clock, px(12.0), palette.text_muted))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(uptime_text),
            );

        let drawer_label = if self.drawer_open {
            tr!("chat_hide_viewers")
        } else {
            tr!("chat_show_viewers")
        };
        let border = palette.border_regular;
        let drawer_btn = div()
            .id("chat-drawer-toggle")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .py(spacing(Spacing::Xxs, Density::Cozy))
            .px(spacing(Spacing::Xs, Density::Cozy))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(border)
            .cursor_pointer()
            .hover(move |s| s.border_color(border))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_drawer(cx)))
            .child(icon(Icon::LayoutSidebar, px(11.0), palette.text_secondary))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(drawer_label),
            );

        let cluster = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(viewers)
            .child(separator)
            .child(uptime)
            .child(drawer_btn);

        breadcrumb(
            vec![BreadcrumbCrumb::leaf(tr!("chat_breadcrumb_chat"))],
            palette,
        )
        .right(cluster)
    }

    fn render_filter_bar(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
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

        let surf = palette.surface_overlay;
        let text = palette.text_primary;
        let faint = palette.text_faint;
        let green = palette.success;

        let toggle_icon = if self.search_open {
            Icon::X
        } else {
            Icon::Search
        };
        let search_toggle = div()
            .id("chat-search-toggle")
            .flex()
            .items_center()
            .justify_center()
            .p(ICON_BTN_PAD)
            .rounded(ICON_BTN_RADIUS)
            .cursor_pointer()
            .hover(move |s| s.bg(surf))
            .on_click(
                cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_search(window, cx)),
            )
            .child(icon(
                toggle_icon,
                px(14.0),
                if self.search_open { text } else { faint },
            ));

        let export_btn = div()
            .id("chat-export")
            .flex()
            .items_center()
            .justify_center()
            .p(ICON_BTN_PAD)
            .rounded(ICON_BTN_RADIUS)
            .cursor_pointer()
            .hover(move |s| s.bg(surf))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.export_chat_log(cx)))
            .child(icon(Icon::Download, px(14.0), green));

        let mut search_control = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density));
        if self.search_open {
            search_control =
                search_control.child(div().w(SEARCH_FIELD_WIDTH).child(self.search_field.clone()));
        }
        let search_control = search_control.child(search_toggle).child(export_btn);

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .bg(palette.elevated)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(chips)
            .child(search_control)
    }

    fn render_chat_area(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let query = self.search_query.to_lowercase();
        let search_active = self.search_open && !query.is_empty();

        let visible: Vec<ChatMessage> = self
            .feed
            .read(cx)
            .messages()
            .iter()
            .filter(|m| self.row_visible(m))
            .cloned()
            .collect();

        let mut list = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density));

        for (idx, msg) in visible.iter().enumerate() {
            let data = ChatRow {
                id: format!("chat-row-{idx}").into(),
                timestamp: msg.timestamp.clone(),
                platform: msg.platform,
                badges: msg.badges.clone(),
                username: msg.username.clone(),
                username_color: Self::username_color(msg, palette),
                body: msg.body.clone(),
                moderated: msg.moderated,
                reply: msg.reply.clone(),
            };
            let username = msg.username.clone();
            let row = chat_row(palette, data).on_username_click(
                ("chat-username", idx),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.open_viewer(username.clone(), cx)
                }),
            );
            let dim = search_active && !msg.matches_query(&query);
            let row_el: AnyElement = if dim {
                div().opacity(0.3).child(row).into_any_element()
            } else {
                row.into_any_element()
            };
            list = list.child(row_el);
        }

        let empty = visible.is_empty();
        let empty_note = if empty {
            Some(
                div()
                    .w_full()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("chat_no_filter_matches")),
            )
        } else {
            None
        };

        let scroll = div()
            .id("chat-scroll")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .track_scroll(&self.chat_scroll)
            .on_scroll_wheel(cx.listener(Self::on_wheel))
            .child(list)
            .children(empty_note);

        let mut area = div()
            .relative()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(scroll);

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
                        .font_family(DEFAULT_MONO_FAMILY)
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
        let messages: Vec<ChatMessage> = self.feed.read(cx).messages().to_vec();
        let search = self.drawer_query.to_ascii_lowercase();
        let authors = unique_authors(&messages);
        let shown: Vec<String> = authors
            .iter()
            .filter(|u| drawer_matches(u, &search))
            .cloned()
            .collect();

        let detail = selected_summary(
            self.selected_viewer.as_deref(),
            &messages,
            &self.viewers,
            palette,
        );
        let selected_name = detail.as_ref().map(|d| d.username.clone());

        let rows: Vec<ViewerSummary> = shown
            .iter()
            .filter_map(|u| {
                synthesize_from_chat(u, &messages, palette)
                    .map(|s| enrich_with_storage(s, &self.viewers))
            })
            .collect();

        let header = self.render_drawer_header(authors.len(), shown.len(), palette, density);
        let detail_el = self.render_selected_detail(detail, palette, density, cx);
        let list_el =
            self.render_viewer_list(rows, selected_name, shown.len(), palette, density, cx);

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
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("chat_viewers_title")),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
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
            .child(self.drawer_search.clone())
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
                        .font_family(DEFAULT_BODY_FAMILY)
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
                    .font_family(DEFAULT_BODY_FAMILY)
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
                    .font_family(DEFAULT_MONO_FAMILY)
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
        let watch_color = if summary.watch_time == DASH {
            palette.text_faint
        } else {
            palette.text_primary
        };
        let grid = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .flex()
                    .gap(spacing(Spacing::Xs, density))
                    .child(stat_cell(
                        tr!("chat_stat_watch_time"),
                        summary.watch_time.clone(),
                        watch_color,
                        palette,
                        density,
                    ))
                    .child(stat_cell(
                        tr!("chat_stat_messages"),
                        summary.message_count.to_string(),
                        palette.text_primary,
                        palette,
                        density,
                    )),
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
                    .child(stat_cell(
                        tr!("chat_stat_follow"),
                        summary.follow.clone(),
                        palette.text_faint,
                        palette,
                        density,
                    )),
            );

        let actions = div()
            .flex()
            .gap(spacing(Spacing::Xs, density))
            .child(drawer_ghost_button(
                "chat-drawer-shoutout",
                Icon::Bolt,
                tr!("chat_drawer_shoutout"),
                palette,
                density,
                cx.listener(|this, _: &ClickEvent, _, cx| this.shoutout_viewer(cx)),
            ))
            .child(drawer_ghost_button(
                "chat-drawer-whisper",
                Icon::MessageCircle,
                tr!("chat_drawer_whisper"),
                palette,
                density,
                cx.listener(|this, _: &ClickEvent, window, cx| this.open_whisper(window, cx)),
            ))
            .child(self.render_drawer_menu(palette, cx));

        let whisper = self
            .whisper_open
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
            .font_family(DEFAULT_BODY_FAMILY)
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
                    .font_family(DEFAULT_BODY_FAMILY)
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
                    .font_family(DEFAULT_BODY_FAMILY)
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

    fn render_drawer_menu(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let items = vec![
            menu_item(
                "chat-drawer-menu-shoutout",
                tr!("chat_drawer_shoutout"),
                cx.listener(|this, _: &ClickEvent, _, cx| this.shoutout_viewer(cx)),
            )
            .icon(Icon::Flag)
            .into(),
            menu_item(
                "chat-drawer-menu-whisper",
                tr!("chat_drawer_whisper"),
                cx.listener(|this, _: &ClickEvent, window, cx| this.open_whisper(window, cx)),
            )
            .icon(Icon::MessageCircle)
            .into(),
            menu_item(
                "chat-drawer-menu-tts-voice",
                tr!("chat_drawer_set_tts_voice"),
                cx.listener(|_, _: &ClickEvent, _, _| {}),
            )
            .icon(Icon::Pencil)
            .disabled(true)
            .into(),
            menu_divider(),
            menu_item(
                "chat-drawer-menu-block-tts",
                tr!("chat_drawer_block_tts"),
                cx.listener(|this, _: &ClickEvent, _, cx| this.block_tts_viewer(cx)),
            )
            .color(palette.warning)
            .into(),
            menu_item(
                "chat-drawer-menu-timeout",
                tr!("chat_drawer_timeout"),
                cx.listener(|this, _: &ClickEvent, _, cx| this.timeout_viewer(cx)),
            )
            .color(palette.warning)
            .into(),
            menu_item(
                "chat-drawer-menu-ban",
                tr!("chat_drawer_ban"),
                cx.listener(|this, _: &ClickEvent, _, cx| this.ban_viewer(cx)),
            )
            .color(palette.random)
            .into(),
        ];

        menu_button(Icon::DotsVertical, self.drawer_menu_open, palette)
            .placement(MenuPlacement::TopRight)
            .items(items)
            .on_toggle(
                "chat-drawer-menu-trigger",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_drawer_menu(cx)),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_drawer_menu(cx));
            })
            .into_any_element()
    }

    fn render_viewer_list(
        &self,
        rows: Vec<ViewerSummary>,
        selected_name: Option<String>,
        shown: usize,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let header = div()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!("chat_drawer_section_active", count = shown as i64));

        let mut list = div().flex().flex_col();
        if rows.is_empty() {
            list = list.child(
                div()
                    .py(spacing(Spacing::Xs, density))
                    .px(spacing(Spacing::Sm, density))
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("chat_drawer_no_matches")),
            );
        } else {
            for summary in rows {
                let is_sel = selected_name.as_deref() == Some(summary.username.as_str());
                list = list.child(self.render_viewer_row(summary, is_sel, palette, density, cx));
            }
        }

        div()
            .id("chat-drawer-list")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .child(header)
            .child(list)
    }

    fn render_viewer_row(
        &self,
        summary: ViewerSummary,
        is_sel: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let username = summary.username.clone();
        let row_id = SharedString::from(format!("chat-drawer-row-{}", summary.username));
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
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(SharedString::from(summary.username.clone())),
            );
        if let Some(role) = summary.role {
            name_row = name_row.child(drawer_role_badge(role, BADGE_ROW, palette));
        }

        let meta = format!(
            "{} \u{b7} {} msg",
            summary.watch_time, summary.message_count
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
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(meta),
            );

        let last_seen = div()
            .flex_none()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(SharedString::from(summary.last_seen_label.clone()));

        let mut row = div()
            .id(row_id)
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .border_l(ROW_STRIPE)
            .border_color(stripe)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.select_viewer(username.clone(), cx)
            }))
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
    div()
        .flex_none()
        .size(size)
        .rounded(radius(corner))
        .bg(color)
        .flex()
        .items_center()
        .justify_center()
        .font_family(DEFAULT_BODY_FAMILY)
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(font)
        .text_color(palette.shell)
        .child(letter.to_string())
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
) -> impl IntoElement {
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
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(label.into()),
        )
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
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
    palette: &ForgePalette,
    density: Density,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let border = palette.border_regular;
    let border_hover = palette.border_input;
    let surf = palette.surface_overlay;
    let text = palette.text_secondary;
    let text_hover = palette.text_primary;
    div()
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
        .cursor_pointer()
        .hover(move |s| s.bg(surf).border_color(border_hover).text_color(text_hover))
        .on_click(handler)
        .child(icon(glyph, DRAWER_ICON, text))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(text)
                .child(label.into()),
        )
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

fn hashed_username_color(username: &str, palette: &ForgePalette) -> Rgba {
    let hash = username.bytes().fold(0u32, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(u32::from(b))
    });
    let colors = [
        palette.brand,
        palette.success,
        palette.warning,
        palette.info,
        palette.random,
        palette.bits,
    ];
    colors[(hash as usize) % colors.len()]
}

impl Render for ChatView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header = self.render_header(&palette, cx);
        let filter_bar = self.render_filter_bar(&palette, density, cx);
        let chat_area = self.render_chat_area(&palette, density, cx);
        let drawer = self
            .drawer_open
            .then(|| self.render_drawer(&palette, density, cx));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(header)
            .child(filter_bar)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(0.0))
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .child(chat_area)
                            .child(self.input.clone()),
                    )
                    .children(drawer),
            )
    }
}
