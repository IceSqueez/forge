use super::{ChatView, PlatformFilter};
use crate::chat_drawer::drawer_matches;
use crate::chat_feed::{ChatFeed, ChatMessage};
use crate::platform_visuals::platform_color;
use forge_components::{ForgePalette, InputEvent, TextInput, hash_accent};
use forge_types::is_bot_account;
use gpui::{Context, Entity, Rgba};
use std::collections::VecDeque;
use std::rc::Rc;

impl ChatView {
    pub(super) fn on_feed_changed(&mut self, feed: Entity<ChatFeed>, cx: &mut Context<Self>) {
        let end = feed.read(cx).end_seq();
        self.sync_visible(cx);
        if self.timeline.auto_scroll {
            self.timeline.chat_list.scroll_to_end();
            self.timeline.unread = 0;
        } else {
            let arrived = usize::try_from(end.saturating_sub(self.timeline.last_seen_seq))
                .unwrap_or(usize::MAX);
            self.timeline.unread = self.timeline.unread.saturating_add(arrived);
        }
        self.timeline.last_seen_seq = end;
        self.feed_changed.send_replace(());
        self.refresh_drawer_keys(cx);
        self.absorb_viewer_messages(cx);
        cx.notify();
    }

    pub(super) fn rebuild_visible(&mut self, cx: &mut Context<Self>) {
        let feed = self.feed.read(cx);
        let visible: VecDeque<u64> = (feed.start_seq()..feed.end_seq())
            .filter(|seq| feed.get(*seq).is_some_and(|m| self.row_visible(m)))
            .collect();
        self.timeline.appended_through = feed.end_seq();
        self.timeline.visible = Rc::new(visible);
    }

    fn sync_visible(&mut self, cx: &mut Context<Self>) {
        let feed = self.feed.read(cx);
        let start = feed.start_seq();
        let end = feed.end_seq();
        let fresh: Vec<u64> = (self.timeline.appended_through.max(start)..end)
            .filter(|seq| feed.get(*seq).is_some_and(|m| self.row_visible(m)))
            .collect();
        self.timeline.appended_through = end;

        let visible = Rc::make_mut(&mut self.timeline.visible);
        let evicted = visible.iter().take_while(|seq| **seq < start).count();
        visible.drain(..evicted);
        let added = fresh.len();
        visible.extend(fresh);

        if evicted > 0 {
            self.timeline.chat_list.splice(0..evicted, 0);
        }
        if added > 0 {
            let current = self.timeline.chat_list.item_count();
            self.timeline.chat_list.splice(current..current, added);
        }
    }

    pub(super) fn reset_chat_list(&mut self, cx: &mut Context<Self>) {
        self.rebuild_visible(cx);
        self.timeline.chat_list.reset(self.timeline.visible.len());
        self.timeline.auto_scroll = true;
        self.timeline.unread = 0;
        self.timeline.last_seen_seq = self.feed.read(cx).end_seq();
    }

    pub(super) fn refresh_drawer_keys(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn on_search_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if self.timeline.search.on_changed(event) {
            cx.notify();
        }
    }

    pub(super) fn set_platform_filter(&mut self, filter: PlatformFilter, cx: &mut Context<Self>) {
        self.timeline.platform_filter = filter;
        self.reset_chat_list(cx);
        cx.notify();
    }

    pub(super) fn toggle_events(&mut self, cx: &mut Context<Self>) {
        self.timeline.events_only = !self.timeline.events_only;
        self.reset_chat_list(cx);
        cx.notify();
    }

    pub(super) fn toggle_hide_bots(&mut self, cx: &mut Context<Self>) {
        self.timeline.hide_bots = !self.timeline.hide_bots;
        self.reset_chat_list(cx);
        cx.notify();
    }

    pub(super) fn jump_to_latest(&mut self, cx: &mut Context<Self>) {
        self.timeline.feed_keys.clear();
        self.timeline.auto_scroll = true;
        self.timeline.chat_list.scroll_to_end();
        self.timeline.unread = 0;
        self.timeline.last_seen_seq = self.feed.read(cx).end_seq();
        cx.notify();
    }

    pub(super) fn row_visible(&self, msg: &ChatMessage) -> bool {
        let platform_ok = match self.timeline.platform_filter {
            PlatformFilter::All => true,
            PlatformFilter::Single(p) => msg.platform == p,
        };
        let events_ok = !self.timeline.events_only || msg.is_event;
        let bots_ok = !self.timeline.hide_bots
            || !(msg.is_bot || is_bot_account(&msg.username, &self.services.bot_accounts.load()));
        platform_ok && events_ok && bots_ok
    }

    pub(super) fn username_color(msg: &ChatMessage, palette: &ForgePalette) -> Rgba {
        if let Some(color) = msg.author_color {
            color
        } else if !msg.username.is_empty() {
            hash_accent(&msg.username, palette)
        } else {
            platform_color(msg.platform, palette)
        }
    }
}
