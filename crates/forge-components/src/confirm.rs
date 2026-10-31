use crate::HOVER_FILL_ALPHA;
use crate::key_activation::KeyActivated;
use crate::presentation::ActivePresentation;
use std::rc::Rc;
use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, ClickEvent, ElementId, FocusHandle, FontWeight, InteractiveElement, IntoElement,
    KeyBinding, KeyDownEvent, KeyboardClickEvent, NoAction, ParentElement, Pixels, RenderOnce,
    Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::buttons::secondary_button;
use crate::icons::{Icon, icon};
use crate::overlay::{ModalFocusClaim, confine_tab, modal_focus};
use crate::palette::{ForgePalette, with_alpha};
use crate::tokens::{
    BORDER_THIN, Density, ENTER_KEY, FONT_MD, FONT_META, FONT_SM, FONT_XS, ModalSize, Radius,
    Spacing, body_family, modal_width, mono_family, radius, scaled, spacing,
};

const HEADER_GAP: Pixels = px(12.0);
const ICON_TILE: Pixels = px(36.0);
const ICON_TILE_GLYPH: Pixels = px(18.0);
const ICON_TILE_ALPHA: f32 = 0.12;
const ACTIONS_GAP: Pixels = px(8.0);
const ESC_HINT_GAP: Pixels = px(5.0);
const ESC_HINT_GLYPH: Pixels = px(12.0);
const ESC_KEY_LABEL: &str = "Esc";
const KEY_CONTEXT: &str = "ForgeConfirm";
const ENTER_FOCUS_KEY: &str = "enter-confirms";

type ActionHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
type SharedActionHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub fn bind_confirm_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(ENTER_KEY, NoAction, Some(KEY_CONTEXT))]);
}

pub(crate) fn esc_hint_label(phrase: SharedString, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(ESC_HINT_GAP)
        .font_family(body_family())
        .text_size(FONT_META)
        .text_color(palette.text_faint)
        .child(icon(Icon::Keyboard, ESC_HINT_GLYPH, palette.text_faint))
        .child(div().font_family(mono_family()).child(ESC_KEY_LABEL))
        .child(phrase)
}

fn keyboard_click() -> ClickEvent {
    ClickEvent::Keyboard(KeyboardClickEvent::default())
}

fn plain_enter_press(event: &KeyDownEvent) -> bool {
    event.keystroke.key.as_str() == ENTER_KEY
        && !event.is_held
        && !event.keystroke.modifiers.modified()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmTone {
    Destructive,
    Warning,
}

impl ConfirmTone {
    pub(crate) fn accent(self, palette: &ForgePalette) -> Rgba {
        match self {
            ConfirmTone::Destructive => palette.danger,
            ConfirmTone::Warning => palette.warning,
        }
    }
}

#[derive(IntoElement)]
pub struct ConfirmModal {
    title: SharedString,
    message: SharedString,
    item_name: Option<SharedString>,
    esc_hint: Option<SharedString>,
    tone: ConfirmTone,
    palette: ForgePalette,
    confirm: Option<(ElementId, SharedString, ActionHandler)>,
    alternate: Option<(ElementId, SharedString, ActionHandler)>,
    cancel: Option<(ElementId, SharedString, ActionHandler)>,
}

pub fn confirm_modal(
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    tone: ConfirmTone,
    palette: &ForgePalette,
) -> ConfirmModal {
    ConfirmModal {
        title: title.into(),
        message: message.into(),
        item_name: None,
        esc_hint: None,
        tone,
        palette: *palette,
        confirm: None,
        alternate: None,
        cancel: None,
    }
}

impl ConfirmModal {
    #[must_use]
    pub fn item_name(mut self, name: impl Into<SharedString>) -> Self {
        self.item_name = Some(name.into());
        self
    }

    #[must_use]
    pub fn esc_hint(mut self, phrase: impl Into<SharedString>) -> Self {
        self.esc_hint = Some(phrase.into());
        self
    }

    #[must_use]
    pub fn on_confirm(
        mut self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.confirm = Some((id.into(), label.into(), Box::new(handler)));
        self
    }

    #[must_use]
    pub fn on_alternate(
        mut self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.alternate = Some((id.into(), label.into(), Box::new(handler)));
        self
    }

    #[must_use]
    pub fn on_cancel(
        mut self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.cancel = Some((id.into(), label.into(), Box::new(handler)));
        self
    }
}

fn accent_confirm_button(
    id: ElementId,
    label: SharedString,
    accent: Rgba,
    ink: Rgba,
    handler: SharedActionHandler,
    density: Density,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .bg(accent)
        .font_family(body_family())
        .text_size(FONT_SM)
        .text_color(ink)
        .cursor_pointer()
        .hover(move |style| style.bg(with_alpha(accent, HOVER_FILL_ALPHA)))
        .key_activated(ink)
        .on_click(move |event, window, cx| handler(event, window, cx))
        .child(label)
}

fn enter_focus(id: &ElementId, window: &mut Window, cx: &mut App) -> FocusHandle {
    modal_focus(
        ElementId::NamedChild(
            Arc::new(id.clone()),
            SharedString::new_static(ENTER_FOCUS_KEY),
        ),
        ModalFocusClaim::AfterThisFrame,
        window,
        cx,
    )
}

impl RenderOnce for ConfirmModal {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let density = cx.density();
        let ConfirmModal {
            title,
            message,
            item_name,
            esc_hint,
            tone,
            palette,
            confirm,
            alternate,
            cancel,
        } = self;

        let accent = tone.accent(&palette);

        let tile = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(ICON_TILE)
            .rounded(radius(Radius::Md))
            .bg(with_alpha(accent, ICON_TILE_ALPHA))
            .child(icon(Icon::AlertTriangle, ICON_TILE_GLYPH, accent));

        let mut titles = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_MD)
                    .text_color(palette.text_primary)
                    .child(title),
            );
        if let Some(name) = item_name {
            titles = titles.child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(name),
            );
        }

        let header = div()
            .flex()
            .items_center()
            .gap(scaled(HEADER_GAP, density))
            .child(tile)
            .child(titles);

        let hint = div()
            .font_family(body_family())
            .text_size(FONT_SM)
            .text_color(palette.text_muted)
            .child(message);

        let body = div()
            .w_full()
            .p(spacing(Spacing::Lg, density))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(header)
            .child(hint);

        let has_actions = cancel.is_some() || alternate.is_some() || confirm.is_some();
        let mut actions = div()
            .flex()
            .items_center()
            .gap(scaled(ACTIONS_GAP, density));
        for (id, label, handler) in [cancel, alternate].into_iter().flatten() {
            actions = actions.child(secondary_button(label, &palette).on_click(id, handler));
        }
        let mut enter_binding = None;
        if let Some((id, label, handler)) = confirm {
            let handler: SharedActionHandler = Rc::from(handler);
            enter_binding = Some((enter_focus(&id, window, cx), Rc::clone(&handler)));
            actions = actions.child(accent_confirm_button(
                id,
                label,
                accent,
                palette.shell,
                handler,
                density,
            ));
        }

        let mut card = div()
            .flex()
            .flex_col()
            .w(modal_width(ModalSize::Sm))
            .bg(palette.elevated)
            .rounded(radius(Radius::Lg))
            .overflow_hidden()
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(body);

        if has_actions {
            card = card.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .when_some(esc_hint, |footer, phrase| {
                        footer.child(esc_hint_label(phrase, &palette))
                    })
                    .child(div().flex_1())
                    .py(spacing(Spacing::Sm, density))
                    .px(spacing(Spacing::Lg, density))
                    .border_t(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .child(actions),
            );
        }

        card.when_some(enter_binding, |card, (focus, confirm)| {
            let enter_scope = focus.clone();
            confine_tab(card.key_context(KEY_CONTEXT).track_focus(&focus), &focus).on_key_down(
                move |event: &KeyDownEvent, window, cx| {
                    if plain_enter_press(event) && enter_scope.is_focused(window) {
                        cx.stop_propagation();
                        confirm(&keyboard_click(), window, cx);
                    }
                },
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use gpui::{
        Bounds, Context, Entity, Keystroke, Render, TestAppContext, VisualTestContext, actions,
    };

    use super::*;
    use crate::overlay::overlay;
    use crate::palette::FORGE_DEFAULT;
    use crate::test_support::install_presentation;

    actions!(confirm_tests, [Activate]);

    const LIST_CONTEXT: &str = "ConfirmTestList";
    const CARD_SLOT: &str = "confirm-test-card";
    const TALL_HINT_LINE: &str = "esc to cancel\n";
    const TALL_HINT_LINES: usize = 12;

    fn assert_same_hue(got: Rgba, want: Rgba) {
        assert_eq!(got.r, want.r, "red channel");
        assert_eq!(got.g, want.g, "green channel");
        assert_eq!(got.b, want.b, "blue channel");
        assert_eq!(got.a, want.a, "alpha channel");
    }

    #[test]
    fn tone_resolves_to_its_severity_palette_field() {
        let p = &FORGE_DEFAULT;
        for (tone, want) in [
            (ConfirmTone::Destructive, p.danger),
            (ConfirmTone::Warning, p.warning),
        ] {
            assert_same_hue(tone.accent(p), want);
        }
    }

    #[test]
    fn destructive_and_warning_are_distinct_hues() {
        let p = &FORGE_DEFAULT;
        assert_ne!(
            ConfirmTone::Destructive.accent(p),
            ConfirmTone::Warning.accent(p),
        );
    }

    #[derive(Default)]
    struct Presses {
        confirmed: Cell<usize>,
        activated: Cell<usize>,
        dismissed: Cell<usize>,
    }

    #[derive(Clone)]
    struct CardShape {
        with_confirm: bool,
        with_cancel: bool,
        hint: Option<String>,
    }

    const CONFIRMING: CardShape = CardShape {
        with_confirm: true,
        with_cancel: true,
        hint: None,
    };

    const CANCEL_ONLY: CardShape = CardShape {
        with_confirm: false,
        with_cancel: true,
        hint: None,
    };

    struct List {
        focus: FocusHandle,
        shape: CardShape,
        open: bool,
        presses: Rc<Presses>,
    }

    impl List {
        fn card(&self) -> ConfirmModal {
            let mut card =
                confirm_modal("Title", "Message", ConfirmTone::Destructive, &FORGE_DEFAULT);
            if let Some(hint) = self.shape.hint.clone() {
                card = card.esc_hint(hint);
            }
            if self.shape.with_cancel {
                card = card.on_cancel("cancel", "Cancel", |_, _, _| {});
            }
            if self.shape.with_confirm {
                let presses = Rc::clone(&self.presses);
                card = card.on_confirm("confirm", "Delete", move |_, _, _| {
                    presses.confirmed.set(presses.confirmed.get() + 1);
                });
            }
            card
        }
    }

    impl Render for List {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let activated = Rc::clone(&self.presses);
            let dismissed = Rc::clone(&self.presses);
            let view = cx.entity();
            div()
                .size_full()
                .key_context(LIST_CONTEXT)
                .track_focus(&self.focus)
                .on_action(move |_: &Activate, _, _| {
                    activated.activated.set(activated.activated.get() + 1);
                })
                .when(self.open, |list| {
                    list.child(overlay(self.card(), &FORGE_DEFAULT).on_dismiss(
                        "confirm-test-overlay",
                        move |_, cx| {
                            dismissed.dismissed.set(dismissed.dismissed.get() + 1);
                            view.update(cx, |list, cx| {
                                list.open = false;
                                cx.notify();
                            });
                        },
                    ))
                })
        }
    }

    struct Rig<'a> {
        list: Entity<List>,
        presses: Rc<Presses>,
        vcx: &'a mut VisualTestContext,
    }

    fn open(cx: &mut TestAppContext, shape: CardShape) -> Rig<'_> {
        install_presentation(cx);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new(ENTER_KEY, Activate, Some(LIST_CONTEXT))]);
            bind_confirm_keys(cx);
        });
        let presses = Rc::new(Presses::default());
        let shared = Rc::clone(&presses);
        let (list, vcx) = cx.add_window_view(move |_, cx| List {
            focus: cx.focus_handle(),
            shape,
            open: false,
            presses: shared,
        });
        vcx.update(|window, cx| {
            let focus = list.read(cx).focus.clone();
            window.focus(&focus, cx);
        });
        vcx.run_until_parked();
        let mut rig = Rig { list, presses, vcx };
        rig.set_open(true);
        rig
    }

    impl Rig<'_> {
        fn set_open(&mut self, open: bool) {
            self.list.update(self.vcx, |list, cx| {
                list.open = open;
                cx.notify();
            });
            self.vcx.run_until_parked();
        }

        fn press(&mut self, keys: &str) {
            self.vcx.simulate_keystrokes(keys);
            self.vcx.run_until_parked();
        }

        fn counts(&self) -> (usize, usize) {
            (self.presses.confirmed.get(), self.presses.activated.get())
        }
    }

    #[gpui::test]
    fn enter_inside_a_list_confirms_once_and_never_activates_the_list(cx: &mut TestAppContext) {
        let mut rig = open(cx, CONFIRMING);

        rig.press("enter");

        assert_eq!(rig.counts(), (1, 0));
    }

    #[gpui::test]
    fn enter_on_the_tabbed_cancel_button_does_not_confirm(cx: &mut TestAppContext) {
        let mut rig = open(cx, CONFIRMING);
        rig.vcx
            .update(|_, cx| crate::key_activation::bind_key_activation(cx));
        rig.vcx.update(|window, cx| window.focus_next(cx));
        rig.vcx.run_until_parked();

        rig.press("enter");

        assert_eq!(rig.counts(), (0, 0));
    }

    #[gpui::test]
    fn the_list_binding_is_back_once_the_card_has_closed(cx: &mut TestAppContext) {
        let mut rig = open(cx, CONFIRMING);
        rig.press("enter");
        rig.set_open(false);
        rig.vcx.update(|_, _| {});
        rig.vcx.run_until_parked();

        rig.press("enter");

        assert_eq!(rig.counts(), (1, 1));
    }

    #[gpui::test]
    fn a_reopened_card_takes_the_keyboard_again(cx: &mut TestAppContext) {
        let mut rig = open(cx, CONFIRMING);
        rig.press("enter");
        rig.set_open(false);
        rig.set_open(true);

        rig.press("enter");

        assert_eq!(rig.counts(), (2, 0));
    }

    #[gpui::test]
    fn a_card_without_a_confirm_action_leaves_enter_to_the_list(cx: &mut TestAppContext) {
        let mut rig = open(cx, CANCEL_ONLY);

        rig.press("enter");

        assert_eq!(rig.counts(), (0, 1));
    }

    #[gpui::test]
    fn enter_with_any_modifier_does_not_confirm(cx: &mut TestAppContext) {
        let mut rig = open(cx, CONFIRMING);

        for chord in ["shift-enter", "ctrl-enter", "alt-enter", "cmd-enter"] {
            rig.press(chord);
        }

        assert_eq!(rig.presses.confirmed.get(), 0);
    }

    #[gpui::test]
    fn an_enter_still_held_from_before_the_card_opened_does_not_confirm(cx: &mut TestAppContext) {
        let rig = open(cx, CONFIRMING);

        rig.vcx.simulate_event(KeyDownEvent {
            keystroke: Keystroke::parse(ENTER_KEY).unwrap(),
            is_held: true,
            prefer_character_input: false,
        });
        rig.vcx.run_until_parked();

        assert_eq!(rig.counts(), (0, 0));
    }

    #[gpui::test]
    fn another_key_does_not_confirm(cx: &mut TestAppContext) {
        let mut rig = open(cx, CONFIRMING);

        rig.press("space");

        assert_eq!(rig.counts(), (0, 0));
    }

    #[gpui::test]
    fn escape_dismisses_the_overlay_around_a_focused_card_without_confirming(
        cx: &mut TestAppContext,
    ) {
        let mut rig = open(cx, CONFIRMING);

        rig.press("escape");

        assert_eq!(rig.presses.dismissed.get(), 1);
        assert_eq!(rig.presses.confirmed.get(), 0);
    }

    struct Slot {
        card: fn() -> ConfirmModal,
    }

    impl Render for Slot {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full().flex().items_start().child(
                div()
                    .debug_selector(|| CARD_SLOT.to_owned())
                    .child((self.card)()),
            )
        }
    }

    fn rendered_height(cx: &mut TestAppContext, card: fn() -> ConfirmModal) -> Pixels {
        install_presentation(cx);
        let (_, vcx) = cx.add_window_view(move |_, _| Slot { card });
        vcx.run_until_parked();
        let bounds: Bounds<Pixels> = vcx.debug_bounds(CARD_SLOT).expect("the card slot paints");
        bounds.size.height
    }

    fn tall_hint() -> String {
        TALL_HINT_LINE.repeat(TALL_HINT_LINES)
    }

    fn bare_card() -> ConfirmModal {
        confirm_modal("Title", "Message", ConfirmTone::Warning, &FORGE_DEFAULT)
    }

    fn footed_card() -> ConfirmModal {
        bare_card().on_confirm("confirm", "Delete", |_, _, _| {})
    }

    #[gpui::test]
    fn the_esc_hint_takes_room_in_the_footer(cx: &mut TestAppContext) {
        let plain = rendered_height(cx, footed_card);
        let hinted = rendered_height(cx, || footed_card().esc_hint(tall_hint()));

        assert!(hinted > plain, "{hinted:?} <= {plain:?}");
    }

    #[gpui::test]
    fn the_esc_hint_alone_does_not_add_a_footer_to_a_card_without_actions(cx: &mut TestAppContext) {
        let plain = rendered_height(cx, bare_card);
        let hinted = rendered_height(cx, || bare_card().esc_hint(tall_hint()));

        assert_eq!(plain, hinted);
    }
}
