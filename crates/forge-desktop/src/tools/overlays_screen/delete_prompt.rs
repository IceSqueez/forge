use std::sync::Arc;

use forge_components::{
    ConfirmTone, ForgePalette, OverlayPosition, ToastKind, confirm_modal, overlay, tr,
};
use forge_host::overlay_removal::{OverlayRemoval, remove_overlay};
use forge_runtime::RuntimeError;
use forge_storage::OverlayId;
use gpui::{AnyElement, ClickEvent, Context, prelude::*};

use super::{OverlayProblem, OverlaysView};
use crate::app::async_bridge;
use crate::shared::repo_failure::RepoFailure;
use forge_components::PushToast;

pub(super) struct PendingDelete {
    id: OverlayId,
    display_name: String,
}

impl OverlaysView {
    pub(super) fn prompt_delete(&mut self, id: OverlayId, cx: &mut Context<Self>) {
        self.registry.menu_open = None;
        if let Some(index) = self.index_of(&id) {
            self.pending_delete.request(PendingDelete {
                display_name: self.registry.overlays[index].display_name.clone(),
                id: id.clone(),
            });
            self.wiring
                .view
                .update(cx, |wiring, cx| wiring.count_for_delete(id, cx));
        }
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete.cancel();
        self.wiring
            .view
            .update(cx, |wiring, _| wiring.clear_delete_count());
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.pending_delete.take() else {
            return;
        };
        self.wiring
            .view
            .update(cx, |wiring, _| wiring.clear_delete_count());
        if self.registry.selected.as_ref() == Some(&prompt.id) {
            self.registry.selected = None;
            self.clear_test(cx);
        }
        let service = self.handles.service.clone();
        let settings = Arc::clone(&self.handles.settings_repo);
        let router = Arc::clone(&self.handles.audio_router);
        let id = prompt.id;
        let deleted = id.clone();
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move { remove_overlay(&service, settings.as_ref(), &router, &id).await },
            move |this, result: Result<OverlayRemoval, RuntimeError>, cx| match result {
                Ok(OverlayRemoval::Removed { folder_left }) => {
                    if this.receiver.as_ref() == Some(&deleted) {
                        this.apply_receiver(None, cx);
                    }
                    cx.push_toast(ToastKind::Success, tr!("overlays_toast_deleted"));
                    if let Some(error) = folder_left {
                        this.report(RepoFailure::from(error).into(), cx);
                    }
                    this.load(cx);
                }
                Ok(OverlayRemoval::Missing) => this.report(OverlayProblem::Missing, cx),
                Err(error) => this.report(RepoFailure::from(error).into(), cx),
            },
            cx,
        );
        cx.notify();
    }

    fn delete_body(&self, id: &OverlayId, cx: &Context<Self>) -> String {
        match self.wiring.view.read(cx).delete_feed_count(id) {
            Some(count) => tr!(
                "overlays_confirm_delete_body_feeds",
                count = i64::try_from(count).unwrap_or(i64::MAX)
            ),
            None => tr!("overlays_confirm_delete_body"),
        }
    }

    pub(super) fn render_delete_confirm(
        &self,
        prompt: &PendingDelete,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let card = confirm_modal(
            tr!("overlays_confirm_delete_title"),
            self.delete_body(&prompt.id, cx),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(prompt.display_name.clone())
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "overlays-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "overlays-delete-confirm",
            tr!("common_delete"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let weak = cx.entity().downgrade();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("overlays-delete-dismiss", move |_window, cx| {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the view may close before the result arrives"
                )]
                let _ = weak.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }
}
