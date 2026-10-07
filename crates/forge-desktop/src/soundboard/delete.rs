use std::sync::Arc;

use forge_components::{ConfirmTone, ForgePalette, OverlayPosition, confirm_modal, overlay, tr};
use forge_types::ClipId;
use gpui::{AnyElement, ClickEvent, Context, prelude::*};

use super::SoundboardView;
use crate::async_bridge;
use crate::clip_messages::failure_message;

impl SoundboardView {
    pub(super) fn request_delete(&mut self, id: ClipId, cx: &mut Context<Self>) {
        self.pending_delete.request(id);
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete.cancel();
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.pending_delete.take() else {
            return;
        };
        self.player.stop(id);
        self.playing.remove(&id);
        cx.notify();
        let library = Arc::clone(&self.library);
        async_bridge::run_async(
            &self.rt_handle,
            async move { library.delete_clip(id).await },
            |this, result, cx| match result {
                Ok(_) => this.reload(cx),
                Err(error) => this.on_load_error(failure_message(&error), cx),
            },
            cx,
        );
    }

    pub(super) fn render_delete_confirm(
        &self,
        id: ClipId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self
            .clips
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let card = confirm_modal(
            tr!("soundboard_delete_title"),
            tr!("soundboard_delete_body"),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .on_cancel(
            "sb-del-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "sb-del-confirm",
            tr!("common_delete"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );
        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("sb-del-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }
}
