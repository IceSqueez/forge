use crate::presentation::ActivePresentation;
use gpui::{
    App, AppContext, Context, Div, Entity, EventEmitter, FontWeight, IntoElement, ParentElement,
    Pixels, Render, Rgba, SharedString, Styled, Subscription, Window, div, px,
};

use crate::buttons::secondary_button;
use crate::confirm::{HEADER_GAP, confirm_action, confirm_actions_row, confirm_footer};
use crate::icons::{Icon, icon};
use crate::modal::{alert_tile, bare_modal};
use crate::palette::{ForgePalette, PaletteColor};
use crate::text_input::{InputEvent, TextInput};
use crate::tokens::{
    BORDER_THIN, Density, FONT_MD, FONT_SM, Radius, Spacing, radius, scaled, spacing,
};

const ICON_TILE_GLYPH: Pixels = px(20.0);
const SECTION_STACK_GAP: Pixels = px(8.0);
const BULLET_GLYPH: Pixels = px(14.0);
const CHIP_PAD_Y: Pixels = px(1.0);
const CARD_WIDTH: Pixels = px(520.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulletKind {
    Check,
    Warning,
    Info,
}

#[derive(Debug)]
pub struct BulletItem {
    pub kind: BulletKind,
    pub text: SharedString,
}

impl BulletItem {
    pub fn new(kind: BulletKind, text: impl Into<SharedString>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeToConfirmEvent {
    Confirmed,
    Cancelled,
}

pub(crate) fn bullet_glyph(kind: BulletKind, palette: &ForgePalette) -> (Icon, Rgba) {
    match kind {
        BulletKind::Check => (Icon::CircleCheck, palette.success),
        BulletKind::Warning => (Icon::AlertTriangle, palette.warning),
        BulletKind::Info => (Icon::InfoCircle, palette.info),
    }
}

fn match_border(matched: bool) -> PaletteColor {
    if matched {
        |palette| palette.brand
    } else {
        |palette| palette.border_input
    }
}

pub struct TypeToConfirm {
    input: Entity<TextInput>,
    target: SharedString,
    title: SharedString,
    explanation: SharedString,
    instruction_prefix: SharedString,
    instruction_suffix: SharedString,
    bullets: Vec<BulletItem>,
    confirm_label: SharedString,
    cancel_label: SharedString,
    esc_hint: Option<SharedString>,
    matched: bool,
    _input_sub: Subscription,
}

impl EventEmitter<TypeToConfirmEvent> for TypeToConfirm {}

pub fn type_to_confirm(
    target: impl Into<SharedString>,
    cx: &mut Context<TypeToConfirm>,
) -> TypeToConfirm {
    let target = target.into();
    let matched = target.is_empty();

    let input = cx.new(|cx| {
        TextInput::new("", cx)
            .with_font_size(FONT_SM)
            .static_chrome(match_border(matched), Radius::Md)
    });

    let sub = cx.subscribe(&input, |this, input, event: &InputEvent, cx| match event {
        InputEvent::Changed(text) => {
            let now = text.as_ref() == this.target.as_ref();
            if now != this.matched {
                this.matched = now;
                let border = match_border(now);
                input.update(cx, |inp, cx| {
                    inp.set_static_chrome(Some((border, Radius::Md)));
                    cx.notify();
                });
                cx.notify();
            }
        }
        InputEvent::Submitted(_) => {
            if this.matched {
                cx.emit(TypeToConfirmEvent::Confirmed);
            }
        }
        InputEvent::Cancelled => cx.emit(TypeToConfirmEvent::Cancelled),
        InputEvent::Blurred(_) => {}
    });

    TypeToConfirm {
        input,
        target,
        title: SharedString::default(),
        explanation: SharedString::default(),
        instruction_prefix: SharedString::default(),
        instruction_suffix: SharedString::default(),
        bullets: Vec::new(),
        confirm_label: SharedString::default(),
        cancel_label: SharedString::default(),
        esc_hint: None,
        matched,
        _input_sub: sub,
    }
}

impl TypeToConfirm {
    #[must_use]
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }

    #[must_use]
    pub fn explanation(mut self, explanation: impl Into<SharedString>) -> Self {
        self.explanation = explanation.into();
        self
    }

    #[must_use]
    pub fn instruction(
        mut self,
        prefix: impl Into<SharedString>,
        suffix: impl Into<SharedString>,
    ) -> Self {
        self.instruction_prefix = prefix.into();
        self.instruction_suffix = suffix.into();
        self
    }

    #[must_use]
    pub fn bullets(mut self, bullets: Vec<BulletItem>) -> Self {
        self.bullets = bullets;
        self
    }

    #[must_use]
    pub fn confirm_label(mut self, label: impl Into<SharedString>) -> Self {
        self.confirm_label = label.into();
        self
    }

    #[must_use]
    pub fn cancel_label(mut self, label: impl Into<SharedString>) -> Self {
        self.cancel_label = label.into();
        self
    }

    #[must_use]
    pub fn esc_hint(mut self, phrase: impl Into<SharedString>) -> Self {
        self.esc_hint = Some(phrase.into());
        self
    }

    pub fn focus_input(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |f, cx| f.focus(window, cx));
    }
}

impl TypeToConfirm {
    fn header_section(&self, density: Density, p: &ForgePalette, cx: &App) -> Div {
        let tile = alert_tile(p.warning, ICON_TILE_GLYPH);

        let title_row = div()
            .flex()
            .items_center()
            .gap(scaled(HEADER_GAP, density))
            .child(tile)
            .child(
                div()
                    .font_family(cx.body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_MD)
                    .text_color(p.text_primary)
                    .child(self.title.clone()),
            );

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(scaled(SECTION_STACK_GAP, density))
            .py(spacing(Spacing::Md, density))
            .px(spacing(Spacing::Lg, density))
            .child(title_row)
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_SM)
                    .text_color(p.text_muted)
                    .child(self.explanation.clone()),
            )
    }

    fn risk_section(&self, density: Density, p: &ForgePalette, cx: &App) -> Div {
        let mut risk = div()
            .w_full()
            .flex()
            .flex_col()
            .py(spacing(Spacing::Md, density))
            .px(spacing(Spacing::Lg, density))
            .bg(p.shell)
            .border_t(BORDER_THIN)
            .border_color(p.border_regular);

        for item in &self.bullets {
            let (glyph, hue) = bullet_glyph(item.kind, p);
            risk = risk.child(
                div()
                    .flex()
                    .items_start()
                    .gap(spacing(Spacing::Sm, density))
                    .py(spacing(Spacing::Xxs, density))
                    .child(icon(glyph, BULLET_GLYPH, hue))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(cx.body_family())
                            .text_size(FONT_SM)
                            .text_color(p.text_primary)
                            .child(item.text.clone()),
                    ),
            );
        }
        risk
    }

    fn confirm_band(&self, density: Density, p: &ForgePalette, cx: &App) -> Div {
        let phrase_chip = div()
            .flex_none()
            .py(scaled(CHIP_PAD_Y, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .bg(p.surface_overlay)
            .font_family(cx.mono_family())
            .text_size(FONT_SM)
            .text_color(p.warning)
            .child(self.target.clone());

        let instruction = div()
            .flex()
            .items_center()
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_SM)
                    .text_color(p.text_primary)
                    .child(format!("{} ", self.instruction_prefix)),
            )
            .child(phrase_chip)
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_SM)
                    .text_color(p.text_primary)
                    .child(format!(" {}", self.instruction_suffix)),
            );

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(scaled(SECTION_STACK_GAP, density))
            .py(spacing(Spacing::Md, density))
            .px(spacing(Spacing::Lg, density))
            .border_t(BORDER_THIN)
            .border_color(p.border_regular)
            .child(instruction)
            .child(self.input.clone())
    }

    fn footer(&self, density: Density, p: &ForgePalette, cx: &mut Context<Self>) -> Div {
        let entity = cx.entity();
        let cancel_entity = entity.clone();
        let cancel = secondary_button(self.cancel_label.clone(), p)
            .fits_parent()
            .on_click("type-to-confirm-cancel", move |_event, _window, app| {
                cancel_entity.update(app, |_this, cx| cx.emit(TypeToConfirmEvent::Cancelled));
            });

        let confirm_entity = entity;
        let confirm = confirm_action(self.confirm_label.clone(), p.warning, p, density)
            .corner(Radius::Md)
            .weight(if self.matched {
                FontWeight::MEDIUM
            } else {
                FontWeight::NORMAL
            })
            .disabled_tones(p.surface_overlay, p.disabled)
            .focus_ring(p.text_primary)
            .disabled(!self.matched)
            .on_click("type-to-confirm-confirm", move |_event, _window, app| {
                confirm_entity.update(app, |_this, cx| cx.emit(TypeToConfirmEvent::Confirmed));
            });

        let actions = confirm_actions_row(density).child(cancel).child(confirm);
        confirm_footer(self.esc_hint.clone(), actions, p, density, cx).bg(p.shell)
    }
}

impl Render for TypeToConfirm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let p = cx.palette();
        let footer = self.footer(density, &p, cx);

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .child(self.header_section(density, &p, cx))
            .child(self.risk_section(density, &p, cx))
            .child(self.confirm_band(density, &p, cx));

        bare_modal(body, &p)
            .width(CARD_WIDTH)
            .frame_border(p.border_input)
            .keeps_focus_on_press()
            .bare_footer(footer)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{
        Bounds, Entity, InteractiveElement, Modifiers, Point, TestAppContext, VisualTestContext,
        point,
    };

    use super::*;
    use crate::test_support::install_presentation;
    use crate::text_input::bind_text_input_keys;

    const TARGET: &str = "bind to LAN";
    const CARD_SLOT: &str = "type-to-confirm-test-card";
    const CANCEL: &str = "type-to-confirm-cancel";
    const LONG_LABEL: &str = "Keep the server bound to this machine";
    const PAST_THE_CANCEL_BUTTON: Pixels = px(24.0);

    struct Card<'a> {
        view: Entity<TypeToConfirm>,
        seen: Rc<RefCell<Vec<TypeToConfirmEvent>>>,
        vcx: &'a mut VisualTestContext,
    }

    fn mount(cx: &mut TestAppContext) -> Card<'_> {
        install_presentation(cx);
        cx.update(bind_text_input_keys);
        let (view, vcx) = cx.add_window_view(|_window, cx| {
            type_to_confirm(TARGET, cx)
                .cancel_label("Keep it local")
                .confirm_label("Open it to the network")
        });
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        vcx.update(|window, cx| {
            window.activate_window();
            cx.subscribe(&view, move |_card, event: &TypeToConfirmEvent, _cx| {
                sink.borrow_mut().push(*event);
            })
            .detach();
            view.update(cx, |card, cx| card.focus_input(window, cx));
        });
        vcx.run_until_parked();
        Card { view, seen, vcx }
    }

    impl Card<'_> {
        fn type_text(&mut self, text: &str) {
            self.vcx.simulate_input(text);
            self.vcx.run_until_parked();
        }

        fn press(&mut self, key: &str) {
            self.vcx.simulate_keystrokes(key);
            self.vcx.run_until_parked();
        }

        fn click(&mut self, position: Point<Pixels>) {
            self.vcx.simulate_click(position, Modifiers::default());
            self.vcx.run_until_parked();
        }

        fn cancel_button(&mut self) -> Bounds<Pixels> {
            self.vcx.debug_bounds(CANCEL).expect("the button paints")
        }

        fn confirm_button_center(&mut self) -> Point<Pixels> {
            let cancel = self.cancel_button();
            point(cancel.right() + PAST_THE_CANCEL_BUTTON, cancel.center().y)
        }

        fn matched(&mut self) -> bool {
            self.view.update(self.vcx, |card, _cx| card.matched)
        }

        fn typed(&mut self) -> String {
            self.view.update(self.vcx, |card, cx| {
                card.input.read(cx).content().to_owned()
            })
        }
    }

    #[gpui::test]
    fn enter_confirms_only_the_exact_text_and_only_once(cx: &mut TestAppContext) {
        let mut card = mount(cx);
        for near_miss in ["bind to LA", "Bind to LAN", "bind to LAN ", " bind to LAN"] {
            card.press("ctrl-a");
            card.type_text(near_miss);

            card.press("enter");

            assert_eq!(card.typed(), near_miss);
            assert!(!card.matched(), "{near_miss:?}");
            assert!(card.seen.borrow().is_empty(), "{near_miss:?}");
        }

        card.press("ctrl-a");
        card.type_text(TARGET);
        card.press("enter");

        assert_eq!(*card.seen.borrow(), [TypeToConfirmEvent::Confirmed]);
    }

    #[gpui::test]
    fn erasing_after_a_match_closes_the_gate_again(cx: &mut TestAppContext) {
        let mut card = mount(cx);
        card.type_text(TARGET);

        card.press("backspace");
        card.press("enter");

        assert!(!card.matched());
        assert!(card.seen.borrow().is_empty());

        card.type_text("N");
        card.press("enter");

        assert_eq!(*card.seen.borrow(), [TypeToConfirmEvent::Confirmed]);
    }

    #[gpui::test]
    fn the_confirm_button_is_inert_until_the_exact_text_is_typed(cx: &mut TestAppContext) {
        let mut card = mount(cx);
        let confirm = card.confirm_button_center();

        card.click(confirm);
        card.type_text("bind to LA");
        card.click(confirm);
        assert!(card.seen.borrow().is_empty());

        card.type_text("N");
        card.click(confirm);

        assert_eq!(*card.seen.borrow(), [TypeToConfirmEvent::Confirmed]);
    }

    #[gpui::test]
    fn the_cancel_button_cancels_whatever_was_typed(cx: &mut TestAppContext) {
        let mut card = mount(cx);
        card.type_text(TARGET);

        let cancel = card.cancel_button().center();
        card.click(cancel);

        assert_eq!(*card.seen.borrow(), [TypeToConfirmEvent::Cancelled]);
    }

    #[gpui::test]
    fn escape_on_an_empty_field_cancels(cx: &mut TestAppContext) {
        let mut card = mount(cx);

        card.press("escape");

        assert_eq!(*card.seen.borrow(), [TypeToConfirmEvent::Cancelled]);
    }

    #[gpui::test]
    fn escape_cancels_even_when_the_text_matches(cx: &mut TestAppContext) {
        let mut card = mount(cx);
        card.type_text(TARGET);

        card.press("escape");

        assert_eq!(*card.seen.borrow(), [TypeToConfirmEvent::Cancelled]);
    }

    struct LongLabelCard {
        card: Entity<TypeToConfirm>,
    }

    impl Render for LongLabelCard {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().flex().items_start().child(
                div()
                    .debug_selector(|| CARD_SLOT.to_owned())
                    .child(self.card.clone()),
            )
        }
    }

    #[gpui::test]
    fn labels_too_long_for_one_line_stay_inside_the_card(cx: &mut TestAppContext) {
        install_presentation(cx);
        cx.update(bind_text_input_keys);
        let (_, vcx) = cx.add_window_view(|_, cx| LongLabelCard {
            card: cx.new(|cx| {
                type_to_confirm(TARGET, cx)
                    .cancel_label(LONG_LABEL)
                    .confirm_label(LONG_LABEL)
                    .esc_hint("esc to keep the current binding")
            }),
        });
        vcx.run_until_parked();

        let card = vcx.debug_bounds(CARD_SLOT).expect("the card paints");
        let cancel = vcx.debug_bounds(CANCEL).expect("the button paints");

        assert!(
            cancel.left() >= card.left() && cancel.right() <= card.right(),
            "{cancel:?} escapes {card:?}"
        );
    }
}
