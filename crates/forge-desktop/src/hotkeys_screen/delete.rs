use std::sync::Arc;

use forge_components::{ConfirmTone, ForgePalette, OverlayPosition, confirm_modal, overlay, tr};
use forge_types::TriggerInstanceId;
use gpui::{AnyElement, ClickEvent, Context, prelude::*};

use super::HotkeysScreenView;
use crate::hotkey_bindings::{HotkeyEdge, delete_binding, delete_binding_half};

#[derive(Clone, Copy)]
pub(super) enum DeleteScope {
    Row,
    Half(TriggerInstanceId, HotkeyEdge),
}

pub(super) struct DeletePrompt {
    combo: String,
    action: Option<String>,
    paired: bool,
    scope: DeleteScope,
}

impl HotkeysScreenView {
    pub(super) fn prompt_delete(&mut self, key: TriggerInstanceId, cx: &mut Context<Self>) {
        self.menu_open = None;
        self.delete_prompt = self.row_by_key(key).map(|row| DeletePrompt {
            combo: row.combo.clone(),
            action: row.primary_action().map(|(_, name)| name.clone()),
            paired: row.is_hold(),
            scope: DeleteScope::Row,
        });
        cx.notify();
    }

    pub(super) fn prompt_delete_half(
        &mut self,
        instance_id: TriggerInstanceId,
        cx: &mut Context<Self>,
    ) {
        self.menu_open = None;
        self.delete_prompt = self.row_of_instance(instance_id).and_then(|row| {
            let (edge, half) = row
                .halves()
                .find(|(_, half)| half.instance_id == instance_id)?;
            Some(DeletePrompt {
                combo: row.combo.clone(),
                action: half.action.as_ref().map(|(_, name)| name.clone()),
                paired: false,
                scope: DeleteScope::Half(instance_id, edge),
            })
        });
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.delete_prompt = None;
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.delete_prompt.take() else {
            return;
        };
        let reconciler = Arc::clone(&self.reconciler);
        let backend = Arc::clone(&self.backend);
        match prompt.scope {
            DeleteScope::Row => {
                self.run_reload(delete_binding(reconciler, backend, prompt.combo), cx);
            }
            DeleteScope::Half(instance_id, _) => {
                self.run_reload(delete_binding_half(backend, instance_id), cx);
            }
        }
    }

    pub(super) fn render_delete_confirm(
        &self,
        prompt: &DeletePrompt,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let message = match (prompt.scope, prompt.action.as_deref()) {
            (DeleteScope::Half(_, HotkeyEdge::Press), _) => {
                tr!("hotkeys_confirm_delete_press_half")
            }
            (DeleteScope::Half(_, HotkeyEdge::Release), _) => {
                tr!("hotkeys_confirm_delete_release_half")
            }
            (DeleteScope::Row, _) if prompt.paired => tr!("hotkeys_confirm_delete_body_pair"),
            (DeleteScope::Row, Some(action)) => {
                tr!("hotkeys_confirm_delete_body", action = action)
            }
            (DeleteScope::Row, None) => tr!("hotkeys_confirm_delete_body_unassigned"),
        };
        let card = confirm_modal(
            tr!("hotkeys_confirm_delete_title"),
            message,
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(prompt.combo.clone())
        .on_cancel(
            "hotkeys-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "hotkeys-delete-confirm",
            tr!("common_delete"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let weak = cx.entity().downgrade();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("hotkeys-delete-dismiss", move |_window, cx| {
                let _ = weak.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }
}
