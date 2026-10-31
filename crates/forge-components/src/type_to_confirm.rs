use crate::HOVER_FILL_ALPHA;
use crate::presentation::ActivePresentation;
use gpui::{
    App, AppContext, ClickEvent, Context, ElementId, Entity, EventEmitter, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, Rgba, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window, div, px,
};

use crate::buttons::secondary_button;
use crate::icons::{Icon, icon};
use crate::palette::{ForgePalette, with_alpha};
use crate::text_input::{InputEvent, TextInput};
use crate::tokens::{
    BORDER_THIN, Density, FONT_MD, FONT_SM, FONT_XS, Radius, Spacing, body_family, mono_family,
    radius, scaled, spacing,
};

const HEADER_GAP: Pixels = px(12.0);
const ICON_TILE: Pixels = px(36.0);
const ICON_TILE_GLYPH: Pixels = px(20.0);
const ICON_TILE_ALPHA: f32 = 0.12;
const SECTION_STACK_GAP: Pixels = px(8.0);
const BULLET_GLYPH: Pixels = px(14.0);
const ACTIONS_GAP: Pixels = px(8.0);
const CHIP_PAD_Y: Pixels = px(1.0);
const CARD_WIDTH: Pixels = px(520.0);

type ActionHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulletKind {
    Check,
    Warning,
    Info,
}

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

pub(crate) fn matches(typed: &str, target: &str) -> bool {
    typed == target
}

fn match_border(matched: bool, palette: &ForgePalette) -> Rgba {
    if matched {
        palette.brand
    } else {
        palette.border_input
    }
}

pub struct TypeToConfirm {
    input: Entity<TextInput>,
    target: SharedString,
    title: SharedString,
    explanation: SharedString,
    section_caption: SharedString,
    instruction_prefix: SharedString,
    instruction_suffix: SharedString,
    bullets: Vec<BulletItem>,
    confirm_label: SharedString,
    cancel_label: SharedString,
    palette: ForgePalette,
    matched: bool,
    _input_sub: Subscription,
}

impl EventEmitter<TypeToConfirmEvent> for TypeToConfirm {}

pub fn type_to_confirm(
    target: impl Into<SharedString>,
    palette: &ForgePalette,
    cx: &mut Context<TypeToConfirm>,
) -> TypeToConfirm {
    let target = target.into();
    let palette = *palette;
    let matched = matches("", target.as_ref());

    let input = cx.new(|cx| {
        TextInput::new("", cx)
            .with_palette(palette)
            .with_font_size(FONT_SM)
            .static_chrome(match_border(matched, &palette), Radius::Md)
    });

    let sub = cx.subscribe(&input, |this, input, event: &InputEvent, cx| match event {
        InputEvent::Changed(text) => {
            let now = matches(text.as_ref(), this.target.as_ref());
            if now != this.matched {
                this.matched = now;
                let border = match_border(now, &this.palette);
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
        section_caption: SharedString::default(),
        instruction_prefix: SharedString::default(),
        instruction_suffix: SharedString::default(),
        bullets: Vec::new(),
        confirm_label: SharedString::default(),
        cancel_label: SharedString::default(),
        palette,
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
    pub fn esc_hint(self, _phrase: impl Into<SharedString>) -> Self {
        self
    }

    pub fn focus_input(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |f, cx| f.focus(window, cx));
    }
}

fn confirm_button(
    id: ElementId,
    label: SharedString,
    palette: &ForgePalette,
    enabled: bool,
    handler: ActionHandler,
    density: Density,
) -> impl IntoElement {
    let accent = palette.warning;
    let ink = palette.shell;
    let disabled_bg = palette.surface_overlay;
    let disabled_ink = palette.disabled;

    let base = div()
        .flex()
        .items_center()
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Md))
        .font_family(body_family())
        .text_size(FONT_SM);

    if enabled {
        base.id(id)
            .bg(accent)
            .font_weight(FontWeight::MEDIUM)
            .text_color(ink)
            .cursor_pointer()
            .hover(move |style| style.bg(with_alpha(accent, HOVER_FILL_ALPHA)))
            .on_click(handler)
            .child(label)
            .into_any_element()
    } else {
        base.bg(disabled_bg)
            .text_color(disabled_ink)
            .child(label)
            .into_any_element()
    }
}

impl Render for TypeToConfirm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let p = self.palette;
        let entity = cx.entity();

        let tile = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(ICON_TILE)
            .rounded(radius(Radius::Md))
            .bg(with_alpha(p.warning, ICON_TILE_ALPHA))
            .child(icon(Icon::AlertTriangle, ICON_TILE_GLYPH, p.warning));

        let title_row = div()
            .flex()
            .items_center()
            .gap(scaled(HEADER_GAP, density))
            .child(tile)
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_MD)
                    .text_color(p.text_primary)
                    .child(self.title.clone()),
            );

        let header = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(scaled(SECTION_STACK_GAP, density))
            .py(spacing(Spacing::Md, density))
            .px(spacing(Spacing::Lg, density))
            .child(title_row)
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(p.text_muted)
                    .child(self.explanation.clone()),
            );

        let mut risk = div()
            .w_full()
            .flex()
            .flex_col()
            .py(spacing(Spacing::Md, density))
            .px(spacing(Spacing::Lg, density))
            .bg(p.shell)
            .border_t(BORDER_THIN)
            .border_color(p.border_regular);

        if !self.section_caption.is_empty() {
            risk = risk.child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(p.text_muted)
                    .child(self.section_caption.clone()),
            );
        }
        for item in &self.bullets {
            let (glyph, hue) = bullet_glyph(item.kind, &p);
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
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(p.text_primary)
                            .child(item.text.clone()),
                    ),
            );
        }

        let phrase_chip = div()
            .flex_none()
            .py(scaled(CHIP_PAD_Y, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .bg(p.surface_overlay)
            .font_family(mono_family())
            .text_size(FONT_SM)
            .text_color(p.warning)
            .child(self.target.clone());

        let instruction = div()
            .flex()
            .items_center()
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(p.text_primary)
                    .child(format!("{} ", self.instruction_prefix)),
            )
            .child(phrase_chip)
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(p.text_primary)
                    .child(format!(" {}", self.instruction_suffix)),
            );

        let confirm_band = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(scaled(SECTION_STACK_GAP, density))
            .py(spacing(Spacing::Md, density))
            .px(spacing(Spacing::Lg, density))
            .border_t(BORDER_THIN)
            .border_color(p.border_regular)
            .child(instruction)
            .child(self.input.clone());

        let cancel_entity = entity.clone();
        let cancel = secondary_button(self.cancel_label.clone(), &p).on_click(
            "type-to-confirm-cancel",
            move |_event, _window, app| {
                cancel_entity.update(app, |_this, cx| cx.emit(TypeToConfirmEvent::Cancelled));
            },
        );

        let confirm_entity = entity.clone();
        let confirm_handler: ActionHandler = Box::new(move |_event, _window, app| {
            confirm_entity.update(app, |_this, cx| cx.emit(TypeToConfirmEvent::Confirmed));
        });
        let confirm = confirm_button(
            "type-to-confirm-confirm".into(),
            self.confirm_label.clone(),
            &p,
            self.matched,
            confirm_handler,
            density,
        );

        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Lg, density))
            .bg(p.shell)
            .border_t(BORDER_THIN)
            .border_color(p.border_regular)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(scaled(ACTIONS_GAP, density))
                    .child(cancel)
                    .child(confirm),
            );

        div()
            .flex()
            .flex_col()
            .w(CARD_WIDTH)
            .bg(p.elevated)
            .rounded(radius(Radius::Lg))
            .overflow_hidden()
            .border(BORDER_THIN)
            .border_color(p.border_input)
            .child(header)
            .child(risk)
            .child(confirm_band)
            .child(footer)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{Entity, TestAppContext, VisualTestContext};

    use super::*;
    use crate::palette::FORGE_DEFAULT;
    use crate::test_support::install_presentation;
    use crate::text_input::bind_text_input_keys;

    const TARGET: &str = "bind to LAN";

    struct Card<'a> {
        view: Entity<TypeToConfirm>,
        seen: Rc<RefCell<Vec<TypeToConfirmEvent>>>,
        vcx: &'a mut VisualTestContext,
    }

    fn mount(cx: &mut TestAppContext) -> Card<'_> {
        install_presentation(cx);
        cx.update(bind_text_input_keys);
        let (view, vcx) =
            cx.add_window_view(|_window, cx| type_to_confirm(TARGET, &FORGE_DEFAULT, cx));
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
}
