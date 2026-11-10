use super::DRAWER_WIDTH;
use super::moderation::ChatModeration;
use super::viewer_actions::ViewerTarget;
use super::viewer_follow::FollowLookups;
use super::viewer_history::{ViewerHistory, ViewerHistoryDismissed};
use super::viewer_tts::TtsVoiceHost;
use crate::app::navigable_list::ListCursor;
use crate::audience::chat_author::AuthorKey;
use crate::audience::chat_drawer::{
    ViewerDirectory, current_name, displayed_viewer, drawer_matches,
};
use crate::audience::chat_feed::ChatFeed;
use crate::audience::chat_viewer_messages::ViewerMessages;
use crate::integrations::integration_lifecycle::IntegrationLifecycle;
use forge_components::{ActivePresentation, InputEvent, SearchState, TextInput, tr};
use forge_host::integrations::BuiltinRegistry;
use forge_speak_queue::SpeakQueueHandle;
use forge_storage::{ChatHistoryRepo, ViewerRepo, VoiceAliasRepo};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Pixels, Point, Subscription,
    UniformListScrollHandle, Window, prelude::*,
};
use std::sync::Arc;

pub(super) struct DrawerServices {
    pub(super) rt_handle: tokio::runtime::Handle,
    pub(super) chat_history_repo: Arc<dyn ChatHistoryRepo>,
    pub(super) voice_alias_repo: Arc<dyn VoiceAliasRepo>,
    pub(super) speak: Option<SpeakQueueHandle>,
}

pub(super) enum ViewerDrawerEvent {
    OpenUserMenu {
        position: Point<Pixels>,
        viewer: ViewerTarget,
        message_id: Option<String>,
    },
}

pub(super) struct ViewerHistoryHost {
    pub(super) view: Entity<ViewerHistory>,
    _dismissed: Subscription,
}

pub(super) struct ViewerDrawer {
    pub(super) feed: Entity<ChatFeed>,
    pub(super) moderation: Entity<ChatModeration>,
    pub(super) services: DrawerServices,
    pub(super) lifecycle: Option<Entity<IntegrationLifecycle>>,
    pub(super) drawer_width: Pixels,
    pub(super) drawer_search: SearchState,
    pub(super) drawer_menu_open: Option<Point<Pixels>>,
    pub(super) selected_viewer: Option<AuthorKey>,
    pub(super) follows: FollowLookups,
    pub(super) viewer_messages: Entity<ViewerMessages>,
    pub(super) viewer_history: Option<ViewerHistoryHost>,
    pub(super) tts_voice: Option<TtsVoiceHost>,
    pub(super) viewers: ViewerDirectory,
    pub(super) drawer_keys: Vec<AuthorKey>,
    pub(super) drawer_list_keys: ListCursor<AuthorKey>,
    pub(super) drawer_list_scroll: UniformListScrollHandle,
    pub(super) whisper_open: bool,
    pub(super) whisper_input: Entity<TextInput>,
    pub(super) feed_changed: tokio::sync::watch::Sender<()>,
    _feed_obs: Subscription,
    _search_sub: Subscription,
    _whisper_sub: Subscription,
    _viewer_messages_obs: Subscription,
}

impl EventEmitter<ViewerDrawerEvent> for ViewerDrawer {}

impl ViewerDrawer {
    pub(super) fn new(
        feed: Entity<ChatFeed>,
        viewer_repo: Arc<dyn ViewerRepo>,
        services: DrawerServices,
        moderation: Entity<ChatModeration>,
        cx: &mut Context<Self>,
    ) -> Self {
        let drawer_search = SearchState::on_surface(cx, tr!("chat_drawer_search_placeholder"));
        let whisper_input = cx.new(|cx| TextInput::new(tr!("chat_drawer_whisper_placeholder"), cx));
        let feed_obs = cx.observe(&feed, Self::on_feed_changed);
        let search_sub = cx.subscribe(drawer_search.field(), Self::on_drawer_search_event);
        let whisper_sub = cx.subscribe(&whisper_input, Self::on_whisper_event);
        let viewer_messages = cx.new(|_| ViewerMessages::default());
        let viewer_messages_obs = cx.observe(&viewer_messages, |_, _, cx| cx.notify());
        let (feed_changed, feed_changes) = tokio::sync::watch::channel(());
        Self::spawn_viewer_refresh(
            viewer_repo,
            Arc::clone(&services.chat_history_repo),
            services.rt_handle.clone(),
            feed_changes,
            cx,
        );

        let mut drawer = Self {
            feed,
            moderation,
            services,
            lifecycle: None,
            drawer_width: DRAWER_WIDTH,
            drawer_search,
            drawer_menu_open: None,
            selected_viewer: None,
            follows: FollowLookups::default(),
            viewer_messages,
            viewer_history: None,
            tts_voice: None,
            viewers: ViewerDirectory::default(),
            drawer_keys: Vec::new(),
            drawer_list_keys: ListCursor::new(cx),
            drawer_list_scroll: UniformListScrollHandle::new(),
            whisper_open: false,
            whisper_input,
            feed_changed,
            _feed_obs: feed_obs,
            _search_sub: search_sub,
            _whisper_sub: whisper_sub,
            _viewer_messages_obs: viewer_messages_obs,
        };
        drawer.refresh_drawer_keys(cx);
        drawer.absorb_viewer_messages(cx);
        drawer
    }

    pub(super) fn set_follow_lookups(&mut self, builtins: BuiltinRegistry) {
        self.follows = FollowLookups::new(builtins);
    }

    pub(super) fn set_lifecycle(&mut self, lifecycle: Entity<IntegrationLifecycle>) {
        self.lifecycle = Some(lifecycle);
    }

    pub(super) fn list_focus(&self) -> FocusHandle {
        self.drawer_list_keys.focus().clone()
    }

    fn on_feed_changed(&mut self, _feed: Entity<ChatFeed>, cx: &mut Context<Self>) {
        self.feed_changed.send_replace(());
        self.refresh_drawer_keys(cx);
        self.absorb_viewer_messages(cx);
        cx.notify();
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

    pub(super) fn shoutout_viewer(&mut self, target: ViewerTarget, cx: &mut Context<Self>) {
        self.drawer_menu_open = None;
        self.moderation
            .update(cx, |moderation, cx| moderation.shoutout_viewer(&target, cx));
        cx.notify();
    }

    pub(super) fn timeout_viewer(&mut self, target: ViewerTarget, cx: &mut Context<Self>) {
        self.drawer_menu_open = None;
        self.moderation
            .update(cx, |moderation, cx| moderation.timeout_viewer(&target, cx));
        cx.notify();
    }

    pub(super) fn request_ban(&mut self, target: ViewerTarget, cx: &mut Context<Self>) {
        self.drawer_menu_open = None;
        self.moderation
            .update(cx, |moderation, cx| moderation.request_ban(target, cx));
        cx.notify();
    }

    pub(super) fn open_viewer_history(
        &mut self,
        key: AuthorKey,
        viewer_name: String,
        cx: &mut Context<Self>,
    ) {
        let feed = self.feed.clone();
        let messages = self.viewer_messages.clone();
        let repo = Arc::clone(&self.services.chat_history_repo);
        let rt_handle = self.services.rt_handle.clone();
        let held = key.clone();
        self.viewer_messages
            .update(cx, |messages, _| messages.hold(held));
        let view = cx.new(|cx| {
            ViewerHistory::new(key, feed, messages, repo, rt_handle, cx).titled(viewer_name)
        });
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
        self.viewer_messages
            .update(cx, |messages, _| messages.release());
        cx.notify();
    }

    pub(super) fn set_drawer_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        if self.drawer_width != width {
            self.drawer_width = width;
            cx.notify();
        }
    }

    pub(super) fn point_at_viewer(&mut self, key: AuthorKey, cx: &mut Context<Self>) {
        self.drawer_list_keys.move_to(key.clone());
        if selection_changes(self.selected_viewer.as_ref(), &key) {
            self.reset_whisper(cx);
        }
        self.selected_viewer = Some(key);
        self.request_selected_follow(cx);
    }

    pub(super) fn open_viewer(&mut self, key: AuthorKey, cx: &mut Context<Self>) {
        self.point_at_viewer(key, cx);
        cx.notify();
    }

    pub(super) fn on_drawer_search_event(
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

    pub(super) fn displayed_target(&self, cx: &App) -> Option<ViewerTarget> {
        let authors = self.feed.read(cx).authors();
        let key = displayed_viewer(self.selected_viewer.as_ref(), authors)?;
        let name = current_name(&key, authors, &self.viewers)?;
        Some(ViewerTarget::new(&key, name.to_string()))
    }

    pub(super) fn toggle_drawer_menu(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.drawer_menu_open = if self.drawer_menu_open.is_some() {
            None
        } else {
            Some(position)
        };
        cx.notify();
    }

    pub(super) fn close_drawer_menu(&mut self, cx: &mut Context<Self>) {
        if self.drawer_menu_open.is_some() {
            self.drawer_menu_open = None;
            cx.notify();
        }
    }
}

impl Render for ViewerDrawer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        self.render_drawer(&palette, density, cx)
    }
}

pub(super) fn selection_changes(current: Option<&AuthorKey>, next: &AuthorKey) -> bool {
    current != Some(next)
}
