use super::PlatformFilter;
use super::viewer_actions::ViewerTarget;
use crate::app::navigable_list::ListCursor;
use crate::audience::chat_author::AuthorKey;
use crate::audience::chat_feed::ChatFeed;
use forge_components::{ActivePresentation, SearchState, tr};
use forge_types::Shared;
use gpui::{
    Context, Entity, EventEmitter, ListAlignment, ListState, Pixels, Point, Subscription, Window,
    prelude::*, px,
};
use std::collections::VecDeque;
use std::rc::Rc;

const LIST_OVERDRAW: Pixels = px(240.0);

pub(super) enum ChatTimelineEvent {
    OpenViewer(AuthorKey),
    OpenUserMenu {
        position: Point<Pixels>,
        viewer: ViewerTarget,
        message_id: String,
    },
}

pub(super) struct ChatTimeline {
    pub(super) feed: Entity<ChatFeed>,
    pub(super) bot_accounts: Shared<Vec<String>>,
    pub(super) rt_handle: tokio::runtime::Handle,
    pub(super) search: SearchState,
    pub(super) platform_filter: PlatformFilter,
    pub(super) events_only: bool,
    pub(super) hide_bots: bool,
    pub(super) visible: Rc<VecDeque<u64>>,
    pub(super) appended_through: u64,
    pub(super) feed_keys: ListCursor<u64>,
    pub(super) auto_scroll: bool,
    pub(super) unread: usize,
    pub(super) last_seen_seq: u64,
    pub(super) chat_list: ListState,
    _feed_obs: Subscription,
    _search_sub: Subscription,
}

impl EventEmitter<ChatTimelineEvent> for ChatTimeline {}

impl ChatTimeline {
    pub(super) fn new(
        feed: Entity<ChatFeed>,
        bot_accounts: Shared<Vec<String>>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = SearchState::new(cx, tr!("chat_search_placeholder"));
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);
        let feed_obs = cx.observe(&feed, Self::on_feed_changed);
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

        let mut timeline = Self {
            feed,
            bot_accounts,
            rt_handle,
            search,
            platform_filter: PlatformFilter::All,
            events_only: false,
            hide_bots: false,
            visible: Rc::new(VecDeque::new()),
            appended_through: last_seen_seq,
            feed_keys: ListCursor::reachable_by_tab(cx),
            auto_scroll: true,
            unread: 0,
            last_seen_seq,
            chat_list,
            _feed_obs: feed_obs,
            _search_sub: search_sub,
        };
        timeline.rebuild_visible(cx);
        timeline.chat_list.reset(timeline.visible.len());
        timeline
    }
}

impl Render for ChatTimeline {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        self.render_chat_area(&palette, density, cx)
    }
}
