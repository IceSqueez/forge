use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    Anchor, AnchoredPositionMode, Animation, AnimationExt, AnyElement, App, Div, ElementId, Entity,
    FocusHandle, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    ParentElement, Pixels, Point, RenderOnce, Rgba, SharedString, Styled, Window, anchored,
    deferred, div, point, px,
};

use crate::palette::{ForgePalette, with_alpha};
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

type DismissHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

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
        let handle: Entity<FocusHandle> = window.use_keyed_state(
            ElementId::NamedChild(
                Arc::new(self.dismiss_id.clone()),
                SharedString::new_static(ESCAPE_KEY),
            ),
            cx,
            |window, cx| {
                let handle = cx.focus_handle();
                let claim = handle.clone();
                window.on_next_frame(move |window, cx| {
                    if !claim.contains_focused(window, cx) {
                        window.focus(&claim, cx);
                    }
                });
                handle
            },
        );
        handle.read(cx).clone()
    }

    fn render_scrim(&self, animate_alpha: bool) -> gpui::AnyElement {
        let scrim = self.scrim;
        let mut layer = div().absolute().top_0().left_0().size_full().occlude();

        if let Some(handler) = self.on_dismiss.clone().filter(|_| !self.busy) {
            layer =
                layer.on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    handler(window, cx);
                });
        }

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
}

impl RenderOnce for Overlay {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let escape_focus = self
            .on_dismiss
            .is_some()
            .then(|| self.escape_focus(window, cx));
        let position = self.position;
        let viewport = window.viewport_size();
        let content = std::mem::replace(&mut self.content, div().into_any_element());

        let root: Div = match position {
            OverlayPosition::Center => {
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

                div()
                    .relative()
                    .w(viewport.width)
                    .h(viewport.height)
                    .overflow_hidden()
                    .child(self.render_scrim(false))
                    .child(content_layer)
            }
            OverlayPosition::Left(width) => {
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

                div()
                    .relative()
                    .w(viewport.width)
                    .h(viewport.height)
                    .overflow_hidden()
                    .child(self.render_scrim(true))
                    .child(card)
            }
            OverlayPosition::Right(width) => {
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

                div()
                    .relative()
                    .w(viewport.width)
                    .h(viewport.height)
                    .overflow_hidden()
                    .child(self.render_scrim(true))
                    .child(card)
            }
        };

        let mut root = root;
        if let (Some(handle), Some(handler)) = (escape_focus.as_ref(), self.on_dismiss.clone()) {
            let busy = self.busy;
            root = root
                .track_focus(handle)
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key.as_str() != ESCAPE_KEY {
                        return;
                    }
                    cx.stop_propagation();
                    if !busy {
                        handler(window, cx);
                    }
                });
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
        let viewport = window.viewport_size();
        let mut backdrop = div().size_full().occlude();
        if let Some(dismiss) = self.on_dismiss.clone() {
            backdrop =
                backdrop.on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    dismiss(window, cx);
                });
        }
        let backdrop_layer = anchored()
            .position_mode(AnchoredPositionMode::Window)
            .position(point(px(0.0), px(0.0)))
            .anchor(Anchor::TopLeft)
            .child(div().w(viewport.width).h(viewport.height).child(backdrop));

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
            root = root
                .track_focus(handle)
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key.as_str() == ESCAPE_KEY {
                        cx.stop_propagation();
                        dismiss(window, cx);
                    }
                });
        }

        deferred(root).with_priority(OVERLAY_PRIORITY)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{
        AppContext, Context, Entity, EntityInputHandler, IntoElement, Modifiers, MouseButton,
        Render, TestAppContext, VisualTestContext, Window, div, point, px,
    };

    use super::{ease_out_cubic, overlay};
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
