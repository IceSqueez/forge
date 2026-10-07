use std::sync::Arc;

use forge_components::{
    ConfirmTone, ForgePalette, OverlayPosition, ToastKind, confirm_modal, overlay, tr,
};
use forge_storage::OverlayId;
use gpui::{AnyElement, ClickEvent, Context, prelude::*};

use super::OverlaysView;
use super::receiver;
use crate::async_bridge;
use crate::toasts::PushToast;

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
            self.clear_test();
        }
        let service = self.handles.service.clone();
        let settings = Arc::clone(&self.handles.settings_repo);
        let router = Arc::clone(&self.handles.audio_router);
        let id = prompt.id;
        let deleted = id.clone();
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move {
                let was_receiver = receiver::release_receiver(settings.as_ref(), &id)
                    .await
                    .map_err(|e| e.to_string())?;
                let removed = match service.delete(&id).await {
                    Ok(removed) => removed,
                    Err(error) => {
                        if was_receiver {
                            receiver::restore_receiver(settings.as_ref(), &id).await;
                        }
                        return Err(error.to_string());
                    }
                };
                if !removed {
                    if was_receiver {
                        receiver::restore_receiver(settings.as_ref(), &id).await;
                    }
                    return Ok((false, None));
                }
                if was_receiver {
                    router.apply().await;
                }
                if let Err(error) = service.release_media(&id).await {
                    tracing::warn!(overlay = %id, %error, "overlay media references not released");
                }
                let swept = service
                    .remove_folder(&id)
                    .await
                    .err()
                    .map(|e| e.to_string());
                Ok((true, swept))
            },
            move |this, result: Result<(bool, Option<String>), String>, cx| match result {
                Ok((true, swept)) => {
                    if this.receiver.as_ref() == Some(&deleted) {
                        this.apply_receiver(None, cx);
                    }
                    cx.push_toast(ToastKind::Success, tr!("overlays_toast_deleted"));
                    if let Some(message) = swept {
                        this.report(&message, cx);
                    }
                    this.load(cx);
                }
                Ok((false, _)) => this.report(&tr!("overlays_toast_missing"), cx),
                Err(message) => this.report(&message, cx),
            },
            cx,
        );
        cx.notify();
    }

    fn delete_body(&self, id: &OverlayId, cx: &Context<Self>) -> String {
        let base = tr!("overlays_confirm_delete_body");
        match self.wiring.view.read(cx).delete_feed_count(id) {
            Some(count) => format!(
                "{base} {}",
                tr!("overlays_confirm_delete_feeds", count = count as i64)
            ),
            None => base,
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
                let _ = weak.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }
}
