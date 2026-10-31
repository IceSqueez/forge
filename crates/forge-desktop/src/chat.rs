mod ban_labels;
mod banned_panel;
mod banned_tab;
mod composer;
mod drawer_pane;
mod drawer_state;
mod feed_keys;
mod feed_pane;
mod log_export;
mod moderation;
mod platform_gate;
mod reply_whisper;
mod send_plan;
#[cfg(test)]
mod tests;
mod user_menu;
mod viewer_actions;
mod viewer_count;
mod viewer_detail;
mod viewer_follow;
mod viewer_history;
mod viewer_tts;
mod visible_rows;

use self::{
    banned_tab::BannedHost, viewer_actions::ViewerTarget, viewer_count::ChatViewerCount,
    viewer_follow::FollowLookups, viewer_history::ViewerHistory, viewer_tts::TtsVoiceHost,
};
use crate::chat_author::AuthorKey;
use crate::chat_drawer::ViewerDirectory;
use crate::chat_feed::ChatFeed;
use crate::chat_viewer_messages::ViewerMessages;
use crate::home_stats::HomeStats;
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::navigable_list::ListCursor;
pub(crate) use composer::ChatComposer;
use forge_components::drive_overlay_focus;
use forge_components::{
    ActivePresentation, BreadcrumbCrumb, Confirm, ForgePalette, SearchState, Spacing, TextInput,
    page_frame, spacing, tr,
};
use forge_runtime::ActionEngineHandle;
use forge_speak_queue::SpeakQueueHandle;
use forge_storage::{ChatHistoryRepo, ViewerRepo, VoiceAliasRepo};
use forge_types::{PlatformId, Shared};
use gpui::{
    Context, Entity, ListAlignment, ListState, Pixels, Point, Subscription, Window, div,
    prelude::*, px,
};
use gpui::{FocusHandle, UniformListScrollHandle};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

const LIST_OVERDRAW: Pixels = px(240.0);

const DRAWER_WIDTH: Pixels = px(320.0);
const VIEWER_REFRESH: Duration = Duration::from_secs(15);
const INFINITY_GLYPH: &str = "\u{221e}";
const DRAWER_TIMEOUT_SECONDS: i64 = CTX_TIMEOUT_10M;
const CTX_TIMEOUT_10M: i64 = 600;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlatformFilter {
    All,
    Single(PlatformId),
}

#[derive(Clone)]
struct UserMenuTarget {
    position: Point<Pixels>,
    viewer: ViewerTarget,
    message_id: Option<String>,
}

#[derive(Clone)]
struct ReplyTarget {
    username: String,
    message_id: String,
}

struct DrawerResizeDrag;

pub(crate) struct ChatView {
    menu_focus: FocusHandle,
    menu_focus_restore: Option<FocusHandle>,
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
    viewer_messages: Entity<ViewerMessages>,
    _viewer_messages_obs: Subscription,
    viewer_history: Option<ViewerHistoryHost>,
    tts_voice: Option<TtsVoiceHost>,
    pending_ban: Confirm<ViewerTarget>,
    feed_changed: tokio::sync::watch::Sender<()>,
    viewers: ViewerDirectory,
    drawer_keys: Vec<AuthorKey>,
    drawer_list_keys: ListCursor<AuthorKey>,
    feed_keys: ListCursor<u64>,
    drawer_list_scroll: UniformListScrollHandle,
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

pub(crate) fn platform_display_name(platform: PlatformId) -> &'static str {
    match platform {
        PlatformId::Twitch => "Twitch",
        PlatformId::YouTube => "YouTube",
        PlatformId::Kick => "Kick",
    }
}

impl ChatView {
    #[expect(
        clippy::too_many_arguments,
        reason = "constructor receives each shared handle it wires"
    )]
    pub(crate) fn new(
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

        let viewer_messages = cx.new(|_| ViewerMessages::default());
        let viewer_messages_obs = cx.observe(&viewer_messages, |_, _, cx| cx.notify());
        let (feed_changed, feed_changes) = tokio::sync::watch::channel(());
        Self::spawn_viewer_refresh(
            viewer_repo,
            Arc::clone(&chat_history_repo),
            rt_handle.clone(),
            feed_changes,
            cx,
        );

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
            viewer_messages,
            _viewer_messages_obs: viewer_messages_obs,
            viewer_history: None,
            tts_voice: None,
            pending_ban: Confirm::default(),
            feed_changed,
            viewers: ViewerDirectory::default(),
            drawer_keys: Vec::new(),
            drawer_list_keys: ListCursor::new(cx),
            feed_keys: ListCursor::reachable_by_tab(cx),
            drawer_list_scroll: UniformListScrollHandle::new(),
            whisper_open: false,
            whisper_input,
            reply_target: None,
            reply_input,
            user_menu: None,
            menu_focus: cx.focus_handle(),
            menu_focus_restore: None,
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
        this.absorb_viewer_messages(cx);
        this
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drive_overlay_focus(
            self.user_menu.is_some(),
            &self.menu_focus,
            &mut self.menu_focus_restore,
            window,
            cx,
        );
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
        let ban_confirm = self.render_ban_confirm(&palette, cx);
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
            .children(ban_confirm)
            .children(self.viewer_history.as_ref().map(|host| host.view.clone()))
            .children(self.tts_voice.as_ref().map(|host| host.view.clone()))
    }
}
