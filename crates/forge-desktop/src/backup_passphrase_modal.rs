use forge_components::focus_inside_modal;
use forge_components::{
    Density, FONT_XS, ForgePalette, Icon, InputEvent, ModalSize, OverlayPosition, Spacing,
    TextInput, body_family, field_column, field_frame, ghost_button, icon, modal, modal_width,
    overlay, primary_button, spacing, tr,
};
use forge_transfer::{MIN_PASSPHRASE_CHARS, Passphrase};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Subscription, Window, div, prelude::*,
    px,
};

use forge_components::ActivePresentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PassphraseCheck {
    pub too_short: bool,
    pub mismatch: bool,
    pub passphrase_entered: bool,
    pub confirmation_entered: bool,
}

impl PassphraseCheck {
    pub(crate) fn of(passphrase: &str, confirmation: &str) -> Self {
        Self {
            too_short: passphrase.chars().count() < MIN_PASSPHRASE_CHARS,
            mismatch: passphrase != confirmation,
            passphrase_entered: !passphrase.is_empty(),
            confirmation_entered: !confirmation.is_empty(),
        }
    }

    pub(crate) fn is_valid(self) -> bool {
        !self.too_short && !self.mismatch
    }

    pub(crate) fn shows_too_short(self) -> bool {
        self.passphrase_entered && self.too_short
    }

    pub(crate) fn shows_mismatch(self) -> bool {
        self.confirmation_entered && self.mismatch
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PassphrasePurpose {
    FullBackup,
    ScheduleSetup,
    ScheduleChange,
}

impl PassphrasePurpose {
    fn title(self) -> String {
        match self {
            Self::FullBackup => tr!("settings_storage_passphrase_title"),
            Self::ScheduleSetup => tr!("backup_schedule_setup_title"),
            Self::ScheduleChange => tr!("backup_schedule_change_title"),
        }
    }

    fn subtitle(self) -> String {
        match self {
            Self::FullBackup => tr!("settings_storage_passphrase_subtitle"),
            Self::ScheduleSetup => tr!("backup_schedule_setup_subtitle"),
            Self::ScheduleChange => tr!("backup_schedule_change_subtitle"),
        }
    }

    fn warning(self) -> String {
        match self {
            Self::FullBackup => tr!("settings_storage_passphrase_warning"),
            Self::ScheduleSetup => tr!("backup_schedule_setup_warning"),
            Self::ScheduleChange => tr!("backup_schedule_change_warning"),
        }
    }

    fn confirm_label(self) -> String {
        match self {
            Self::FullBackup => tr!("settings_storage_passphrase_confirm_btn"),
            Self::ScheduleSetup => tr!("backup_schedule_setup_confirm_btn"),
            Self::ScheduleChange => tr!("backup_schedule_change_confirm_btn"),
        }
    }

    fn new_passphrase_label(self) -> String {
        match self {
            Self::ScheduleChange => tr!("backup_schedule_change_new_label"),
            Self::FullBackup | Self::ScheduleSetup => tr!("settings_storage_passphrase_label"),
        }
    }

    pub(crate) fn asks_current(self) -> bool {
        self == Self::ScheduleChange
    }
}

pub(crate) enum BackupPassphraseEvent {
    Confirmed,
    Cancelled,
}

pub(crate) struct BackupPassphraseModal {
    purpose: PassphrasePurpose,
    current: Option<Entity<TextInput>>,
    current_error: Option<String>,
    working: bool,
    passphrase: Entity<TextInput>,
    confirmation: Entity<TextInput>,
    focus_pending: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<BackupPassphraseEvent> for BackupPassphraseModal {}

impl BackupPassphraseModal {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self::with_purpose(PassphrasePurpose::FullBackup, cx)
    }

    pub(crate) fn with_purpose(purpose: PassphrasePurpose, cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
        let current = purpose.asks_current().then(|| {
            cx.new(|cx| {
                TextInput::new(tr!("backup_schedule_change_current_placeholder"), cx)
                    .with_palette(palette)
                    .with_font_size(FONT_XS)
                    .plain()
                    .secure(true)
            })
        });
        let passphrase = cx.new(|cx| {
            TextInput::new(tr!("settings_storage_passphrase_placeholder"), cx)
                .with_palette(palette)
                .with_font_size(FONT_XS)
                .plain()
                .secure(true)
        });
        let confirmation = cx.new(|cx| {
            TextInput::new(tr!("settings_storage_passphrase_confirm_placeholder"), cx)
                .with_palette(palette)
                .with_font_size(FONT_XS)
                .plain()
                .secure(true)
        });
        let mut subs = vec![
            cx.subscribe(&passphrase, Self::on_field_event),
            cx.subscribe(&confirmation, Self::on_field_event),
        ];
        if let Some(current) = &current {
            subs.push(cx.subscribe(current, Self::on_field_event));
        }
        Self {
            purpose,
            current,
            current_error: None,
            working: false,
            passphrase,
            confirmation,
            focus_pending: true,
            _subs: subs,
        }
    }

    pub(crate) fn check(&self, cx: &Context<Self>) -> PassphraseCheck {
        PassphraseCheck::of(
            self.passphrase.read(cx).content(),
            self.confirmation.read(cx).content(),
        )
    }

    fn current_entered(&self, cx: &Context<Self>) -> bool {
        self.current
            .as_ref()
            .is_none_or(|current| !current.read(cx).content().is_empty())
    }

    pub(crate) fn ready(&self, cx: &Context<Self>) -> bool {
        !self.working && self.check(cx).is_valid() && self.current_entered(cx)
    }

    pub(crate) fn entered_passphrase(&self, cx: &Context<Self>) -> Option<Passphrase> {
        self.check(cx)
            .is_valid()
            .then(|| Passphrase::new(self.passphrase.read(cx).content().to_owned()))
    }

    pub(crate) fn entered_current(&self, cx: &Context<Self>) -> Option<Passphrase> {
        self.current
            .as_ref()
            .map(|current| current.read(cx).content().to_owned())
            .filter(|current| !current.is_empty())
            .map(Passphrase::new)
    }

    pub(crate) fn set_working(&mut self, working: bool, cx: &mut Context<Self>) {
        self.working = working;
        if working {
            self.current_error = None;
        }
        cx.notify();
    }

    pub(crate) fn reject_current(&mut self, cx: &mut Context<Self>) {
        self.working = false;
        self.current_error = Some(tr!("backup_schedule_change_wrong_current"));
        if let Some(current) = &self.current {
            current.update(cx, |input, cx| input.clear(cx));
        }
        self.focus_pending = true;
        cx.notify();
    }

    pub(crate) fn take_passphrase(&mut self, cx: &mut Context<Self>) -> Option<Passphrase> {
        if !self.check(cx).is_valid() {
            return None;
        }
        let entered = self.passphrase.read(cx).content().to_owned();
        self.passphrase.update(cx, |input, cx| input.clear(cx));
        self.confirmation.update(cx, |input, cx| input.clear(cx));
        cx.notify();
        Some(Passphrase::new(entered))
    }

    fn on_field_event(
        &mut self,
        field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Cancelled => self.cancel(cx),
            InputEvent::Submitted(_) => self.confirm(cx),
            InputEvent::Changed(_) => {
                if self
                    .current
                    .as_ref()
                    .is_some_and(|current| *current == field)
                {
                    self.current_error = None;
                }
                cx.notify();
            }
            InputEvent::Blurred(_) => {}
        }
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        if self.ready(cx) {
            cx.emit(BackupPassphraseEvent::Confirmed);
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(BackupPassphraseEvent::Cancelled);
    }

    fn warning(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(Icon::AlertTriangle, FONT_XS, palette.warning))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.warning)
                    .child(self.purpose.warning()),
            )
            .into_any_element()
    }

    fn footer(
        &self,
        ready: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, density))
            .child(ghost_button(tr!("common_cancel"), palette).on_click(
                "backup-passphrase-cancel",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            ))
            .child(
                primary_button(self.purpose.confirm_label(), palette)
                    .busy(self.working)
                    .disabled(!ready)
                    .on_click(
                        "backup-passphrase-confirm",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.confirm(cx)),
                    ),
            )
            .into_any_element()
    }
}

impl Render for BackupPassphraseModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        if self.focus_pending {
            self.focus_pending = false;
            let first = self.current.as_ref().unwrap_or(&self.passphrase);
            focus_inside_modal(window, cx, |window, cx| {
                first.update(cx, |input, cx| input.focus(window, cx))
            });
        }
        let check = self.check(cx);
        let ready = self.ready(cx);

        let too_short = tr!(
            "settings_storage_passphrase_too_short",
            min = MIN_PASSPHRASE_CHARS.to_string()
        );
        let mismatch = tr!("settings_storage_passphrase_mismatch");
        let hint = tr!(
            "settings_storage_passphrase_hint",
            min = MIN_PASSPHRASE_CHARS.to_string()
        );

        let current_field = self.current.as_ref().map(|current| {
            field_column(
                &palette,
                &tr!("backup_schedule_change_current_label"),
                None,
                self.current_error.as_deref(),
                field_frame(&palette, density)
                    .child(current.clone())
                    .into_any_element(),
                density,
            )
        });
        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(self.warning(&palette, density))
            .children(current_field)
            .child(field_column(
                &palette,
                &self.purpose.new_passphrase_label(),
                Some(hint.as_str()),
                check.shows_too_short().then_some(too_short.as_str()),
                field_frame(&palette, density)
                    .child(self.passphrase.clone())
                    .into_any_element(),
                density,
            ))
            .child(field_column(
                &palette,
                &tr!("settings_storage_passphrase_confirm_label"),
                None,
                check.shows_mismatch().then_some(mismatch.as_str()),
                field_frame(&palette, density)
                    .child(self.confirmation.clone())
                    .into_any_element(),
                density,
            ));

        let card = modal(self.purpose.title(), body, &palette)
            .header_icon(Icon::Lock, palette.brand)
            .subtitle(self.purpose.subtitle())
            .width(modal_width(ModalSize::Sm))
            .footer(self.footer(ready, &palette, density, cx))
            .on_close(
                "backup-passphrase-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        div().absolute().top_0().left_0().size_full().child(
            overlay(card, &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("backup-passphrase-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel(cx));
                }),
        )
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use gpui::TestAppContext;

    use super::*;
    use crate::test_support::install_presentation;

    fn chars(c: char, n: usize) -> String {
        std::iter::repeat_n(c, n).collect()
    }

    impl BackupPassphraseModal {
        pub(crate) fn fill(
            &mut self,
            current: Option<&str>,
            passphrase: &str,
            confirmation: &str,
            cx: &mut Context<Self>,
        ) {
            if let (Some(field), Some(text)) = (&self.current, current) {
                let text = text.to_owned();
                field.update(cx, |input, cx| input.set_content(text, cx));
            }
            let (passphrase, confirmation) = (passphrase.to_owned(), confirmation.to_owned());
            self.passphrase
                .update(cx, |input, cx| input.set_content(passphrase, cx));
            self.confirmation
                .update(cx, |input, cx| input.set_content(confirmation, cx));
        }

        pub(crate) fn submit(&mut self, cx: &mut Context<Self>) {
            self.confirm(cx);
        }

        pub(crate) fn shown_current_error(&self) -> Option<String> {
            self.current_error.clone()
        }

        fn typed_current(&self, cx: &gpui::App) -> Option<String> {
            self.current
                .as_ref()
                .map(|current| current.read(cx).content().to_owned())
        }

        fn typed_passphrase(&self, cx: &gpui::App) -> String {
            self.passphrase.read(cx).content().to_owned()
        }
    }

    fn prompt(
        cx: &mut TestAppContext,
        purpose: PassphrasePurpose,
    ) -> Entity<BackupPassphraseModal> {
        install_presentation(cx);
        cx.update(|cx| cx.new(|cx| BackupPassphraseModal::with_purpose(purpose, cx)))
    }

    fn confirmations(
        cx: &mut TestAppContext,
        modal: &Entity<BackupPassphraseModal>,
    ) -> std::rc::Rc<std::cell::Cell<usize>> {
        let count = std::rc::Rc::new(std::cell::Cell::new(0));
        let seen = std::rc::Rc::clone(&count);
        cx.update(|cx| {
            cx.subscribe(modal, move |_, event: &BackupPassphraseEvent, _| {
                if matches!(event, BackupPassphraseEvent::Confirmed) {
                    seen.set(seen.get() + 1);
                }
            })
            .detach();
        });
        count
    }

    #[gpui::test]
    fn change_prompt_does_not_confirm_until_the_current_passphrase_is_entered(
        cx: &mut TestAppContext,
    ) {
        let modal = prompt(cx, PassphrasePurpose::ScheduleChange);
        let confirmed = confirmations(cx, &modal);
        let new = chars('n', MIN_PASSPHRASE_CHARS);

        modal.update(cx, |modal, cx| {
            modal.fill(Some(""), &new, &new, cx);
            modal.submit(cx);
        });
        cx.run_until_parked();
        let without_current = confirmed.get();
        modal.update(cx, |modal, cx| {
            modal.fill(Some("old passphrase"), &new, &new, cx);
            modal.submit(cx);
        });
        cx.run_until_parked();

        assert_eq!((without_current, confirmed.get()), (0, 1));
    }

    #[gpui::test]
    fn setup_prompt_confirms_on_a_matching_passphrase_without_asking_a_current_one(
        cx: &mut TestAppContext,
    ) {
        let modal = prompt(cx, PassphrasePurpose::ScheduleSetup);
        let confirmed = confirmations(cx, &modal);
        let new = chars('n', MIN_PASSPHRASE_CHARS);

        modal.update(cx, |modal, cx| {
            modal.fill(None, &new, &new, cx);
            modal.submit(cx);
        });
        cx.run_until_parked();

        let asks_current = modal.read_with(cx, |modal, cx| modal.typed_current(cx).is_some());
        assert_eq!((asks_current, confirmed.get()), (false, 1));
    }

    #[gpui::test]
    fn a_working_prompt_ignores_repeated_confirms(cx: &mut TestAppContext) {
        let modal = prompt(cx, PassphrasePurpose::ScheduleSetup);
        let confirmed = confirmations(cx, &modal);
        let new = chars('n', MIN_PASSPHRASE_CHARS);

        modal.update(cx, |modal, cx| {
            modal.fill(None, &new, &new, cx);
            modal.set_working(true, cx);
            modal.submit(cx);
        });
        cx.run_until_parked();

        assert_eq!(confirmed.get(), 0);
    }

    #[gpui::test]
    fn rejecting_the_current_passphrase_clears_it_shows_the_error_and_keeps_the_new_one(
        cx: &mut TestAppContext,
    ) {
        let modal = prompt(cx, PassphrasePurpose::ScheduleChange);
        let new = chars('n', MIN_PASSPHRASE_CHARS);

        modal.update(cx, |modal, cx| {
            modal.fill(Some("wrong passphrase"), &new, &new, cx);
            modal.set_working(true, cx);
            modal.reject_current(cx);
        });
        cx.run_until_parked();

        let shown = modal.update(cx, |modal, cx| {
            (
                modal.shown_current_error().is_some(),
                modal.typed_current(cx),
                modal.typed_passphrase(cx),
                modal.ready(cx),
            )
        });
        assert_eq!(shown, (true, Some(String::new()), new, false));
    }

    #[gpui::test]
    fn editing_the_current_passphrase_clears_its_error(cx: &mut TestAppContext) {
        let modal = prompt(cx, PassphrasePurpose::ScheduleChange);
        modal.update(cx, |modal, cx| modal.reject_current(cx));

        let current = modal.read_with(cx, |modal, _| modal.current.clone().unwrap());
        current.update(cx, |_, cx| cx.emit(InputEvent::Changed("o".into())));
        cx.run_until_parked();

        assert_eq!(
            modal.read_with(cx, |modal, _| modal.shown_current_error()),
            None
        );
    }

    #[gpui::test]
    fn editing_the_new_passphrase_keeps_the_current_passphrase_error(cx: &mut TestAppContext) {
        let modal = prompt(cx, PassphrasePurpose::ScheduleChange);
        modal.update(cx, |modal, cx| modal.reject_current(cx));

        let passphrase = modal.read_with(cx, |modal, _| modal.passphrase.clone());
        passphrase.update(cx, |_, cx| cx.emit(InputEvent::Changed("n".into())));
        cx.run_until_parked();

        assert!(
            modal
                .read_with(cx, |modal, _| modal.shown_current_error())
                .is_some()
        );
    }

    #[test]
    fn passphrase_one_below_the_minimum_is_too_short_and_at_the_minimum_is_valid() {
        let below = chars('a', MIN_PASSPHRASE_CHARS - 1);
        let at = chars('a', MIN_PASSPHRASE_CHARS);

        let (below, at) = (
            PassphraseCheck::of(&below, &below),
            PassphraseCheck::of(&at, &at),
        );

        assert_eq!(
            (below.is_valid(), below.shows_too_short(), at.is_valid()),
            (false, true, true)
        );
    }

    #[test]
    fn passphrase_length_counts_cyrillic_characters_not_bytes() {
        let below = chars('ж', MIN_PASSPHRASE_CHARS - 1);
        let at = chars('ж', MIN_PASSPHRASE_CHARS);

        assert_eq!(
            (
                PassphraseCheck::of(&below, &below).is_valid(),
                PassphraseCheck::of(&at, &at).is_valid(),
            ),
            (false, true)
        );
    }

    #[test]
    fn mismatched_confirmation_blocks_confirm_and_shows_the_hint() {
        let passphrase = chars('a', MIN_PASSPHRASE_CHARS);
        let confirmation = chars('b', MIN_PASSPHRASE_CHARS);

        let check = PassphraseCheck::of(&passphrase, &confirmation);

        assert_eq!((check.is_valid(), check.shows_mismatch()), (false, true));
    }

    #[test]
    fn empty_confirmation_blocks_confirm_without_showing_the_mismatch_hint() {
        let check = PassphraseCheck::of(&chars('a', MIN_PASSPHRASE_CHARS), "");

        assert_eq!((check.is_valid(), check.shows_mismatch()), (false, false));
    }

    #[test]
    fn empty_passphrase_blocks_confirm_without_showing_the_too_short_hint() {
        let check = PassphraseCheck::of("", "");

        assert_eq!((check.is_valid(), check.shows_too_short()), (false, false));
    }
}
