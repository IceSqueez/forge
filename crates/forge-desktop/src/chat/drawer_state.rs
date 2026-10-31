use super::viewer_actions::ViewerTarget;
use super::viewer_history::{ViewerHistory, ViewerHistoryDismissed};
use super::{ChatView, ViewerHistoryHost};
use crate::chat_author::AuthorKey;
use crate::chat_drawer::{current_name, displayed_viewer};
use forge_components::{InputEvent, TextInput};
use gpui::{App, Context, Entity, Pixels, Point, prelude::*};
use std::sync::Arc;

impl ChatView {
    pub(super) fn open_viewer_history(
        &mut self,
        key: AuthorKey,
        viewer_name: String,
        cx: &mut Context<Self>,
    ) {
        let feed = self.feed.clone();
        let messages = self.viewer_messages.clone();
        let repo = Arc::clone(&self.chat_history_repo);
        let rt_handle = self.rt_handle.clone();
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

    pub(super) fn open_viewer(&mut self, key: AuthorKey, cx: &mut Context<Self>) {
        self.selected_viewer = Some(key);
        self.request_selected_follow(cx);
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

    pub(super) fn select_viewer(&mut self, key: AuthorKey, cx: &mut Context<Self>) {
        self.selected_viewer = Some(key);
        self.request_selected_follow(cx);
        cx.notify();
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
