use super::ScriptList;
use crate::script_lifecycle::{delete_hint, rename_hint};
use forge_components::{ConfirmTone, ForgePalette, OverlayPosition, confirm_modal, overlay, tr};
use gpui::prelude::*;
use gpui::{AnyElement, ClickEvent, Context};

impl ScriptList {
    pub(in super::super) fn render_prompt_layer(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.pending_delete.is_pending() {
            self.delete_overlay(palette, cx)
        } else if self.pending_rename.is_some() {
            self.rename_overlay(palette, cx)
        } else {
            None
        }
    }

    fn delete_overlay(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = *self.pending_delete.get()?;
        let name = self
            .find_entry(id)
            .map(|e| e.name.clone())
            .unwrap_or_default();

        let kind = tr!("widget_confirm_delete_kind_script");
        let card = confirm_modal(
            tr!("widget_confirm_delete_title", kind = kind.as_str()),
            delete_hint(self.delete_referrers.as_ref()),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "script-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "script-delete-confirm",
            tr!("script_editor_delete_action"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let view = cx.entity();
        Some(
            overlay(card, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("script-delete-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_delete(cx));
                })
                .into_any_element(),
        )
    }

    fn rename_overlay(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pending = self.pending_rename.as_ref()?;
        let card = confirm_modal(
            tr!("script_editor_rename_title"),
            rename_hint(&pending.references),
            ConfirmTone::Warning,
            palette,
        )
        .item_name(format!("{} -> {}", pending.old_name, pending.new_name))
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "script-rename-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_pending_rename(cx)),
        )
        .on_confirm(
            "script-rename-confirm",
            tr!("script_editor_rename_action"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_pending_rename(cx)),
        );

        let view = cx.entity();
        Some(
            overlay(card, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("script-rename-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_pending_rename(cx));
                })
                .into_any_element(),
        )
    }
}
