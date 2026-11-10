use super::viewer_actions::ViewerAction;
use super::viewer_drawer::ViewerDrawer;
use crate::audience::chat_author::AuthorKey;
use forge_components::{
    ActivePresentation, BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, InputEvent,
    KeyActivated, Radius, Spacing, TextInput, ToastKind, radius, spacing, tr,
};
use gpui::{ClickEvent, Context, Entity, FontWeight, Window, div, prelude::*};

impl ViewerDrawer {
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

    pub(super) fn reset_whisper(&mut self, cx: &mut Context<Self>) {
        self.whisper_open = false;
        self.whisper_input.update(cx, |input, cx| input.clear(cx));
    }

    fn cancel_whisper(&mut self, cx: &mut Context<Self>) {
        self.reset_whisper(cx);
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
        self.moderation.update(cx, |moderation, cx| {
            moderation.run_viewer_action(
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
        });
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
            .font_family(cx.body_family())
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
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_whisper(cx)))
            .child(
                div()
                    .font_family(cx.body_family())
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
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.send_whisper(cx)))
            .child(
                div()
                    .font_family(cx.body_family())
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
}
