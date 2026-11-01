mod ban_labels;
mod banned_panel;
mod banned_tab;
mod composer;
mod drawer_pane;
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
mod viewer_drawer;
mod viewer_follow;
mod viewer_history;
mod viewer_tts;
mod visible_rows;
mod whisper;

use self::moderation::{ChatModeration, ChatModerationEvent};
pub(crate) use self::state::ChatServices;
use self::state::{ChatSubscriptions, ReplyMenuState};
use self::timeline::{ChatTimeline, ChatTimelineEvent};
use self::viewer_drawer::{DrawerServices, ViewerDrawer, ViewerDrawerEvent};
use self::{banned_tab::BannedHost, viewer_actions::ViewerTarget, viewer_count::ChatViewerCount};
use crate::audience::chat_feed::ChatFeed;
use crate::audience::home_stats::HomeStats;
use crate::integrations::integration_lifecycle::IntegrationLifecycle;
pub(crate) use composer::ChatComposer;
use forge_components::drive_overlay_focus;
use forge_components::{
    ActivePresentation, BreadcrumbCrumb, ForgePalette, Spacing, TextInput, page_frame, spacing, tr,
};
use forge_host::integrations::BuiltinRegistry;
use forge_storage::ViewerRepo;
use forge_types::PlatformId;
use gpui::{App, Context, Entity, Pixels, Point, Window, div, prelude::*, px};
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
    viewer_count: Entity<ChatViewerCount>,
    composer: Entity<ChatComposer>,
    lifecycle: Option<Entity<IntegrationLifecycle>>,
    banned: BannedHost,
    services: ChatServices,
    timeline: Entity<ChatTimeline>,
    drawer: Entity<ViewerDrawer>,
    moderation: Entity<ChatModeration>,
    reply_menu: ReplyMenuState,
    subscriptions: ChatSubscriptions,
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
        let reply_input = cx.new(|cx| TextInput::new(tr!("chat_reply_placeholder"), cx));
        let reply_sub = cx.subscribe(&reply_input, Self::on_reply_event);
        let composer = cx.new(|cx| {
            ChatComposer::new(home_stats.clone(), services.rt_handle.clone(), palette, cx)
        });
        let viewer_count = cx.new(|cx| ChatViewerCount::new(home_stats, cx));

        let moderation = cx.new(|_| {
            ChatModeration::new(services.action_engine.clone(), services.rt_handle.clone())
        });
        let moderation_sub = cx.subscribe(&moderation, Self::on_moderation_event);

        let drawer_services = DrawerServices {
            rt_handle: services.rt_handle.clone(),
            chat_history_repo: Arc::clone(&services.chat_history_repo),
            voice_alias_repo: Arc::clone(&services.voice_alias_repo),
            speak: services.speak.clone(),
        };
        let drawer_moderation = moderation.clone();
        let drawer_feed = feed.clone();
        let drawer = cx.new(|cx| {
            ViewerDrawer::new(
                drawer_feed,
                viewer_repo,
                drawer_services,
                drawer_moderation,
                cx,
            )
        });
        let drawer_sub = cx.subscribe(&drawer, Self::on_drawer_event);

        let bot_accounts = services.bot_accounts.clone();
        let rt_handle = services.rt_handle.clone();
        let timeline = cx.new(|cx| ChatTimeline::new(feed, bot_accounts, rt_handle, cx));
        let timeline_sub = cx.subscribe(&timeline, Self::on_timeline_event);

        Self {
            viewer_count,
            composer,
            lifecycle: None,
            banned: BannedHost::default(),
            services,
            timeline,
            drawer,
            moderation,
            reply_menu: ReplyMenuState {
                reply_target: None,
                reply_input,
                user_menu: None,
                menu_focus: cx.focus_handle(),
                menu_focus_restore: None,
            },
            subscriptions: ChatSubscriptions {
                _timeline_sub: timeline_sub,
                _drawer_sub: drawer_sub,
                _moderation_sub: moderation_sub,
                _reply_sub: reply_sub,
                _lifecycle_obs: None,
            },
        }
    }

    #[must_use]
    pub(crate) fn with_follow_lookups(
        self,
        builtins: BuiltinRegistry,
        cx: &mut Context<Self>,
    ) -> Self {
        self.drawer
            .update(cx, |drawer, _| drawer.set_follow_lookups(builtins));
        self
    }
}

impl ChatView {
    fn on_timeline_event(
        &mut self,
        _timeline: Entity<ChatTimeline>,
        event: &ChatTimelineEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ChatTimelineEvent::OpenViewer(key) => {
                self.drawer
                    .update(cx, |drawer, cx| drawer.open_viewer(key.clone(), cx));
            }
            ChatTimelineEvent::OpenUserMenu {
                position,
                viewer,
                message_id,
            } => self.open_user_menu(*position, viewer.clone(), Some(message_id.clone()), cx),
        }
    }

    fn on_drawer_event(
        &mut self,
        _drawer: Entity<ViewerDrawer>,
        event: &ViewerDrawerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ViewerDrawerEvent::OpenUserMenu {
                position,
                viewer,
                message_id,
            } => self.open_user_menu(*position, viewer.clone(), message_id.clone(), cx),
        }
    }

    fn on_moderation_event(
        &mut self,
        _moderation: Entity<ChatModeration>,
        event: &ChatModerationEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ChatModerationEvent::Moderated(platform) => self.note_moderated(*platform, cx),
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
        let user_menu = self.render_user_menu(&palette, cx);
        let ban_confirm = self.moderation.update(cx, |moderation, cx| {
            moderation.render_ban_confirm(&palette, cx)
        });
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
            .child(self.drawer.clone());

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

        let drawer = self.drawer.read(cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(user_menu)
            .children(ban_confirm)
            .children(drawer.viewer_history.as_ref().map(|host| host.view.clone()))
            .children(drawer.tts_voice.as_ref().map(|host| host.view.clone()))
    }
}
