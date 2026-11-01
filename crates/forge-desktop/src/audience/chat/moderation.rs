use super::DRAWER_TIMEOUT_SECONDS;
use super::viewer_actions::{ViewerAction, ViewerTarget};
use crate::shared::repo_failure::RepoFailure;
use forge_components::{
    Confirm, ConfirmTone, ForgePalette, OverlayPosition, PushToast, ToastKind, confirm_modal,
    overlay, tr,
};
use forge_runtime::ActionEngineHandle;
use forge_types::{PlatformId, SubActionStep};
use gpui::{AnyElement, ClickEvent, Context, EventEmitter, prelude::*};

pub(super) enum ChatModerationEvent {
    Moderated(PlatformId),
}

pub(super) struct ChatModeration {
    pub(super) action_engine: ActionEngineHandle,
    rt_handle: tokio::runtime::Handle,
    pub(super) pending_ban: Confirm<ViewerTarget>,
}

impl EventEmitter<ChatModerationEvent> for ChatModeration {}

impl ChatModeration {
    pub(super) fn new(
        action_engine: ActionEngineHandle,
        rt_handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            action_engine,
            rt_handle,
            pending_ban: Confirm::default(),
        }
    }

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
        self.dispatch(
            step,
            target.builtin_id(),
            target.label(&action),
            toast,
            moderated,
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
        self.dispatch(step, builtin_id, label, toast, None, cx);
    }

    fn dispatch(
        &self,
        step: SubActionStep,
        builtin_id: String,
        label: String,
        toast: impl FnOnce(Result<(), RepoFailure>) -> (ToastKind, String) + 'static,
        moderated: Option<PlatformId>,
        cx: &mut Context<Self>,
    ) {
        let engine = self.action_engine.clone();
        let reply = crate::app::async_bridge::on_runtime(&self.rt_handle, async move {
            crate::automation::quick_step::run_quick_step(
                engine,
                step,
                forge_runtime::QuickActionOrigin { builtin_id, label },
            )
            .await
        });
        cx.spawn(async move |this, cx| {
            let outcome = reply
                .await
                .unwrap_or_else(|_| Err(RepoFailure::localized(tr!("chat_dispatch_cancelled"))));
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |_, cx| {
                let succeeded = outcome.is_ok();
                let (kind, message) = toast(outcome);
                cx.push_toast(kind, message);
                if let (true, Some(platform)) = (succeeded, moderated) {
                    cx.emit(ChatModerationEvent::Moderated(platform));
                }
            });
        })
        .detach();
    }

    pub(super) fn shoutout_viewer(&self, target: &ViewerTarget, cx: &mut Context<Self>) {
        self.run_viewer_action(
            target,
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
    }

    pub(super) fn timeout_viewer(&self, target: &ViewerTarget, cx: &mut Context<Self>) {
        self.run_viewer_action(
            target,
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
    }

    pub(super) fn request_ban(&mut self, target: ViewerTarget, cx: &mut Context<Self>) {
        self.pending_ban.request(target);
        cx.notify();
    }

    pub(super) fn cancel_ban(&mut self, cx: &mut Context<Self>) {
        self.pending_ban.cancel();
        cx.notify();
    }

    pub(super) fn confirm_ban(&mut self, cx: &mut Context<Self>) {
        if let Some(target) = self.pending_ban.take() {
            self.ban_viewer(&target, cx);
        }
        cx.notify();
    }

    pub(super) fn ban_viewer(&self, target: &ViewerTarget, cx: &mut Context<Self>) {
        self.run_viewer_action(
            target,
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
}
