use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    App, AppContext, Bounds, Context, DragMoveEvent, ElementId, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, ParentElement, Pixels, Render, RenderOnce, Rgba,
    StatefulInteractiveElement, Styled, Window, canvas, div, px, relative,
};

use crate::key_activation::{KeyActivated, StepDown, StepUp};
use crate::palette::ForgePalette;

const TRACK_HEIGHT: Pixels = px(4.0);
const KNOB_DIAMETER: Pixels = px(11.0);
const KNOB_RADIUS: Pixels = px(5.5);
const TRACK_RADIUS: Pixels = px(2.0);
const RAIL_TOP: Pixels = px(3.5);
const KNOB_TOP: Pixels = px(-3.5);
const KNOB_RIGHT: Pixels = px(-5.5);
const KEY_STEPS_ACROSS: f32 = 20.0;

type ChangeHandler = Rc<dyn Fn(&f32, &mut Window, &mut App) + 'static>;

pub(crate) fn fraction(value: f32, min: f32, max: f32) -> f32 {
    let span = max - min;
    if span <= 0.0 {
        return 0.0;
    }
    ((value - min) / span).clamp(0.0, 1.0)
}

pub(crate) fn value_at(fraction: f32, min: f32, max: f32) -> f32 {
    min + fraction.clamp(0.0, 1.0) * (max - min)
}

fn fraction_at(x: Pixels, track: Bounds<Pixels>) -> f32 {
    let width = track.right() - track.left();
    if width > px(0.0) {
        ((x - track.left()) / width).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub(crate) fn key_step(min: f32, max: f32, whole_numbers: bool) -> f32 {
    let step = (max - min) / KEY_STEPS_ACROSS;
    if whole_numbers {
        step.round().max(1.0)
    } else {
        step
    }
}

pub(crate) fn stepped_value(value: f32, by: f32, min: f32, max: f32) -> f32 {
    if max <= min {
        return value;
    }
    (value + by).clamp(min, max)
}

#[derive(IntoElement)]
pub struct Slider {
    value: f32,
    min: f32,
    max: f32,
    fill: Rgba,
    rail: Rgba,
    thumb: Rgba,
    whole_numbers: bool,
    id: Option<ElementId>,
    on_change: Option<ChangeHandler>,
}

pub fn slider(value: f32, min: f32, max: f32, palette: &ForgePalette) -> Slider {
    Slider {
        value,
        min,
        max,
        fill: palette.brand,
        rail: palette.surface_overlay,
        thumb: palette.text_primary,
        whole_numbers: false,
        id: None,
        on_change: None,
    }
}

struct DragGhost;

impl Render for DragGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

struct SliderDrag {
    id: ElementId,
}

impl Slider {
    #[must_use]
    pub fn accent(mut self, fill: Rgba) -> Self {
        self.fill = fill;
        self.thumb = fill;
        self
    }

    #[must_use]
    pub fn whole_numbers(mut self) -> Self {
        self.whole_numbers = true;
        self
    }

    #[must_use]
    pub fn on_change(
        mut self,
        id: impl Into<ElementId>,
        handler: impl Fn(&f32, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.id = Some(id.into());
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Slider {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let f = fraction(self.value, self.min, self.max);

        let thumb = div()
            .absolute()
            .top(KNOB_TOP)
            .right(KNOB_RIGHT)
            .size(KNOB_DIAMETER)
            .rounded(KNOB_RADIUS)
            .bg(self.thumb);

        let fill = div()
            .absolute()
            .left_0()
            .top(RAIL_TOP)
            .w(relative(f))
            .h(TRACK_HEIGHT)
            .rounded(TRACK_RADIUS)
            .bg(self.fill)
            .child(thumb);

        let rail = div()
            .absolute()
            .left_0()
            .right_0()
            .top(RAIL_TOP)
            .h(TRACK_HEIGHT)
            .rounded(TRACK_RADIUS)
            .bg(self.rail);

        let track = div()
            .relative()
            .w_full()
            .h(KNOB_DIAMETER)
            .child(rail)
            .child(fill);

        match (self.id, self.on_change) {
            (Some(id), Some(handler)) => {
                let (value, min, max) = (self.value, self.min, self.max);
                let step = key_step(min, max, self.whole_numbers);
                let drag_id = id.clone();
                let payload_id = id.clone();
                let painted: Rc<Cell<Bounds<Pixels>>> = Rc::default();
                let probe = Rc::clone(&painted);
                let on_press = Rc::clone(&handler);
                let on_step_up = Rc::clone(&handler);
                let on_step_down = Rc::clone(&handler);
                let change_by =
                    move |by: f32, changed: &ChangeHandler, window: &mut Window, cx: &mut App| {
                        let next = stepped_value(value, by, min, max);
                        if next != value {
                            changed(&next, window, cx);
                        }
                    };
                track
                    .id(id)
                    .cursor_pointer()
                    .rounded(TRACK_RADIUS)
                    .key_stepped(self.fill)
                    .child(
                        canvas(move |bounds, _, _| probe.set(bounds), |_, _, _, _| {})
                            .absolute()
                            .size_full(),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        move |event: &MouseDownEvent, window, cx| {
                            let at = fraction_at(event.position.x, painted.get());
                            on_press(&value_at(at, min, max), window, cx);
                        },
                    )
                    .on_action(move |_: &StepUp, window, cx| {
                        change_by(step, &on_step_up, window, cx)
                    })
                    .on_action(move |_: &StepDown, window, cx| {
                        change_by(-step, &on_step_down, window, cx)
                    })
                    .on_drag(SliderDrag { id: payload_id }, |_, _, _, cx| {
                        cx.new(|_| DragGhost)
                    })
                    .on_drag_move(move |e: &DragMoveEvent<SliderDrag>, window, cx| {
                        if e.drag(cx).id != drag_id {
                            return;
                        }
                        let at = fraction_at(e.event.position.x, e.bounds);
                        handler(&value_at(at, min, max), window, cx);
                    })
                    .into_any_element()
            }
            _ => track.into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use gpui::{
        Entity, FocusHandle, KeyBinding, Modifiers, Render, TestAppContext, VisualTestContext,
        point,
    };

    use super::*;
    use super::{fraction, key_step, slider, stepped_value, value_at};
    use crate::key_activation::bind_key_activation;
    use crate::palette::FORGE_DEFAULT;
    use crate::test_support::install_presentation;
    use crate::text_input::FocusNextField;

    const EPS: f32 = 1e-5;
    const MIN: f32 = 20.0;
    const MAX: f32 = 100.0;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < EPS
    }

    #[test]
    fn fraction_normalizes_and_clamps_across_the_span() {
        for (value, expected) in [
            (MIN, 0.0),
            (MAX, 1.0),
            (60.0, 0.5),
            (MIN - 30.0, 0.0),
            (MAX + 30.0, 1.0),
        ] {
            let f = fraction(value, MIN, MAX);
            assert!(
                close(f, expected),
                "fraction({value}) = {f}, want {expected}"
            );
        }
    }

    #[test]
    fn fraction_returns_zero_when_span_is_not_positive() {
        for (min, max) in [(50.0, 50.0), (100.0, 20.0)] {
            let f = fraction(75.0, min, max);
            assert!(
                f.is_finite(),
                "fraction over span [{min},{max}] = {f}, not finite"
            );
            assert!(
                close(f, 0.0),
                "fraction over span [{min},{max}] = {f}, want 0.0"
            );
        }
    }

    #[test]
    fn value_at_maps_fraction_back_onto_the_range_and_clamps() {
        for (frac, expected) in [(0.0, MIN), (1.0, MAX), (0.5, 60.0), (-0.5, MIN), (1.5, MAX)] {
            let v = value_at(frac, MIN, MAX);
            assert!(
                close(v, expected),
                "value_at({frac}) = {v}, want {expected}"
            );
        }
    }

    #[test]
    fn value_at_is_the_inverse_of_fraction_over_the_valid_range() {
        for v in [MIN, 35.0, 60.0, 99.9, MAX] {
            let round = value_at(fraction(v, MIN, MAX), MIN, MAX);
            assert!(close(round, v), "round-trip of {v} = {round}");
        }
    }

    #[test]
    fn a_key_step_covers_a_twentieth_of_the_span_and_never_less_than_one_whole_number() {
        for (min, max, whole, expected) in [
            (0.0, 1.0, false, 0.05),
            (0.0, 100.0, false, 5.0),
            (0.0, 100.0, true, 5.0),
            (1.0, 8.0, true, 1.0),
            (0.0, 3600.0, true, 180.0),
        ] {
            let step = key_step(min, max, whole);
            assert!(close(step, expected), "{min}..{max} {whole}: {step}");
        }
    }

    #[test]
    fn stepping_stops_at_the_ends_and_an_empty_span_keeps_the_value() {
        for (value, by, min, max, expected) in [
            (50.0, 5.0, 0.0, 100.0, 55.0),
            (98.0, 5.0, 0.0, 100.0, 100.0),
            (2.0, -5.0, 0.0, 100.0, 0.0),
            (7.0, 1.0, 7.0, 7.0, 7.0),
            (7.0, 1.0, 9.0, 3.0, 7.0),
        ] {
            let next = stepped_value(value, by, min, max);
            assert!(close(next, expected), "{value}+{by}: {next}");
        }
    }

    const SLIDER_ID: &str = "slider-under-test";
    const SLIDER_WIDTH: f32 = 200.0;

    struct Host {
        focus: FocusHandle,
        value: f32,
        whole_numbers: bool,
        changes: Rc<RefCell<Vec<f32>>>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let changes = self.changes.clone();
            let mut control = slider(self.value, 0.0, 100.0, &FORGE_DEFAULT);
            if self.whole_numbers {
                control = control.whole_numbers();
            }
            div()
                .size_full()
                .track_focus(&self.focus)
                .on_action(|_: &FocusNextField, window, cx| window.focus_next(cx))
                .child(
                    div()
                        .w(px(SLIDER_WIDTH))
                        .debug_selector(|| SLIDER_ID.to_owned())
                        .child(control.on_change(
                            SLIDER_ID,
                            cx.listener(move |this, next: &f32, _, cx| {
                                changes.borrow_mut().push(*next);
                                this.value = *next;
                                cx.notify();
                            }),
                        )),
                )
        }
    }

    fn mount(
        cx: &mut TestAppContext,
        value: f32,
    ) -> (Rc<RefCell<Vec<f32>>>, Entity<Host>, &mut VisualTestContext) {
        cx.update(|cx| {
            bind_key_activation(cx);
            cx.bind_keys([KeyBinding::new("tab", FocusNextField, None)]);
        });
        install_presentation(cx);
        let changes: Rc<RefCell<Vec<f32>>> = Rc::default();
        let seen = changes.clone();
        let (host, vcx) = cx.add_window_view(move |window, cx| {
            let focus = cx.focus_handle();
            window.focus(&focus, cx);
            Host {
                focus,
                value,
                whole_numbers: false,
                changes: seen,
            }
        });
        vcx.update(|window, _| window.activate_window());
        vcx.run_until_parked();
        (changes, host, vcx)
    }

    #[gpui::test]
    fn arrows_on_a_tabbed_slider_step_its_value(cx: &mut TestAppContext) {
        let (changes, _, vcx) = mount(cx, 50.0);

        vcx.simulate_keystrokes("tab right up left left down");

        assert_eq!(*changes.borrow(), [55.0, 60.0, 55.0, 50.0, 45.0]);
    }

    #[gpui::test]
    fn arrows_do_nothing_until_the_slider_has_focus(cx: &mut TestAppContext) {
        let (changes, _, vcx) = mount(cx, 50.0);

        vcx.simulate_keystrokes("right left");

        assert!(changes.borrow().is_empty());
    }

    #[gpui::test]
    fn a_held_arrow_at_the_end_reports_no_change(cx: &mut TestAppContext) {
        let (changes, _, vcx) = mount(cx, 100.0);

        vcx.simulate_keystrokes("tab right up");

        assert!(changes.borrow().is_empty());
    }

    #[gpui::test]
    fn whole_number_sliders_step_by_at_least_one(cx: &mut TestAppContext) {
        let (changes, host, vcx) = mount(cx, 50.0);
        host.update(vcx, |host, cx| {
            host.whole_numbers = true;
            cx.notify();
        });
        vcx.run_until_parked();

        vcx.simulate_keystrokes("tab right");

        assert_eq!(*changes.borrow(), [55.0]);
    }

    #[gpui::test]
    fn a_press_on_the_track_sets_the_value_without_taking_focus(cx: &mut TestAppContext) {
        let (changes, host, vcx) = mount(cx, 50.0);
        let track = vcx.debug_bounds(SLIDER_ID).expect("the slider paints");
        let at_a_quarter = point(
            track.left() + px(SLIDER_WIDTH / 4.0),
            track.top() + track.size.height / 2.0,
        );

        vcx.simulate_click(at_a_quarter, Modifiers::none());
        vcx.run_until_parked();

        assert_eq!(*changes.borrow(), [25.0]);
        let focus = host.read_with(vcx, |host, _| host.focus.clone());
        assert!(vcx.update(|window, _| focus.is_focused(window)));
    }

    #[gpui::test]
    fn enter_and_space_leave_a_focused_slider_alone(cx: &mut TestAppContext) {
        let (changes, _, vcx) = mount(cx, 50.0);

        vcx.simulate_keystrokes("tab enter space");

        assert!(changes.borrow().is_empty());
    }
}
