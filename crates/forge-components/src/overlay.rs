use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    Anchor, AnchoredPositionMode, Animation, AnimationExt, AnyElement, App, Div, ElementId, Entity,
    FocusHandle, Global, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, PlatformInput, Point, RenderOnce, Rgba,
    ScrollDelta, ScrollEvent, SharedString, Size, Styled, WeakFocusHandle, Window, anchored,
    deferred, div, point, px,
};

use crate::palette::{ForgePalette, with_alpha};
use crate::text_input::{FocusNextField, FocusPreviousField};
use crate::tokens::ESCAPE_KEY;

pub fn drive_overlay_focus(
    open: bool,
    handle: &FocusHandle,
    restore: &mut Option<FocusHandle>,
    window: &mut Window,
    cx: &mut App,
) {
    if open {
        if !handle.is_focused(window) {
            *restore = window.focused(cx);
            window.focus(handle, cx);
        }
    } else if let Some(prev) = restore.take()
        && handle.is_focused(window)
    {
        window.focus(&prev, cx);
    }
}

struct FocusEntrusted {
    entered: WeakFocusHandle,
    prior: WeakFocusHandle,
}

#[derive(Default)]
struct PendingFocusReturn(Option<FocusEntrusted>);

impl Global for PendingFocusReturn {}

pub fn focus_inside_modal(
    window: &mut Window,
    cx: &mut App,
    focus: impl FnOnce(&mut Window, &mut App),
) {
    let prior = window.focused(cx);
    focus(window, cx);
    if let (Some(prior), Some(entered)) = (prior, window.focused(cx)) {
        cx.default_global::<PendingFocusReturn>().0 = Some(FocusEntrusted {
            entered: entered.downgrade(),
            prior: prior.downgrade(),
        });
    }
}

fn take_entrusted_focus(window: &Window, cx: &mut App) -> Option<FocusEntrusted> {
    let focused = window.focused(cx)?;
    let pending = &mut cx.default_global::<PendingFocusReturn>().0;
    let entered = pending.as_ref()?.entered.upgrade()?;
    if entered == focused {
        pending.take()
    } else {
        None
    }
}

pub(crate) const PRESS_LEFT: &[MouseButton] = &[MouseButton::Left];
pub(crate) const PRESS_ANY: &[MouseButton] = &[MouseButton::Left, MouseButton::Right];

fn dismissing_on_press(
    mut layer: Div,
    dismiss: Option<DismissHandler>,
    buttons: &[MouseButton],
) -> Div {
    if let Some(dismiss) = dismiss {
        for button in buttons {
            let dismiss = dismiss.clone();
            layer = layer.on_mouse_down(*button, move |_: &MouseDownEvent, window, cx| {
                dismiss(window, cx);
            });
        }
    }
    layer
}

pub(crate) fn dismiss_backdrop(
    dismiss: Option<DismissHandler>,
    buttons: &[MouseButton],
    window: &Window,
) -> impl IntoElement + use<> {
    let viewport = window.viewport_size();
    anchored()
        .position_mode(AnchoredPositionMode::Window)
        .position(point(px(0.0), px(0.0)))
        .anchor(Anchor::TopLeft)
        .child(
            div()
                .w(viewport.width)
                .h(viewport.height)
                .child(dismissing_on_press(
                    div().size_full().occlude(),
                    dismiss,
                    buttons,
                )),
        )
}

pub(crate) fn dismissing_on_escape<E: InteractiveElement>(
    element: E,
    dismiss: impl Fn(&mut Window, &mut App) + 'static,
) -> E {
    element.on_key_down(move |event: &KeyDownEvent, window, cx| {
        if event.keystroke.key.as_str() == ESCAPE_KEY {
            cx.stop_propagation();
            dismiss(window, cx);
        }
    })
}

struct ModalFocus {
    handle: FocusHandle,
    restore: Option<WeakFocusHandle>,
    held: Option<WeakFocusHandle>,
}

impl ModalFocus {
    fn claim(&mut self, window: &mut Window, cx: &mut App) {
        if !self.handle.contains_focused(window, cx) {
            self.restore = window.focused(cx).map(|prior| prior.downgrade());
            window.focus(&self.handle, cx);
        }
    }

    fn note_held_focus(&mut self, window: &Window, cx: &App) {
        if self.handle.contains_focused(window, cx) {
            self.held = window.focused(cx).map(|held| held.downgrade());
        }
    }

    fn focus_is_stranded(&self, window: &Window, cx: &App) -> bool {
        let Some(focused) = window.focused(cx) else {
            return true;
        };
        focused == self.handle
            || self
                .held
                .as_ref()
                .and_then(WeakFocusHandle::upgrade)
                .is_some_and(|held| held == focused)
    }

    fn give_back(&mut self, window: &mut Window, cx: &mut App) {
        if self.focus_is_stranded(window, cx)
            && let Some(prior) = self.restore.take().and_then(|weak| weak.upgrade())
        {
            window.focus(&prior, cx);
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ModalFocusClaim {
    AfterThisFrame,
    OnNextFrame,
}

pub(crate) fn modal_focus(
    key: ElementId,
    claim: ModalFocusClaim,
    window: &mut Window,
    cx: &mut App,
) -> FocusHandle {
    let state: Entity<ModalFocus> = window.use_keyed_state(key, cx, |window, cx| {
        cx.on_release_in(window, ModalFocus::give_back).detach();
        let state = cx.weak_entity();
        let take_focus = move |window: &mut Window, cx: &mut App| {
            if let Some(state) = state.upgrade() {
                state.update(cx, |state, cx| state.claim(window, cx));
            }
        };
        match claim {
            ModalFocusClaim::AfterThisFrame => window.defer(cx, take_focus),
            ModalFocusClaim::OnNextFrame => window.on_next_frame(take_focus),
        }
        let (restore, held) = match take_entrusted_focus(window, cx) {
            Some(entrusted) => (Some(entrusted.prior), Some(entrusted.entered)),
            None => (None, None),
        };
        ModalFocus {
            handle: cx.focus_handle(),
            restore,
            held,
        }
    });
    state.update(cx, |state, cx| {
        state.note_held_focus(window, cx);
        state.handle.clone()
    })
}

const MAX_TAB_STOPS: usize = 512;

fn cycle_focus_within(scope: &FocusHandle, forward: bool, window: &mut Window, cx: &mut App) {
    for _ in 0..MAX_TAB_STOPS {
        if forward {
            window.focus_next(cx);
        } else {
            window.focus_prev(cx);
        }
        if scope.contains_focused(window, cx) {
            return;
        }
    }
    window.focus(scope, cx);
}

pub(crate) fn confine_tab<E: InteractiveElement>(element: E, scope: &FocusHandle) -> E {
    let forward_scope = scope.clone();
    let backward_scope = scope.clone();
    element
        .capture_action(move |_: &FocusNextField, window, cx| {
            cycle_focus_within(&forward_scope, true, window, cx);
            cx.stop_propagation();
        })
        .capture_action(move |_: &FocusPreviousField, window, cx| {
            cycle_focus_within(&backward_scope, false, window, cx);
            cx.stop_propagation();
        })
}

fn dismiss_covered_tooltips(window: &mut Window, cx: &mut App) {
    if window.last_input_was_keyboard() {
        return;
    }
    let position = window.mouse_position();
    let modifiers = window.modifiers();
    window.dispatch_event(
        PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers,
        }),
        cx,
    );
    window.dispatch_event(
        PlatformInput::Scroll(ScrollEvent {
            position,
            delta: ScrollDelta::Pixels(Point::default()),
            modifiers,
            ..ScrollEvent::default()
        }),
        cx,
    );
}

const ENTER_MS: u64 = 200;

const OVERLAY_PRIORITY: usize = 1;

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

pub(crate) fn enter_animation() -> Animation {
    Animation::new(std::time::Duration::from_millis(ENTER_MS)).with_easing(ease_out_cubic)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OverlayPosition {
    Center,
    Left(Pixels),
    Right(Pixels),
}

pub(crate) type DismissHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct Overlay {
    content: gpui::AnyElement,
    position: OverlayPosition,
    scrim: Rgba,
    dismiss_id: ElementId,
    on_dismiss: Option<DismissHandler>,
    busy: bool,
}

pub fn overlay(content: impl IntoElement, palette: &ForgePalette) -> Overlay {
    Overlay {
        content: content.into_any_element(),
        position: OverlayPosition::Center,
        scrim: palette.scrim,
        dismiss_id: ElementId::Name(SharedString::new_static("forge-overlay")),
        on_dismiss: None,
        busy: false,
    }
}

impl Overlay {
    #[must_use]
    pub fn position(mut self, position: OverlayPosition) -> Self {
        self.position = position;
        self
    }

    #[must_use]
    pub fn on_dismiss(
        mut self,
        id: impl Into<ElementId>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.dismiss_id = id.into();
        self.on_dismiss = Some(Rc::new(handler));
        self
    }

    #[must_use]
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    fn escape_focus(&self, window: &mut Window, cx: &mut App) -> FocusHandle {
        modal_focus(
            ElementId::NamedChild(
                Arc::new(self.dismiss_id.clone()),
                SharedString::new_static(ESCAPE_KEY),
            ),
            ModalFocusClaim::OnNextFrame,
            window,
            cx,
        )
    }

    fn dismiss_tooltips_once_mounted(&self, window: &mut Window, cx: &mut App) {
        let key = ElementId::NamedChild(
            Arc::new(self.dismiss_id.clone()),
            SharedString::new_static("tooltips"),
        );
        window.use_keyed_state(key, cx, |window, _| {
            window.on_next_frame(dismiss_covered_tooltips);
        });
    }

    fn render_scrim(&self, animate_alpha: bool) -> gpui::AnyElement {
        let scrim = self.scrim;
        let layer = dismissing_on_press(
            div().absolute().top_0().left_0().size_full().occlude(),
            self.on_dismiss.clone().filter(|_| !self.busy),
            PRESS_LEFT,
        );

        if animate_alpha {
            let base_a = scrim.a;
            layer
                .with_animation(
                    ElementId::NamedChild(
                        Arc::new(self.dismiss_id.clone()),
                        SharedString::new_static("scrim"),
                    ),
                    enter_animation(),
                    move |el, delta| el.bg(with_alpha(scrim, base_a * delta)),
                )
                .into_any_element()
        } else {
            layer.bg(scrim).into_any_element()
        }
    }

    fn panel_anim_id(&self) -> ElementId {
        ElementId::NamedChild(
            Arc::new(self.dismiss_id.clone()),
            SharedString::new_static("panel"),
        )
    }

    fn center_root(&self, content: AnyElement, viewport: Size<Pixels>) -> Div {
        let card = div().occlude().child(content);
        let content_layer = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(card);

        viewport_frame(viewport)
            .child(self.render_scrim(false))
            .child(content_layer)
    }

    fn left_root(&self, content: AnyElement, width: Pixels, viewport: Size<Pixels>) -> Div {
        let panel_id = self.panel_anim_id();
        let card = div()
            .absolute()
            .top_0()
            .left_0()
            .h_full()
            .occlude()
            .child(content)
            .with_animation(panel_id, enter_animation(), move |el, delta| {
                el.left(width * -(1.0 - delta))
            });

        viewport_frame(viewport)
            .child(self.render_scrim(true))
            .child(card)
    }

    fn right_root(&self, content: AnyElement, width: Pixels, viewport: Size<Pixels>) -> Div {
        let panel_id = self.panel_anim_id();
        let card = div()
            .absolute()
            .top_0()
            .right_0()
            .h_full()
            .occlude()
            .child(content)
            .with_animation(panel_id, enter_animation(), move |el, delta| {
                el.right(width * -(1.0 - delta))
            });

        viewport_frame(viewport)
            .child(self.render_scrim(true))
            .child(card)
    }
}

fn viewport_frame(viewport: Size<Pixels>) -> Div {
    div()
        .relative()
        .w(viewport.width)
        .h(viewport.height)
        .overflow_hidden()
}

impl RenderOnce for Overlay {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.dismiss_tooltips_once_mounted(window, cx);
        let escape_focus = self
            .on_dismiss
            .is_some()
            .then(|| self.escape_focus(window, cx));
        let position = self.position;
        let viewport = window.viewport_size();
        let content = std::mem::replace(&mut self.content, div().into_any_element());

        let root: Div = match position {
            OverlayPosition::Center => self.center_root(content, viewport),
            OverlayPosition::Left(width) => self.left_root(content, width, viewport),
            OverlayPosition::Right(width) => self.right_root(content, width, viewport),
        };

        let mut root = root;
        if let (Some(handle), Some(handler)) = (escape_focus.as_ref(), self.on_dismiss.clone()) {
            let busy = self.busy;
            root = dismissing_on_escape(
                confine_tab(root.track_focus(handle), handle),
                move |window, cx| {
                    if !busy {
                        handler(window, cx);
                    }
                },
            );
        }

        let layer = match position {
            OverlayPosition::Center => root
                .with_animation(self.dismiss_id.clone(), enter_animation(), |el, delta| {
                    el.opacity(delta)
                })
                .into_any_element(),
            OverlayPosition::Left(_) | OverlayPosition::Right(_) => root.into_any_element(),
        };
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(Point::default())
                .anchor(Anchor::TopLeft)
                .child(layer),
        )
        .with_priority(OVERLAY_PRIORITY)
        .into_any_element()
    }
}

#[derive(IntoElement)]
pub struct AnchoredPopover {
    position: Point<Pixels>,
    content: AnyElement,
    on_dismiss: Option<DismissHandler>,
    escape_focus: Option<FocusHandle>,
}

pub fn anchored_popover(position: Point<Pixels>, content: impl IntoElement) -> AnchoredPopover {
    AnchoredPopover {
        position,
        content: content.into_any_element(),
        on_dismiss: None,
        escape_focus: None,
    }
}

impl AnchoredPopover {
    #[must_use]
    pub fn on_dismiss(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }

    #[must_use]
    pub fn dismiss_on_escape(mut self, focus_handle: &FocusHandle) -> Self {
        self.escape_focus = Some(focus_handle.clone());
        self
    }
}

impl RenderOnce for AnchoredPopover {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let backdrop_layer = dismiss_backdrop(self.on_dismiss.clone(), PRESS_LEFT, window);

        let content = div().child(self.content).with_animation(
            ElementId::Name(SharedString::new_static("forge-popover-content")),
            enter_animation(),
            |el, delta| el.opacity(delta),
        );

        let panel_layer = anchored()
            .position_mode(AnchoredPositionMode::Window)
            .position(self.position)
            .anchor(Anchor::TopLeft)
            .snap_to_window()
            .child(content);

        let mut root = div().child(backdrop_layer).child(panel_layer);
        if let (Some(handle), Some(dismiss)) = (self.escape_focus.as_ref(), self.on_dismiss.clone())
        {
            root = dismissing_on_escape(
                confine_tab(root.track_focus(handle), handle),
                move |window, cx| dismiss(window, cx),
            );
        }

        deferred(root).with_priority(OVERLAY_PRIORITY)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use std::time::Duration;

    use gpui::{
        AnyView, App, AppContext, Bounds, Context, Entity, EntityInputHandler, FocusHandle,
        Focusable, InteractiveElement, IntoElement, Modifiers, MouseButton, Pixels, Render,
        StatefulInteractiveElement, Styled, TestAppContext, VisualTestContext, Window, div, point,
        px,
    };

    use super::{ease_out_cubic, focus_inside_modal, overlay};
    use crate::palette::FORGE_DEFAULT;
    use crate::test_support::install_presentation;
    use crate::text_input::{TextInput, bind_text_input_keys};
    use gpui::ParentElement;

    struct Host {
        input: Entity<TextInput>,
        outer: Rc<Cell<usize>>,
        inner: Option<Rc<Cell<usize>>>,
        busy: bool,
    }

    impl Render for Host {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let mut content = div().child(self.input.clone()).into_any_element();
            if let Some(counter) = self.inner.clone() {
                content = overlay(content, &FORGE_DEFAULT)
                    .on_dismiss("inner-overlay", move |_, _| counter.set(counter.get() + 1))
                    .into_any_element();
            }
            let outer = self.outer.clone();
            overlay(content, &FORGE_DEFAULT)
                .busy(self.busy)
                .on_dismiss("outer-overlay", move |_, _| outer.set(outer.get() + 1))
        }
    }

    struct Opened<'a> {
        input: Entity<TextInput>,
        outer: Rc<Cell<usize>>,
        inner: Rc<Cell<usize>>,
        vcx: &'a mut VisualTestContext,
    }

    fn open_focused_input(cx: &mut TestAppContext, nested: bool, busy: bool) -> Opened<'_> {
        cx.update(bind_text_input_keys);
        install_presentation(cx);
        let outer = Rc::new(Cell::new(0));
        let inner = Rc::new(Cell::new(0));
        let (outer_seen, inner_seen) = (outer.clone(), inner.clone());
        let (host, vcx) = cx.add_window_view(move |_window, cx| Host {
            input: cx.new(|cx| TextInput::new("placeholder", cx)),
            outer: outer_seen,
            inner: nested.then_some(inner_seen),
            busy,
        });
        vcx.run_until_parked();
        let input = host.read_with(vcx, |host, _| host.input.clone());
        vcx.update(|window, cx| input.update(cx, |input, cx| input.focus(window, cx)));
        vcx.run_until_parked();
        Opened {
            input,
            outer,
            inner,
            vcx,
        }
    }

    #[gpui::test]
    fn escape_in_a_focused_text_input_dismisses_the_enclosing_overlay_once(
        cx: &mut TestAppContext,
    ) {
        let opened = open_focused_input(cx, false, false);

        opened.vcx.simulate_keystrokes("escape");

        assert_eq!(opened.outer.get(), 1);
    }

    #[gpui::test]
    fn escape_during_an_ime_composition_does_not_dismiss_the_overlay(cx: &mut TestAppContext) {
        let opened = open_focused_input(cx, false, false);
        opened.vcx.update(|window, cx| {
            opened.input.update(cx, |input, cx| {
                input.replace_and_mark_text_in_range(None, "\u{304B}", None, window, cx)
            })
        });

        opened.vcx.simulate_keystrokes("escape");

        assert_eq!(opened.outer.get(), 0);
    }

    #[gpui::test]
    fn escape_dismisses_only_the_innermost_of_nested_overlays(cx: &mut TestAppContext) {
        let opened = open_focused_input(cx, true, false);

        opened.vcx.simulate_keystrokes("escape");

        assert_eq!((opened.inner.get(), opened.outer.get()), (1, 0));
    }

    #[gpui::test]
    fn escape_does_not_dismiss_a_busy_overlay(cx: &mut TestAppContext) {
        let opened = open_focused_input(cx, false, true);

        opened.vcx.simulate_keystrokes("escape");

        assert_eq!(opened.outer.get(), 0);
    }

    #[gpui::test]
    fn a_scrim_click_dismisses_an_idle_overlay_and_leaves_a_busy_one_open(cx: &mut TestAppContext) {
        for (busy, dismissals) in [(false, 1), (true, 0)] {
            let opened = open_focused_input(cx, false, busy);
            let scrim = point(px(2.0), px(2.0));

            opened
                .vcx
                .simulate_mouse_down(scrim, MouseButton::Left, Modifiers::none());
            opened
                .vcx
                .simulate_mouse_up(scrim, MouseButton::Left, Modifiers::none());
            opened.vcx.run_until_parked();

            assert_eq!(opened.outer.get(), dismissals, "busy={busy}");
        }
    }

    struct ModalHost {
        list: FocusHandle,
        field: Entity<TextInput>,
        other: FocusHandle,
        open: bool,
    }

    impl Render for ModalHost {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let screen = div().track_focus(&self.list);
            let outside = div().track_focus(&self.other);
            let card = div().child(self.field.clone());
            div().child(screen).child(outside).children(
                self.open
                    .then(|| overlay(card, &FORGE_DEFAULT).on_dismiss("focus-return", |_, _| {})),
            )
        }
    }

    struct ModalRig<'a> {
        host: Entity<ModalHost>,
        vcx: &'a mut VisualTestContext,
    }

    fn mount_modal_rig(cx: &mut TestAppContext) -> ModalRig<'_> {
        cx.update(bind_text_input_keys);
        install_presentation(cx);
        let (host, vcx) = cx.add_window_view(|_window, cx| ModalHost {
            list: cx.focus_handle(),
            field: cx.new(|cx| TextInput::new("placeholder", cx)),
            other: cx.focus_handle(),
            open: false,
        });
        vcx.update(|window, cx| window.focus(&host.read(cx).list.clone(), cx));
        vcx.run_until_parked();
        ModalRig { host, vcx }
    }

    impl ModalRig<'_> {
        fn open_with_the_field_focused(&mut self) {
            let host = self.host.clone();
            self.vcx.update(|window, cx| {
                let field = host.read(cx).field.clone();
                focus_inside_modal(window, cx, |window, cx| {
                    field.update(cx, |field, cx| field.focus(window, cx))
                });
                host.update(cx, |host, cx| {
                    host.open = true;
                    cx.notify();
                });
            });
            self.settle();
        }

        fn close(&mut self) {
            self.host.update(self.vcx, |host, cx| {
                host.open = false;
                cx.notify();
            });
            self.settle();
        }

        fn settle(&mut self) {
            self.vcx.run_until_parked();
            self.vcx.update(|_, _| {});
            self.vcx.run_until_parked();
        }

        fn focused(&mut self, handle: impl Fn(&ModalHost) -> FocusHandle) -> bool {
            let host = self.host.clone();
            self.vcx
                .update(|window, cx| handle(host.read(cx)).is_focused(window))
        }
    }

    #[gpui::test]
    fn a_field_focused_while_opening_stays_focused_and_returns_focus_on_close(
        cx: &mut TestAppContext,
    ) {
        let mut rig = mount_modal_rig(cx);

        rig.open_with_the_field_focused();
        let field = rig.host.read_with(rig.vcx, |host, _| host.field.clone());
        assert!(
            rig.vcx
                .update(|window, cx| { field.read(cx).focus_handle(cx).is_focused(window) })
        );

        rig.close();
        assert!(rig.focused(|host| host.list.clone()));
    }

    #[gpui::test]
    fn focus_moved_outside_before_the_close_is_left_alone(cx: &mut TestAppContext) {
        let mut rig = mount_modal_rig(cx);
        rig.open_with_the_field_focused();
        let host = rig.host.clone();
        rig.vcx
            .update(|window, cx| window.focus(&host.read(cx).other.clone(), cx));
        rig.settle();

        rig.close();

        assert!(rig.focused(|host| host.other.clone()));
    }

    const TOOLTIP_PROBE: &str = "overlay-test-tooltip";
    const BEHIND_THE_MODAL: &str = "overlay-test-behind";
    const INSIDE_THE_MODAL: &str = "overlay-test-inside";
    const LONGER_THAN_ANY_TOOLTIP_DELAY: Duration = Duration::from_secs(5);

    struct TooltipProbe;

    impl Render for TooltipProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .debug_selector(|| TOOLTIP_PROBE.to_owned())
                .w(px(40.0))
                .h(px(20.0))
        }
    }

    fn tooltip_probe(_: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|_| TooltipProbe).into()
    }

    fn tooltip_trigger(id: &'static str) -> impl IntoElement {
        div()
            .id(id)
            .debug_selector(|| id.to_owned())
            .w(px(100.0))
            .h(px(40.0))
            .tooltip(tooltip_probe)
    }

    struct TooltipHost {
        open: bool,
    }

    impl Render for TooltipHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(tooltip_trigger(BEHIND_THE_MODAL))
                .children(
                    self.open
                        .then(|| overlay(tooltip_trigger(INSIDE_THE_MODAL), &FORGE_DEFAULT)),
                )
        }
    }

    struct TooltipRig<'a> {
        host: Entity<TooltipHost>,
        vcx: &'a mut VisualTestContext,
    }

    fn mount_tooltip_rig(cx: &mut TestAppContext) -> TooltipRig<'_> {
        install_presentation(cx);
        let (host, vcx) = cx.add_window_view(|_, _| TooltipHost { open: false });
        vcx.update(|window, _| window.activate_window());
        vcx.run_until_parked();
        TooltipRig { host, vcx }
    }

    impl TooltipRig<'_> {
        fn hover(&mut self, selector: &'static str) {
            let center = self.bounds(selector).center();
            self.vcx
                .simulate_mouse_move(center, None, Modifiers::none());
            self.vcx.run_until_parked();
        }

        fn bounds(&mut self, selector: &'static str) -> Bounds<Pixels> {
            self.vcx.debug_bounds(selector).expect("the element paints")
        }

        fn set_open(&mut self, open: bool) {
            self.host.update(self.vcx, |host, cx| {
                host.open = open;
                cx.notify();
            });
            self.vcx.run_until_parked();
            self.vcx.update(|window, cx| window.simulate_next_frame(cx));
            self.vcx.run_until_parked();
        }

        fn let_any_tooltip_delay_elapse(&mut self) {
            self.vcx
                .executor()
                .advance_clock(LONGER_THAN_ANY_TOOLTIP_DELAY);
            self.vcx.run_until_parked();
        }

        fn tooltip_shown(&mut self) -> bool {
            self.vcx.debug_bounds(TOOLTIP_PROBE).is_some()
        }
    }

    #[gpui::test]
    fn a_tooltip_shows_on_the_screen_behind_while_no_modal_is_up(cx: &mut TestAppContext) {
        let mut rig = mount_tooltip_rig(cx);
        rig.hover(BEHIND_THE_MODAL);

        rig.let_any_tooltip_delay_elapse();

        assert!(rig.tooltip_shown());
    }

    #[gpui::test]
    fn a_pending_tooltip_does_not_show_over_a_mounted_modal(cx: &mut TestAppContext) {
        let mut rig = mount_tooltip_rig(cx);
        rig.hover(BEHIND_THE_MODAL);

        rig.set_open(true);
        rig.let_any_tooltip_delay_elapse();

        assert!(!rig.tooltip_shown());
    }

    #[gpui::test]
    fn a_visible_tooltip_disappears_when_a_modal_mounts(cx: &mut TestAppContext) {
        let mut rig = mount_tooltip_rig(cx);
        rig.hover(BEHIND_THE_MODAL);
        rig.let_any_tooltip_delay_elapse();
        assert!(rig.tooltip_shown());

        rig.set_open(true);

        assert!(!rig.tooltip_shown());
    }

    #[gpui::test]
    fn a_pending_tooltip_is_also_dropped_by_a_reopened_modal(cx: &mut TestAppContext) {
        let mut rig = mount_tooltip_rig(cx);
        rig.set_open(true);
        rig.set_open(false);
        rig.hover(BEHIND_THE_MODAL);

        rig.set_open(true);
        rig.let_any_tooltip_delay_elapse();

        assert!(!rig.tooltip_shown());
    }

    #[gpui::test]
    fn a_tooltip_inside_a_modal_still_shows(cx: &mut TestAppContext) {
        let mut rig = mount_tooltip_rig(cx);
        rig.set_open(true);
        rig.hover(INSIDE_THE_MODAL);

        rig.let_any_tooltip_delay_elapse();

        assert!(rig.tooltip_shown());
    }

    #[gpui::test]
    fn a_modal_opened_from_the_keyboard_keeps_keyboard_modality(cx: &mut TestAppContext) {
        let mut rig = mount_tooltip_rig(cx);
        rig.vcx.simulate_keystrokes("a");

        rig.set_open(true);

        assert!(rig.vcx.update(|window, _| window.last_input_was_keyboard()));
    }

    #[test]
    fn ease_out_cubic_traces_the_expected_curve() {
        const EPS: f32 = 1e-6;

        for (t, want) in [(0.0_f32, 0.0_f32), (0.5, 0.875), (1.0, 1.0)] {
            assert!(
                (ease_out_cubic(t) - want).abs() < EPS,
                "ease_out_cubic({t}) = {}, want {want}",
                ease_out_cubic(t),
            );
        }

        let mut prev = f32::NEG_INFINITY;
        for t in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            let y = ease_out_cubic(t);
            assert!(y >= prev, "curve dipped at t={t}: {y} < {prev}");
            prev = y;
        }
    }
}
