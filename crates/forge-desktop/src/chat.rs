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
mod state;
#[cfg(test)]
mod tests;
mod timeline;
mod user_menu;
mod viewer_actions;
mod viewer_count;
mod viewer_detail;
mod viewer_follow;
mod viewer_history;
mod viewer_tts;
mod visible_rows;

pub(crate) use self::state::ChatServices;
use self::state::{ChatSubscriptions, DrawerState, ReplyMenuState};
use self::timeline::{ChatTimeline, ChatTimelineEvent};
use self::{
    banned_tab::BannedHost, viewer_actions::ViewerTarget, viewer_count::ChatViewerCount,
    viewer_follow::FollowLookups, viewer_history::ViewerHistory,
};
use crate::chat_drawer::{ViewerDirectory, drawer_matches};
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
use forge_storage::ViewerRepo;
use forge_types::PlatformId;
use gpui::UniformListScrollHandle;
use gpui::{App, Context, Entity, Pixels, Point, Subscription, Window, div, prelude::*, px};
use std::sync::Arc;
use std::time::Duration;

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
    feed: Entity<ChatFeed>,
    viewer_count: Entity<ChatViewerCount>,
    composer: Entity<ChatComposer>,
    feed_changed: tokio::sync::watch::Sender<()>,
    lifecycle: Option<Entity<IntegrationLifecycle>>,
    banned: BannedHost,
    services: ChatServices,
    timeline: Entity<ChatTimeline>,
    drawer: DrawerState,
    reply_menu: ReplyMenuState,
    subscriptions: ChatSubscriptions,
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
    pub(crate) fn new(
        feed: Entity<ChatFeed>,
        home_stats: Entity<HomeStats>,
        viewer_repo: Arc<dyn ViewerRepo>,
        services: ChatServices,
        palette: ForgePalette,
        cx: &mut Context<Self>,
    ) -> Self {
        let drawer_search =
            SearchState::on_surface(cx, palette, tr!("chat_drawer_search_placeholder"));
        let whisper_input = cx.new(|cx| {
            TextInput::new(tr!("chat_drawer_whisper_placeholder"), cx).with_palette(palette)
        });
        let reply_input =
            cx.new(|cx| TextInput::new(tr!("chat_reply_placeholder"), cx).with_palette(palette));

        let feed_obs = cx.observe(&feed, Self::on_feed_changed);
        let composer = cx.new(|cx| {
            ChatComposer::new(home_stats.clone(), services.rt_handle.clone(), palette, cx)
        });
        let viewer_count = cx.new(|cx| ChatViewerCount::new(home_stats, cx));
        let drawer_search_sub = cx.subscribe(drawer_search.field(), Self::on_drawer_search_event);
        let whisper_sub = cx.subscribe(&whisper_input, Self::on_whisper_event);
        let reply_sub = cx.subscribe(&reply_input, Self::on_reply_event);

        let bot_accounts = services.bot_accounts.clone();
        let rt_handle = services.rt_handle.clone();
        let timeline =
            cx.new(|cx| ChatTimeline::new(feed.clone(), bot_accounts, rt_handle, palette, cx));
        let timeline_sub = cx.subscribe(&timeline, Self::on_timeline_event);

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

        let mut this = Self {
            feed,
            viewer_count,
            composer,
            feed_changed,
            lifecycle: None,
            banned: BannedHost::default(),
            services,
            timeline,
            drawer: DrawerState {
                drawer_width: DRAWER_WIDTH,
                drawer_search,
                drawer_menu_open: None,
                selected_viewer: None,
                follows: FollowLookups::default(),
                viewer_messages,
                viewer_history: None,
                tts_voice: None,
                pending_ban: Confirm::default(),
                viewers: ViewerDirectory::default(),
                drawer_keys: Vec::new(),
                drawer_list_keys: ListCursor::new(cx),
                drawer_list_scroll: UniformListScrollHandle::new(),
                whisper_open: false,
                whisper_input,
            },
            reply_menu: ReplyMenuState {
                reply_target: None,
                reply_input,
                user_menu: None,
                menu_focus: cx.focus_handle(),
                menu_focus_restore: None,
            },
            subscriptions: ChatSubscriptions {
                _feed_obs: feed_obs,
                _timeline_sub: timeline_sub,
                _drawer_search_sub: drawer_search_sub,
                _whisper_sub: whisper_sub,
                _reply_sub: reply_sub,
                _viewer_messages_obs: viewer_messages_obs,
                _lifecycle_obs: None,
            },
        };
        this.refresh_drawer_keys(cx);
        this.absorb_viewer_messages(cx);
        this
    }
}

impl ChatView {
    fn on_feed_changed(&mut self, _feed: Entity<ChatFeed>, cx: &mut Context<Self>) {
        self.feed_changed.send_replace(());
        self.refresh_drawer_keys(cx);
        self.absorb_viewer_messages(cx);
        cx.notify();
    }

    fn refresh_drawer_keys(&mut self, cx: &mut Context<Self>) {
        let search = self.drawer.drawer_search.query();
        self.drawer.drawer_keys = self
            .feed
            .read(cx)
            .authors()
            .newest_first()
            .filter(|(_, activity)| drawer_matches(&activity.name, search))
            .map(|(key, _)| key.clone())
            .collect();
    }

    fn on_timeline_event(
        &mut self,
        _timeline: Entity<ChatTimeline>,
        event: &ChatTimelineEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ChatTimelineEvent::OpenViewer(key) => self.open_viewer(key.clone(), cx),
            ChatTimelineEvent::OpenUserMenu {
                position,
                viewer,
                message_id,
            } => self.open_user_menu(*position, viewer.clone(), Some(message_id.clone()), cx),
        }
    }

    fn offered_filters(&self, cx: &App) -> Vec<PlatformFilter> {
        [
            PlatformFilter::All,
            PlatformFilter::Single(PlatformId::Twitch),
            PlatformFilter::Single(PlatformId::YouTube),
            PlatformFilter::Single(PlatformId::Kick),
        ]
        .into_iter()
        .filter(|filter| self.filter_offered(*filter, cx))
        .collect()
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drive_overlay_focus(
            self.reply_menu.user_menu.is_some(),
            &self.reply_menu.menu_focus,
            &mut self.reply_menu.menu_focus_restore,
            window,
            cx,
        );
        let palette = cx.palette();
        let density = cx.density();

        let header_right = self.viewer_count.clone();
        let tabs = self.render_tabs(&palette, cx);
        let banned_panel = self.banned.panel().cloned();
        let feed_tab = banned_panel.is_none();
        let offered = self.offered_filters(cx);
        let filter_left = feed_tab.then(|| {
            self.timeline.update(cx, |timeline, cx| {
                timeline.render_filter_left(&offered, &palette, density, cx)
            })
        });
        let filter_right = feed_tab.then(|| {
            self.timeline.update(cx, |timeline, cx| {
                timeline.render_filter_right(&palette, cx)
            })
        });
        let drawer = self.render_drawer(&palette, density, cx);
        let user_menu = self.render_user_menu(&palette, cx);
        let ban_confirm = self.render_ban_confirm(&palette, cx);
        let reply_compose = self
            .reply_menu
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
                .child(self.timeline.clone())
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
            .children(
                self.drawer
                    .viewer_history
                    .as_ref()
                    .map(|host| host.view.clone()),
            )
            .children(self.drawer.tts_voice.as_ref().map(|host| host.view.clone()))
    }
}
