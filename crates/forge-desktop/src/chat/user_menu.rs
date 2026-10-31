use super::viewer_actions::{ViewerAction, ViewerTarget};
use super::{CTX_TIMEOUT_10M, ChatView, UserMenuTarget};
use forge_components::{
    ForgePalette, Icon, ToastKind, context_menu, menu_divider, menu_header, menu_item, tr,
};
use forge_types::PlatformId;
use gpui::{AnyElement, ClickEvent, Context, Pixels, Point, SharedString, prelude::*};

const CTX_TIMEOUT_1H: i64 = 3600;
const CTX_TIMEOUT_2W: i64 = 1_209_600;

impl ChatView {
    pub(super) fn open_user_menu(
        &mut self,
        position: Point<Pixels>,
        viewer: ViewerTarget,
        message_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.user_menu = Some(UserMenuTarget {
            position,
            viewer,
            message_id,
        });
        cx.notify();
    }

    fn close_user_menu(&mut self, cx: &mut Context<Self>) {
        if self.user_menu.is_some() {
            self.user_menu = None;
            cx.notify();
        }
    }

    fn ctx_timeout_viewer(&mut self, seconds: i64, cx: &mut Context<Self>) {
        let Some(target) = self.user_menu.take() else {
            cx.notify();
            return;
        };
        self.run_viewer_action(
            &target.viewer,
            ViewerAction::Timeout { seconds },
            |outcome| match outcome {
                Ok(()) => (ToastKind::Success, tr!("chat_ctx_timeout_sent")),
                Err(e) => (
                    ToastKind::Error,
                    tr!("chat_drawer_timeout_failed", error = e.text()),
                ),
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn ctx_ban_viewer(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.user_menu.take() else {
            cx.notify();
            return;
        };
        self.request_ban(target.viewer, cx);
    }

    pub(super) fn render_user_menu(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let target = self.user_menu.as_ref()?;
        let position = target.position;
        let viewer = &target.viewer;
        let can_reply = viewer.platform == PlatformId::Twitch && target.message_id.is_some();
        let view = cx.entity();

        let mut items = vec![menu_header(SharedString::from(viewer.name.clone()))];
        if can_reply {
            items.push(
                menu_item(
                    "chat-ctx-reply",
                    tr!("chat_reply"),
                    cx.listener(|this, _: &ClickEvent, window, cx| this.open_reply(window, cx)),
                )
                .icon(Icon::MessageCircle)
                .into(),
            );
        }
        let timeouts = [
            (
                "chat-ctx-timeout-10m",
                tr!("chat_ctx_timeout_10m"),
                CTX_TIMEOUT_10M,
                None,
            ),
            (
                "chat-ctx-timeout-1h",
                tr!("chat_ctx_timeout_1h"),
                CTX_TIMEOUT_1H,
                None,
            ),
            (
                "chat-ctx-timeout-2w",
                tr!("chat_ctx_timeout_2w"),
                CTX_TIMEOUT_2W,
                Some(palette.warning),
            ),
        ];
        let mut moderation = Vec::new();
        for (id, label, seconds, color) in timeouts {
            if !viewer.supports(&ViewerAction::Timeout { seconds }) {
                continue;
            }
            let mut item = menu_item(
                id,
                label,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.ctx_timeout_viewer(seconds, cx)
                }),
            )
            .icon(Icon::Clock);
            if let Some(color) = color {
                item = item.color(color);
            }
            moderation.push(item.into());
        }
        if viewer.supports(&ViewerAction::Ban) {
            if !moderation.is_empty() {
                moderation.push(menu_divider());
            }
            moderation.push(
                menu_item(
                    "chat-ctx-ban",
                    tr!("chat_ctx_ban"),
                    cx.listener(|this, _: &ClickEvent, _, cx| this.ctx_ban_viewer(cx)),
                )
                .icon(Icon::BellOff)
                .color(palette.danger)
                .into(),
            );
        }
        if can_reply && !moderation.is_empty() {
            items.push(menu_divider());
        }
        items.extend(moderation);

        Some(
            context_menu(position, palette)
                .items(items)
                .dismiss_on_escape(&self.menu_focus)
                .on_dismiss(move |_window, cx| {
                    view.update(cx, |this, cx| this.close_user_menu(cx));
                })
                .into_any_element(),
        )
    }
}
