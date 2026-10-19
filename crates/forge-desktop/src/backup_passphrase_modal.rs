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

pub enum BackupPassphraseEvent {
    Confirmed,
    Cancelled,
}

pub struct BackupPassphraseModal {
    passphrase: Entity<TextInput>,
    confirmation: Entity<TextInput>,
    focus_pending: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<BackupPassphraseEvent> for BackupPassphraseModal {}

impl BackupPassphraseModal {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
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
        let subs = vec![
            cx.subscribe(&passphrase, Self::on_field_event),
            cx.subscribe(&confirmation, Self::on_field_event),
        ];
        Self {
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
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Cancelled => self.cancel(cx),
            InputEvent::Submitted(_) => self.confirm(cx),
            InputEvent::Changed(_) => cx.notify(),
            InputEvent::Blurred(_) => {}
        }
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        if self.check(cx).is_valid() {
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
                    .child(tr!("settings_storage_passphrase_warning")),
            )
            .into_any_element()
    }

    fn footer(
        &self,
        check: PassphraseCheck,
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
                primary_button(tr!("settings_storage_passphrase_confirm_btn"), palette)
                    .disabled(!check.is_valid())
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
            self.passphrase
                .update(cx, |input, cx| input.focus(window, cx));
        }
        let check = self.check(cx);

        let too_short = tr!(
            "settings_storage_passphrase_too_short",
            min = MIN_PASSPHRASE_CHARS.to_string()
        );
        let mismatch = tr!("settings_storage_passphrase_mismatch");
        let hint = tr!(
            "settings_storage_passphrase_hint",
            min = MIN_PASSPHRASE_CHARS.to_string()
        );

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(self.warning(&palette, density))
            .child(field_column(
                &palette,
                &tr!("settings_storage_passphrase_label"),
                Some(hint.as_str()),
                check.shows_too_short().then_some(too_short.as_str()),
                field_frame(&palette)
                    .child(self.passphrase.clone())
                    .into_any_element(),
            ))
            .child(field_column(
                &palette,
                &tr!("settings_storage_passphrase_confirm_label"),
                None,
                check.shows_mismatch().then_some(mismatch.as_str()),
                field_frame(&palette)
                    .child(self.confirmation.clone())
                    .into_any_element(),
            ));

        let card = modal(tr!("settings_storage_passphrase_title"), body, &palette)
            .header_icon(Icon::Lock, palette.brand)
            .subtitle(tr!("settings_storage_passphrase_subtitle"))
            .width(modal_width(ModalSize::Sm))
            .footer(self.footer(check, &palette, density, cx))
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
mod tests {
    use super::*;

    fn chars(c: char, n: usize) -> String {
        std::iter::repeat_n(c, n).collect()
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
