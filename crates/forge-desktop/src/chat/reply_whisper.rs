use super::platform_gate;
use super::viewer_actions::ViewerAction;
use super::{ChatView, ReplyTarget};
use crate::chat_author::AuthorKey;
use forge_components::{
    BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, InputEvent, Radius, Spacing, TextInput,
    ToastKind, body_family, radius, spacing, tr,
};
use forge_platform_twitch::vocabulary::config_keys as twitch_config_keys;
use forge_types::{PlatformId, SubActionStep, Variant};
use gpui::{ClickEvent, Context, Entity, FontWeight, Window, div, prelude::*};
use std::collections::BTreeMap;

fn build_reply_step(username: &str, message: &str, parent_message_id: &str) -> SubActionStep {
    let mut config = BTreeMap::new();
    config.insert(
        twitch_config_keys::MESSAGE.to_owned(),
        Variant::String(message.to_owned()),
    );
    config.insert(
        twitch_config_keys::PARENT_MESSAGE_ID.to_owned(),
        Variant::String(parent_message_id.to_owned()),
    );
    SubActionStep {
        kind_id: forge_platform_twitch::vocabulary::sub_action_ids::CHAT_REPLY.to_owned(),
        config,
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: Some(tr!("viewer_step_reply", name = username)),
    }
}

impl ChatView {
    pub(super) fn open_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.user_menu.take() else {
            cx.notify();
            return;
        };
        let Some(message_id) = target
            .message_id
            .filter(|_| target.viewer.platform == PlatformId::Twitch)
        else {
            cx.notify();
            return;
        };
        self.reply_target = Some(ReplyTarget {
            username: target.viewer.name,
            message_id,
        });
        self.reply_input.update(cx, |input, cx| {
            input.clear(cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn cancel_reply(&mut self, cx: &mut Context<Self>) {
        self.reply_target = None;
        self.reply_input.update(cx, |input, cx| input.clear(cx));
        cx.notify();
    }

    fn send_reply(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.reply_target.clone() else {
            return;
        };
        let message = self.reply_input.read(cx).content().trim().to_owned();
        if message.is_empty() {
            return;
        }
        self.reply_target = None;
        self.reply_input.update(cx, |input, cx| input.clear(cx));
        let step = build_reply_step(&target.username, &message, &target.message_id);
        let username = target.username;
        self.dispatch_quick_action(
            step,
            platform_gate::platform_integration(PlatformId::Twitch)
                .id_str()
                .to_owned(),
            tr!("viewer_step_reply", name = username),
            |outcome| match outcome {
                Ok(()) => (ToastKind::Success, tr!("chat_reply_sent")),
                Err(e) => (ToastKind::Error, tr!("chat_reply_failed", error = e.text())),
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn on_reply_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Submitted(_) => self.send_reply(cx),
            InputEvent::Cancelled => self.cancel_reply(cx),
            InputEvent::Changed(_) | InputEvent::Blurred(_) => {}
        }
    }

    pub(super) fn open_whisper(
        &mut self,
        key: AuthorKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.point_at_viewer(key, cx);
        self.drawer_menu_open = None;
        self.whisper_open = true;
        self.whisper_input.update(cx, |input, cx| {
            input.clear(cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn cancel_whisper(&mut self, cx: &mut Context<Self>) {
        self.whisper_open = false;
        self.whisper_input.update(cx, |input, cx| input.clear(cx));
        cx.notify();
    }

    pub(super) fn send_whisper(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.displayed_target(cx) else {
            return;
        };
        let message = self.whisper_input.read(cx).content().trim().to_owned();
        if message.is_empty() {
            return;
        }
        self.whisper_open = false;
        self.whisper_input.update(cx, |input, cx| input.clear(cx));
        self.run_viewer_action(
            &target,
            ViewerAction::Whisper(message),
            |outcome| match outcome {
                Ok(()) => (ToastKind::Success, tr!("chat_drawer_whisper_sent")),
                Err(e) => (
                    ToastKind::Error,
                    tr!("chat_drawer_whisper_failed", error = e.text()),
                ),
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn on_whisper_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Submitted(_) => self.send_whisper(cx),
            InputEvent::Cancelled => self.cancel_whisper(cx),
            InputEvent::Changed(_) | InputEvent::Blurred(_) => {}
        }
    }

    pub(super) fn render_whisper_compose(
        &self,
        recipient: String,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let title = div()
            .font_family(body_family())
            .font_weight(FontWeight::MEDIUM)
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(tr!("chat_drawer_whisper_title", recipient = recipient));

        let border = palette.border_regular;
        let border_hover = palette.border_input;
        let surf = palette.surface_overlay;
        let text = palette.text_secondary;
        let text_hover = palette.text_primary;
        let cancel = div()
            .id("chat-drawer-whisper-cancel")
            .flex()
            .items_center()
            .justify_center()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(border)
            .cursor_pointer()
            .hover(move |s| s.bg(surf).border_color(border_hover).text_color(text_hover))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_whisper(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(text)
                    .child(tr!("chat_drawer_whisper_cancel")),
            );

        let brand = palette.brand;
        let shell = palette.shell;
        let send = div()
            .id("chat-drawer-whisper-send")
            .flex()
            .items_center()
            .justify_center()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(brand)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.send_whisper(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(shell)
                    .child(tr!("chat_drawer_whisper_send")),
            );

        let buttons = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, density))
            .child(cancel)
            .child(send);

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .p(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.elevated)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(title)
            .child(self.whisper_input.clone())
            .child(buttons)
    }

    pub(super) fn render_reply_compose(
        &self,
        recipient: String,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let title = div()
            .font_family(body_family())
            .font_weight(FontWeight::MEDIUM)
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(tr!("chat_reply_title", recipient = recipient));

        let border = palette.border_regular;
        let border_hover = palette.border_input;
        let surf = palette.surface_overlay;
        let text = palette.text_secondary;
        let text_hover = palette.text_primary;
        let cancel = div()
            .id("chat-reply-cancel")
            .flex()
            .items_center()
            .justify_center()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(border)
            .cursor_pointer()
            .hover(move |s| s.bg(surf).border_color(border_hover).text_color(text_hover))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_reply(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(text)
                    .child(tr!("chat_drawer_whisper_cancel")),
            );

        let brand = palette.brand;
        let shell = palette.shell;
        let send = div()
            .id("chat-reply-send")
            .flex()
            .items_center()
            .justify_center()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(brand)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.send_reply(cx)))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(shell)
                    .child(tr!("chat_drawer_whisper_send")),
            );

        let buttons = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, density))
            .child(cancel)
            .child(send);

        let card = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .p(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.elevated)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(title)
            .child(self.reply_input.clone())
            .child(buttons);

        div()
            .w_full()
            .px(spacing(Spacing::Md, density))
            .pb(spacing(Spacing::Xs, density))
            .child(card)
    }
}
