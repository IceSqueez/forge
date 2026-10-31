use forge_components::focus_inside_modal;
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

pub(crate) enum RestorePassphraseEvent {
    Submitted,
    Cancelled,
}

pub(crate) struct RestorePassphraseModal {
    passphrase: Entity<TextInput>,
    problem: Option<RestoreProblem>,
    busy: bool,
    focus_pending: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<RestorePassphraseEvent> for RestorePassphraseModal {}

impl RestorePassphraseModal {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
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
            focus_inside_modal(window, cx, |window, cx| {
                self.passphrase
                    .update(cx, |input, cx| input.focus(window, cx))
            });
        }
        let problem = self.problem.as_ref().map(RestoreProblem::message);

        let body = div().w_full().flex().flex_col().child(field_column(
            &palette,
            &tr!("restore_passphrase_label"),
            Some(tr!("restore_passphrase_hint").as_str()),
            problem.as_deref(),
            field_frame(&palette, density)
                .child(self.passphrase.clone())
                .into_any_element(),
            density,
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

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use forge_storage::Language;
    use gpui::{AppContext, Entity, TestAppContext};

    use super::{RestorePassphraseEvent, RestorePassphraseModal};
    use crate::i18n::install_language;
    use crate::restore_problem::RestoreProblem;
    use crate::test_support::install_presentation;

    pub(crate) fn type_passphrase(
        cx: &mut TestAppContext,
        modal: &Entity<RestorePassphraseModal>,
        text: &str,
    ) {
        modal.update(cx, |modal, cx| {
            modal
                .passphrase
                .update(cx, |input, cx| input.set_content(text.to_owned(), cx));
        });
    }

    fn modal_with_events(
        cx: &mut TestAppContext,
    ) -> (
        Entity<RestorePassphraseModal>,
        Rc<RefCell<Vec<&'static str>>>,
    ) {
        install_language(Language::En);
        install_presentation(cx);
        let modal = cx.new(RestorePassphraseModal::new);
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&events);
        cx.update(|cx| {
            cx.subscribe(&modal, move |_, event: &RestorePassphraseEvent, _| {
                sink.borrow_mut().push(match event {
                    RestorePassphraseEvent::Submitted => "submitted",
                    RestorePassphraseEvent::Cancelled => "cancelled",
                });
            })
            .detach();
        });
        (modal, events)
    }

    fn take(cx: &mut TestAppContext, modal: &Entity<RestorePassphraseModal>) -> bool {
        modal.update(cx, |modal, cx| modal.take_passphrase(cx).is_some())
    }

    #[gpui::test]
    fn an_empty_passphrase_is_neither_submitted_nor_taken(cx: &mut TestAppContext) {
        let (modal, events) = modal_with_events(cx);

        modal.update(cx, |modal, cx| modal.submit(cx));
        cx.run_until_parked();

        assert_eq!((take(cx, &modal), events.borrow().len()), (false, 0));
    }

    #[gpui::test]
    fn taking_the_passphrase_clears_the_field(cx: &mut TestAppContext) {
        let (modal, _) = modal_with_events(cx);
        type_passphrase(cx, &modal, "correct horse battery");

        let taken = take(cx, &modal);

        let left = modal.update(cx, |modal, cx| {
            modal.passphrase.read(cx).content().to_owned()
        });
        assert_eq!((taken, left), (true, String::new()));
    }

    #[gpui::test]
    fn a_taken_passphrase_blocks_submit_and_cancel_until_a_problem_returns(
        cx: &mut TestAppContext,
    ) {
        let (modal, events) = modal_with_events(cx);
        type_passphrase(cx, &modal, "correct horse battery");
        take(cx, &modal);
        type_passphrase(cx, &modal, "typed while checking");

        modal.update(cx, |modal, cx| {
            modal.submit(cx);
            modal.cancel(cx);
        });
        cx.run_until_parked();

        assert_eq!(events.borrow().len(), 0);
    }

    #[gpui::test]
    fn a_wrong_passphrase_reopens_the_field_for_another_try(cx: &mut TestAppContext) {
        let (modal, events) = modal_with_events(cx);
        type_passphrase(cx, &modal, "correct horse battery");
        take(cx, &modal);

        modal.update(cx, |modal, cx| {
            modal.show_problem(RestoreProblem::WrongPassphrase, cx)
        });
        type_passphrase(cx, &modal, "second attempt here");
        modal.update(cx, |modal, cx| modal.submit(cx));
        cx.run_until_parked();

        assert_eq!(*events.borrow(), vec!["submitted"]);
    }

    #[gpui::test]
    fn cancelling_an_idle_prompt_reports_cancelled(cx: &mut TestAppContext) {
        let (modal, events) = modal_with_events(cx);

        modal.update(cx, |modal, cx| modal.cancel(cx));
        cx.run_until_parked();

        assert_eq!(*events.borrow(), vec!["cancelled"]);
    }
}
