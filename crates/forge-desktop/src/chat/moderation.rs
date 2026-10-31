use super::viewer_actions::{ViewerAction, ViewerTarget};
use super::{ChatView, DRAWER_TIMEOUT_SECONDS};
use crate::async_bridge;
use crate::repo_failure::RepoFailure;
use forge_components::{
    ConfirmTone, ForgePalette, OverlayPosition, PushToast, ToastKind, confirm_modal, overlay, tr,
};
use forge_types::SubActionStep;
use gpui::{AnyElement, ClickEvent, Context, prelude::*};

impl ChatView {
    pub(super) fn run_viewer_action(
        &self,
        target: &ViewerTarget,
        action: ViewerAction,
        toast: impl FnOnce(Result<(), RepoFailure>) -> (ToastKind, String) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(step) = target.step(&action) else {
            return;
        };
        let moderated = matches!(
            action,
            ViewerAction::Ban | ViewerAction::Unban | ViewerAction::Timeout { .. }
        )
        .then_some(target.platform);
        self.dispatch_quick_action_then(
            step,
            target.builtin_id(),
            target.label(&action),
            toast,
            move |this, outcome_ok, cx| {
                if let (true, Some(platform)) = (outcome_ok, moderated) {
                    this.note_moderated(platform, cx);
                }
            },
            cx,
        );
    }

    pub(super) fn dispatch_quick_action(
        &self,
        step: SubActionStep,
        builtin_id: String,
        label: String,
        toast: impl FnOnce(Result<(), RepoFailure>) -> (ToastKind, String) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_quick_action_then(step, builtin_id, label, toast, |_, _, _| {}, cx);
    }

    fn dispatch_quick_action_then(
        &self,
        step: SubActionStep,
        builtin_id: String,
        label: String,
        toast: impl FnOnce(Result<(), RepoFailure>) -> (ToastKind, String) + 'static,
        after: impl FnOnce(&mut Self, bool, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let engine = self.action_engine.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let outcome = async_bridge::run_quick_step(engine, step, builtin_id, label).await;
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| {
            let outcome = rx
                .await
                .unwrap_or_else(|_| Err(RepoFailure::localized(tr!("chat_dispatch_cancelled"))));
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |this, cx| {
                let succeeded = outcome.is_ok();
                let (kind, message) = toast(outcome);
                cx.push_toast(kind, message);
                after(this, succeeded, cx);
            });
        })
        .detach();
    }

    pub(super) fn shoutout_viewer(&mut self, target: ViewerTarget, cx: &mut Context<Self>) {
        self.drawer_menu_open = None;
        self.run_viewer_action(
            &target,
            ViewerAction::Shoutout,
            |outcome| match outcome {
                Ok(()) => (ToastKind::Success, tr!("chat_drawer_shoutout_sent")),
                Err(e) => (
                    ToastKind::Error,
                    tr!("chat_drawer_shoutout_failed", error = e.text()),
                ),
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn timeout_viewer(&mut self, target: ViewerTarget, cx: &mut Context<Self>) {
        self.drawer_menu_open = None;
        self.run_viewer_action(
            &target,
            ViewerAction::Timeout {
                seconds: DRAWER_TIMEOUT_SECONDS,
            },
            |outcome| match outcome {
                Ok(()) => (ToastKind::Success, tr!("chat_drawer_timeout_sent")),
                Err(e) => (
                    ToastKind::Error,
                    tr!("chat_drawer_timeout_failed", error = e.text()),
                ),
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn request_ban(&mut self, target: ViewerTarget, cx: &mut Context<Self>) {
        self.drawer_menu_open = None;
        self.pending_ban.request(target);
        cx.notify();
    }

    pub(super) fn cancel_ban(&mut self, cx: &mut Context<Self>) {
        self.pending_ban.cancel();
        cx.notify();
    }

    pub(super) fn confirm_ban(&mut self, cx: &mut Context<Self>) {
        if let Some(target) = self.pending_ban.take() {
            self.ban_viewer(target, cx);
        }
        cx.notify();
    }

    pub(super) fn render_ban_confirm(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let target = self.pending_ban.get()?;
        let card = confirm_modal(
            tr!("chat_ban_confirm_title"),
            tr!("chat_ban_confirm_message", name = target.name.as_str()),
            ConfirmTone::Destructive,
            palette,
        )
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "chat-ban-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_ban(cx)),
        )
        .on_confirm(
            "chat-ban-confirm",
            tr!("chat_ctx_ban"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_ban(cx)),
        );
        let view = cx.entity();
        Some(
            overlay(card, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("chat-ban-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_ban(cx));
                })
                .into_any_element(),
        )
    }

    pub(super) fn ban_viewer(&mut self, target: ViewerTarget, cx: &mut Context<Self>) {
        self.drawer_menu_open = None;
        self.run_viewer_action(
            &target,
            ViewerAction::Ban,
            |outcome| match outcome {
                Ok(()) => (ToastKind::Success, tr!("chat_drawer_ban_sent")),
                Err(e) => (
                    ToastKind::Error,
                    tr!("chat_drawer_ban_failed", error = e.text()),
                ),
            },
            cx,
        );
        cx.notify();
    }
}
