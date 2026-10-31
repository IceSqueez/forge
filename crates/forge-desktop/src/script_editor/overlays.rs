use super::ScriptEditorView;
use crate::script_lifecycle::{delete_hint, rename_hint};
use forge_components::{ConfirmTone, ForgePalette, OverlayPosition, confirm_modal, overlay, tr};
use gpui::prelude::*;
use gpui::{AnyElement, ClickEvent, Context};

impl ScriptEditorView {
    pub(super) fn delete_overlay(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
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
            tr!("widget_confirm_cancel"),
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

    pub(super) fn rename_overlay(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pending = self.pending_rename.as_ref()?;
        let card = confirm_modal(
            tr!("script_editor_rename_title"),
            rename_hint(&pending.references),
            ConfirmTone::Warning,
            palette,
        )
        .item_name(format!("{} -> {}", pending.old_name, pending.new_name))
        .on_cancel(
            "script-rename-cancel",
            tr!("widget_confirm_cancel"),
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

    pub(super) fn discard_overlay(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.pending_nav.as_ref()?;

        let card = confirm_modal(
            tr!("script_editor_discard_title"),
            tr!("script_editor_discard_body"),
            ConfirmTone::Warning,
            palette,
        )
        .esc_hint(tr!("script_editor_discard_esc_hint"))
        .on_cancel(
            "script-discard-cancel",
            tr!("script_editor_discard_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_discard(cx)),
        )
        .on_confirm(
            "script-discard-confirm",
            tr!("script_editor_discard_confirm"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_discard(cx)),
        );

        let view = cx.entity();
        Some(
            overlay(card, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("script-discard-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_discard(cx));
                })
                .into_any_element(),
        )
    }
}
