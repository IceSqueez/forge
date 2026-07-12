use gpui::{
    App, AppContext, Context, DragMoveEvent, ElementId, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, RenderOnce, Rgba, StatefulInteractiveElement, Styled, Window,
    div, px, relative,
};

use crate::palette::ForgePalette;

/// Thickness of the rail — both the recessed unfilled bar and the accent fill.
const TRACK_HEIGHT: Pixels = px(4.0);
/// Diameter of the circular thumb.
const KNOB_DIAMETER: Pixels = px(11.0);
/// Half the thumb diameter — the thumb's centre sits at the value point, so it is
/// pulled back by this much to straddle the leading edge of the fill.
const KNOB_RADIUS: Pixels = px(5.5);
/// Corner radius of the rail (half its height), giving the bar rounded caps.
const TRACK_RADIUS: Pixels = px(2.0);
/// Top inset of the rail inside the thumb-tall track row, centring the thin bar:
/// `(KNOB_DIAMETER - TRACK_HEIGHT) / 2`.
const RAIL_TOP: Pixels = px(3.5);
/// Top inset of the thumb inside the fill bar, lifting the thumb so its own centre
/// lines up with the rail centre: `-(KNOB_DIAMETER - TRACK_HEIGHT) / 2`.
const KNOB_TOP: Pixels = px(-3.5);
/// Trailing inset of the thumb inside the fill bar. Negative by a radius so the
/// thumb centres on the fill's leading edge rather than sitting beyond it.
const KNOB_RIGHT: Pixels = px(-5.5);

/// Boxed value handler, fired continuously while the track is dragged. Takes the
/// already-clamped new value by reference so it composes with `cx.listener` (which
/// yields `Fn(&E, …)`); the caller stores the value, `cx.notify()`s and feeds it
/// back through [`slider`].
type ChangeHandler = Box<dyn Fn(&f32, &mut Window, &mut App) + 'static>;

/// Maps a value onto its `0.0..=1.0` position along the `[min, max]` span, clamped.
/// A non-positive span (misconfigured or collapsed range) yields `0.0` rather than a
/// division by zero.
pub(crate) fn fraction(value: f32, min: f32, max: f32) -> f32 {
    let span = max - min;
    if span <= 0.0 {
        return 0.0;
    }
    ((value - min) / span).clamp(0.0, 1.0)
}

/// Maps a `0.0..=1.0` track position back onto a value in `[min, max]`. The position
/// is clamped first, so the result never escapes the range.
pub(crate) fn value_at(fraction: f32, min: f32, max: f32) -> f32 {
    min + fraction.clamp(0.0, 1.0) * (max - min)
}

/// A horizontal value slider: a recessed rail whose leading portion fills with the
/// brand accent up to the current value, carrying a circular thumb at the value
/// point.
///
/// The value is caller-owned state — pass the current value plus its `[min, max]`
/// bounds via [`slider`]. Attach [`Slider::on_change`] to make it draggable: the
/// handler receives each new (already-clamped) value as the drag moves, for the
/// caller to store, `cx.notify()` and feed back. Left off, the slider is a static,
/// read-only value bar.
///
/// The three inks are fixed — brand fill, a recessed unfilled rail and a bright
/// thumb — so there is no value-dependent colour choice to resolve.
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

/// Builds a slider at `value` within `[min, max]`, resolving its rail, fill and thumb
/// inks from the active theme up front so the returned value carries no palette
/// borrow. The fill is `brand`, the unfilled rail is `surface_overlay`, and the thumb
/// is `text_primary`.
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

/// Invisible ghost view gpui renders at the cursor while a slider drag is active. It
/// paints nothing — a slider has no drag preview beyond the moving thumb itself.
struct DragGhost;

impl Render for DragGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Zero-size drag payload marker. An active drag of this type is what
/// [`gpui::InteractiveElement::on_drag_move`] keys on to keep delivering move events
/// once the cursor leaves the track bounds.
struct SliderDrag;

impl Slider {
    /// Makes the slider draggable. gpui needs a stable [`ElementId`] to promote the
    /// track to a draggable element; the `handler` receives each new (already-clamped)
    /// value as the drag moves, for the caller to store and feed back through
    /// [`slider`]. Compose the handler with `cx.listener` so it mutates the caller's
    /// entity.
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

        // Thumb straddles the leading edge of the fill: pulled back a radius on the
        // trailing side and lifted so its centre meets the rail centre.
        let thumb = div()
            .absolute()
            .top(KNOB_TOP)
            .right(KNOB_RIGHT)
            .size(KNOB_DIAMETER)
            .rounded(KNOB_RADIUS)
            .bg(self.thumb);

        // Accent fill from the leading edge up to the value, carrying the thumb.
        let fill = div()
            .absolute()
            .left_0()
            .top(RAIL_TOP)
            .w(relative(f))
            .h(TRACK_HEIGHT)
            .rounded(TRACK_RADIUS)
            .bg(self.fill)
            .child(thumb);

        // Recessed unfilled rail spanning the full width behind the fill.
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
                track
                    .id(id)
                    .cursor_pointer()
                    .on_drag(SliderDrag, |_, _, _, cx| cx.new(|_| DragGhost))
                    // While a drag is active the move events arrive here even once the
                    // cursor leaves the track. The value is the cursor's fractional
                    // position across the track width, clamped and mapped onto the range.
                    .on_drag_move(move |e: &DragMoveEvent<SliderDrag>, window, cx| {
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
            (MIN, 0.0),        // lower bound maps to 0
            (MAX, 1.0),        // upper bound maps to 1
            (60.0, 0.5),       // midpoint of [20, 100]
            (MIN - 30.0, 0.0), // below min clamps to 0
            (MAX + 30.0, 1.0), // above max clamps to 1
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
        for (frac, expected) in [
            (0.0, MIN),  // start of track is min
            (1.0, MAX),  // end of track is max
            (0.5, 60.0), // midpoint of [20, 100]
            (-0.5, MIN), // below 0 clamps to min
            (1.5, MAX),  // above 1 clamps to max
        ] {
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
