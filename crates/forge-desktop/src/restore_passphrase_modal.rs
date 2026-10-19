use forge_components::{
    ActivePresentation, Density, FONT_XS, ForgePalette, Icon, InputEvent, ModalSize,
    OverlayPosition, Spacing, TextInput, field_column, field_frame, ghost_button, modal,
    modal_width, overlay, primary_button, spacing, tr,
};
use forge_transfer::Passphrase;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Subscription, Window, div, prelude::*,
};

use crate::restore_problem::RestoreProblem;

pub enum RestorePassphraseEvent {
    Submitted,
    Cancelled,
}

pub struct RestorePassphraseModal {
    passphrase: Entity<TextInput>,
    problem: Option<RestoreProblem>,
    busy: bool,
    focus_pending: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<RestorePassphraseEvent> for RestorePassphraseModal {}

impl RestorePassphraseModal {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
        let passphrase = cx.new(|cx| {
            TextInput::new(tr!("restore_passphrase_placeholder"), cx)
                .with_palette(palette)
                .with_font_size(FONT_XS)
                .plain()
                .secure(true)
        });
        let subs = vec![cx.subscribe(&passphrase, Self::on_field_event)];
        Self {
            passphrase,
            problem: None,
            busy: false,
            focus_pending: true,
            _subs: subs,
        }
    }

    pub(crate) fn can_submit(&self, cx: &Context<Self>) -> bool {
        !self.busy && !self.passphrase.read(cx).content().is_empty()
    }

    pub(crate) fn take_passphrase(&mut self, cx: &mut Context<Self>) -> Option<Passphrase> {
        if !self.can_submit(cx) {
            return None;
        }
        let entered = self.passphrase.read(cx).content().to_owned();
        self.passphrase.update(cx, |input, cx| input.clear(cx));
        self.busy = true;
        self.problem = None;
        cx.notify();
        Some(Passphrase::new(entered))
    }

    pub(crate) fn show_problem(&mut self, problem: RestoreProblem, cx: &mut Context<Self>) {
        self.busy = false;
        self.problem = Some(problem);
        self.focus_pending = true;
        cx.notify();
    }

    fn on_field_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Cancelled => self.cancel(cx),
            InputEvent::Submitted(_) => self.submit(cx),
            InputEvent::Changed(_) => cx.notify(),
            InputEvent::Blurred(_) => {}
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.can_submit(cx) {
            cx.emit(RestorePassphraseEvent::Submitted);
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if !self.busy {
            cx.emit(RestorePassphraseEvent::Cancelled);
        }
    }

    fn footer(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = if self.busy {
            tr!("restore_passphrase_submit_busy")
        } else {
            tr!("restore_passphrase_submit")
        };
        div()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, density))
            .child(
                ghost_button(tr!("common_cancel"), palette)
                    .disabled(self.busy)
                    .on_click(
                        "restore-passphrase-cancel",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
                    ),
            )
            .child(
                primary_button(label, palette)
                    .busy(self.busy)
                    .disabled(!self.can_submit(cx))
                    .on_click(
                        "restore-passphrase-submit",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)),
                    ),
            )
            .into_any_element()
    }
}

impl Render for RestorePassphraseModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        if self.focus_pending {
            self.focus_pending = false;
            self.passphrase
                .update(cx, |input, cx| input.focus(window, cx));
        }
        let problem = self.problem.as_ref().map(RestoreProblem::message);

        let body = div().w_full().flex().flex_col().child(field_column(
            &palette,
            &tr!("restore_passphrase_label"),
            Some(tr!("restore_passphrase_hint").as_str()),
            problem.as_deref(),
            field_frame(&palette)
                .child(self.passphrase.clone())
                .into_any_element(),
        ));

        let card = modal(tr!("restore_passphrase_title"), body, &palette)
            .header_icon(Icon::Lock, palette.brand)
            .subtitle(tr!("restore_passphrase_subtitle"))
            .width(modal_width(ModalSize::Sm))
            .footer(self.footer(&palette, density, cx))
            .on_close(
                "restore-passphrase-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        div().absolute().top_0().left_0().size_full().child(
            overlay(card, &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("restore-passphrase-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel(cx));
                }),
        )
    }
}
