use super::QueuesView;
use forge_components::{ConfirmTone, ForgePalette, OverlayPosition, confirm_modal, overlay, tr};
use forge_types::DEFAULT_QUEUE_ID;
use gpui::prelude::*;
use gpui::{ClickEvent, Context, SharedString};

impl QueuesView {
    pub(super) fn render_delete_confirm(
        &self,
        name: SharedString,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let default_name = self
            .queues
            .iter()
            .find(|q| q.id == DEFAULT_QUEUE_ID)
            .map(|q| q.name.clone())
            .unwrap_or_default();
        let card = confirm_modal(
            tr!("queues_delete_confirm_title"),
            tr!(
                "queues_delete_confirm_body",
                default = default_name.as_str()
            ),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "queues-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "queues-delete-confirm",
            tr!("common_delete"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("queues-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
    }

    pub(super) fn render_free_confirm(
        &self,
        name: SharedString,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let card = confirm_modal(
            tr!("queues_free_confirm_title"),
            tr!("queues_free_confirm_body"),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "queues-free-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_free(cx)),
        )
        .on_confirm(
            "queues-free-confirm",
            tr!("queues_free_btn"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_free(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("queues-free-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_free(cx));
            })
    }
}
