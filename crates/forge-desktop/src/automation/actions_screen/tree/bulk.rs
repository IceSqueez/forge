use super::*;
use forge_components::{ConfirmTone, confirm_modal, icon_button, selection_bar};

fn count_arg(count: usize) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

impl ActionTree {
    fn marked_in_state(&self, enabled: bool) -> Vec<ActionId> {
        self.marked
            .ids()
            .iter()
            .copied()
            .filter(|id| {
                self.find(*id)
                    .is_some_and(|action| action.enabled == enabled)
            })
            .collect()
    }

    pub(super) fn bulk_set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.row_open = None;
        for id in self.marked_in_state(!enabled) {
            cx.emit(ActionTreeEvent::Write(id, ActionWrite::SetEnabled(enabled)));
        }
        cx.notify();
    }

    pub(super) fn request_bulk_delete(&mut self, cx: &mut Context<Self>) {
        if !self.marked.is_bulk() {
            return;
        }
        self.row_open = None;
        self.pending_bulk_delete.request(self.marked.ids().to_vec());
        cx.emit(ActionTreeEvent::DeletePromptChanged);
        cx.notify();
    }

    pub(super) fn cancel_bulk_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_bulk_delete.cancel();
        cx.emit(ActionTreeEvent::DeletePromptChanged);
        cx.notify();
    }

    pub(super) fn confirm_bulk_delete(&mut self, cx: &mut Context<Self>) {
        let Some(ids) = self.pending_bulk_delete.take() else {
            return;
        };
        let ids: Vec<ActionId> = ids
            .into_iter()
            .filter(|id| self.find(*id).is_some())
            .collect();
        self.clear_marked(cx);
        cx.emit(ActionTreeEvent::DeletePromptChanged);
        if !ids.is_empty() {
            cx.emit(ActionTreeEvent::ArchiveMany(ids));
        }
        cx.notify();
    }

    pub(super) fn render_bulk_bar(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.marked.is_bulk() {
            return None;
        }
        let density = cx.density();
        let any_disabled = !self.marked_in_state(false).is_empty();
        let any_enabled = !self.marked_in_state(true).is_empty();
        let bar = selection_bar(
            tr!("bulk_selected_count", count = count_arg(self.marked.len())),
            palette,
        )
        .density(density)
        .action(
            icon_button(Icon::CircleCheckFilled, palette)
                .tooltip(tr!(
                    "bulk_enable_selected",
                    count = count_arg(self.marked_in_state(false).len())
                ))
                .disabled(!any_disabled)
                .on_click(
                    "actions-bulk-enable",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.bulk_set_enabled(true, cx)),
                ),
        )
        .action(
            icon_button(Icon::Circle, palette)
                .tooltip(tr!(
                    "bulk_disable_selected",
                    count = count_arg(self.marked_in_state(true).len())
                ))
                .disabled(!any_enabled)
                .on_click(
                    "actions-bulk-disable",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.bulk_set_enabled(false, cx)),
                ),
        )
        .action(
            icon_button(Icon::Eraser, palette)
                .tooltip(tr!(
                    "bulk_delete_selected",
                    count = count_arg(self.marked.len())
                ))
                .ink(palette.danger)
                .on_click(
                    "actions-bulk-delete",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.request_bulk_delete(cx)),
                ),
        )
        .clear(
            icon_button(Icon::X, palette)
                .tooltip(tr!("bulk_clear"))
                .on_click(
                    "actions-bulk-clear",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.clear_marked(cx)),
                ),
        );
        Some(bar.into_any_element())
    }

    pub(super) fn render_bulk_delete_confirm(
        &self,
        count: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let card = confirm_modal(
            tr!("bulk_delete_actions_title", count = count_arg(count)),
            tr!("bulk_delete_actions_body"),
            ConfirmTone::Destructive,
            palette,
        )
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "actions-bulk-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_bulk_delete(cx)),
        )
        .on_confirm(
            "actions-bulk-delete-confirm",
            tr!("bulk_delete_selected", count = count_arg(self.marked.len())),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_bulk_delete(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-bulk-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_bulk_delete(cx));
            })
            .into_any_element()
    }
}
