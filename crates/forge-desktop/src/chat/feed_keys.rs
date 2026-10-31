use super::ChatView;
use super::viewer_actions::ViewerTarget;
use crate::actions::{LIST_CONTEXT, ListActivate, ListOpenMenu, ListSelectNext, ListSelectPrev};
use crate::navigable_list::ListStep;
use forge_components::{ForgePalette, with_focus_ring};
use gpui::{Context, InteractiveElement};

impl ChatView {
    pub(super) fn feed_keys_on<E: InteractiveElement>(
        &self,
        area: E,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> E {
        let keyed = area
            .track_focus(self.feed_keys.focus())
            .key_context(LIST_CONTEXT)
            .on_action(
                cx.listener(|this, _: &ListSelectPrev, _, cx| this.step_feed(ListStep::Prev, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ListSelectNext, _, cx| this.step_feed(ListStep::Next, cx)),
            )
            .on_action(cx.listener(|this, _: &ListActivate, _, cx| this.activate_feed_row(cx)))
            .on_action(cx.listener(|this, _: &ListOpenMenu, _, cx| this.open_feed_row_menu(cx)));
        with_focus_ring(keyed, palette.border_active)
    }

    fn visible_seqs(&self) -> Vec<u64> {
        self.visible.iter().copied().collect()
    }

    fn step_feed(&mut self, step: ListStep, cx: &mut Context<Self>) {
        let rows = self.visible_seqs();
        let newest = rows.last().copied();
        let resume_from = newest.as_ref().filter(|_| step == ListStep::Next);
        let Some(next) = self.feed_keys.step(&rows, resume_from, step) else {
            return;
        };
        self.auto_scroll = false;
        self.chat_list.scroll_to_reveal_item(next);
        cx.notify();
    }

    fn activate_feed_row(&mut self, cx: &mut Context<Self>) {
        let Some(seq) = self.feed_keys.activation_target(&self.visible_seqs()) else {
            return;
        };
        let author = self.feed.read(cx).get(seq).and_then(|msg| msg.author_key());
        if let Some(author) = author {
            self.open_viewer(author, cx);
        }
    }

    fn open_feed_row_menu(&mut self, cx: &mut Context<Self>) {
        let Some(seq) = self.feed_keys.activation_target(&self.visible_seqs()) else {
            return;
        };
        let Some(msg) = self.feed.read(cx).get(seq) else {
            return;
        };
        let Some(author) = msg.author_key() else {
            return;
        };
        let viewer = ViewerTarget::new(&author, msg.username.to_string());
        let message_id = msg.id.to_string();
        let position = self
            .feed_keys
            .row_bounds()
            .map(|row| row.bottom_left())
            .unwrap_or_default();
        self.open_user_menu(position, viewer, Some(message_id), cx);
    }
}
