use gpui::{
    App, AppContext, Context, DragMoveEvent, ElementId, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, RenderOnce, Rgba, StatefulInteractiveElement, Styled, Window,
    div, px, relative,
};

use crate::palette::ForgePalette;

const TRACK_HEIGHT: Pixels = px(4.0);
const KNOB_DIAMETER: Pixels = px(11.0);
const KNOB_RADIUS: Pixels = px(5.5);
const TRACK_RADIUS: Pixels = px(2.0);
const RAIL_TOP: Pixels = px(3.5);
const KNOB_TOP: Pixels = px(-3.5);
const KNOB_RIGHT: Pixels = px(-5.5);

type ChangeHandler = Box<dyn Fn(&f32, &mut Window, &mut App) + 'static>;

/// A non-positive span (collapsed or inverted range) yields `0.0` rather than dividing
/// by zero.
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

#[derive(IntoElement)]
pub struct Slider {
    value: f32,
    min: f32,
    max: f32,
    fill: Rgba,
    rail: Rgba,
    thumb: Rgba,
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
        id: None,
        on_change: None,
    }
}

/// Drag-payload preview gpui renders at the cursor; deliberately paints nothing - the
/// moving thumb is the only feedback.
struct DragGhost;

impl Render for DragGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Drag payload; `on_drag_move` keys on an active drag of this type to keep
/// delivering move events once the cursor leaves the track bounds. Carries the
/// originating slider's id because gpui fans a typed drag out to EVERY listener
/// of that type - each handler must ignore drags it did not start.
struct SliderDrag {
    id: ElementId,
}

impl Slider {
    /// Makes the slider draggable; without it the slider is a static read-only bar. The
    /// handler gets each new (already-clamped) value as the drag moves.
    #[must_use]
    pub fn on_change(
        mut self,
        id: impl Into<ElementId>,
        handler: impl Fn(&f32, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.id = Some(id.into());
        self.on_change = Some(Box::new(handler));
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
                let (min, max) = (self.min, self.max);
                let drag_id = id.clone();
                let payload_id = id.clone();
                track
                    .id(id)
                    .cursor_pointer()
                    .on_drag(SliderDrag { id: payload_id }, |_, _, _, cx| {
                        cx.new(|_| DragGhost)
                    })
                    .on_drag_move(move |e: &DragMoveEvent<SliderDrag>, window, cx| {
                        if e.drag(cx).id != drag_id {
                            return;
                        }
                        let width = e.bounds.right() - e.bounds.left();
                        let f = if width > px(0.0) {
                            ((e.event.position.x - e.bounds.left()) / width).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        handler(&value_at(f, min, max), window, cx);
                    })
                    .into_any_element()
            }
            _ => track.into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fraction, value_at};

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
        // Why: a naive (value - min) / (max - min) divides by zero here and yields
        // NaN/inf, which would corrupt the track width. The guard must return 0.0.
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
}
