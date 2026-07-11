use std::rc::Rc;

use gpui::{
    Animation, AnimationExt, App, Div, ElementId, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, ParentElement, Pixels, RenderOnce, Rgba,
    SharedString, Styled, Window, deferred, div,
};

use crate::palette::{ForgePalette, with_alpha};

/// Enter-animation span, shared by the scrim fade and the panel slide. Matches the
/// docked-panel timing so an overlay wrapping a panel reads as one motion.
const ENTER_MS: u64 = 200;

/// Draw priority for the deferred overlay pass. Any positive value lifts the whole
/// overlay above the ordinary sibling content painted in the same frame; `1` is
/// enough while overlays are singular on screen.
const OVERLAY_PRIORITY: usize = 1;

/// Ease-out cubic: quick to start, settling gently into place. Kept as a free
/// function so the scrim and panel animations share one identical curve.
fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// Where the overlay parks its content within the dimmed window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OverlayPosition {
    /// Centred both ways — the shape a modal dialog composes into. Enters by fading
    /// the whole overlay in.
    Center,
    /// Docked full-height to the left edge, `Pixels` wide. Enters by sliding in from
    /// off the left edge; the width is the slide distance.
    Left(Pixels),
    /// Docked full-height to the right edge, `Pixels` wide. Enters by sliding in from
    /// off the right edge; the width is the slide distance.
    Right(Pixels),
}

/// Boxed dismiss callback. Deliberately event-free: the same callback answers both a
/// scrim click and an Escape press, so it takes only the two contexts through which
/// the caller reaches its own entity to hide the overlay. Shared (`Rc`) because it is
/// wired into more than one listener.
type DismissHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

/// The chrome an overlaid surface sits in: a full-window scrim that dims and seals off
/// the app behind it, a positioned content slot, an enter animation, and optional
/// scrim-click / Escape dismissal.
///
/// Build one with [`overlay`], then layer on `.position`, `.on_dismiss` and (to enable
/// Escape) `.dismiss_on_escape`. The surface itself — a modal card, a docked side sheet,
/// a popover — is the `content` passed in; this component only supplies the shell.
///
/// The overlay draws in a deferred pass so it lifts above ordinary sibling content, and
/// its scrim occludes the mouse so nothing behind it stays interactive. A click that
/// lands on the content is swallowed by the content; a click that misses it lands on the
/// scrim and dismisses.
#[derive(IntoElement)]
pub struct Overlay {
    content: gpui::AnyElement,
    position: OverlayPosition,
    scrim: Rgba,
    dismiss_id: ElementId,
    on_dismiss: Option<DismissHandler>,
    escape_focus: Option<FocusHandle>,
}

/// Wrap `content` in overlay chrome, resolving the scrim ink from `palette` up front so
/// the built value carries no palette borrow. Defaults to centred, with no dismissal
/// wired; layer those on through the builder methods.
pub fn overlay(content: impl IntoElement, palette: &ForgePalette) -> Overlay {
    Overlay {
        content: content.into_any_element(),
        position: OverlayPosition::Center,
        scrim: palette.scrim,
        dismiss_id: ElementId::Name(SharedString::new_static("forge-overlay")),
        on_dismiss: None,
        escape_focus: None,
    }
}

impl Overlay {
    /// Sets where the content parks (default [`OverlayPosition::Center`]).
    #[must_use]
    pub fn position(mut self, position: OverlayPosition) -> Self {
        self.position = position;
        self
    }

    /// Wires scrim-click (and, once [`Overlay::dismiss_on_escape`] is set, Escape)
    /// dismissal. `id` gives the overlay a stable identity for its enter-animation
    /// state; `handler` mutates the caller's entity through the passed contexts to hide
    /// the overlay.
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

    /// Routes Escape to the [`Overlay::on_dismiss`] handler. gpui delivers key events
    /// only down the focus path, so the overlay tracks `focus_handle` and the caller
    /// must focus it when the overlay opens (e.g. `window.focus(&handle)`); without a
    /// focused handle the scrim click still dismisses but Escape stays inert.
    #[must_use]
    pub fn dismiss_on_escape(mut self, focus_handle: &FocusHandle) -> Self {
        self.escape_focus = Some(focus_handle.clone());
        self
    }

    /// Builds the scrim: a full-window dimmer that seals the mouse off from the app and,
    /// when dismissal is wired, reports clicks that miss the content. For a docked
    /// position it fades its alpha in over the enter span; for a centred position it
    /// stays flat (the whole overlay fades as one, see [`RenderOnce::render`]).
    fn render_scrim(&self, animate_alpha: bool) -> gpui::AnyElement {
        let scrim = self.scrim;
        let mut layer = div().absolute().top_0().left_0().size_full().occlude();

        if let Some(handler) = self.on_dismiss.clone() {
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
                        Box::new(self.dismiss_id.clone()),
                        SharedString::new_static("scrim"),
                    ),
                    Animation::new(std::time::Duration::from_millis(ENTER_MS))
                        .with_easing(ease_out_cubic),
                    move |el, delta| el.bg(with_alpha(scrim, base_a * delta)),
                )
                .into_any_element()
        } else {
            layer.bg(scrim).into_any_element()
        }
    }

    /// The stable id for the panel-slide animation, derived from the overlay id so it
    /// never collides with the scrim-fade animation.
    fn panel_anim_id(&self) -> ElementId {
        ElementId::NamedChild(
            Box::new(self.dismiss_id.clone()),
            SharedString::new_static("panel"),
        )
    }
}

impl RenderOnce for Overlay {
    fn render(mut self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let position = self.position;
        let content = std::mem::replace(&mut self.content, div().into_any_element());

        // The content wrapper occludes so a click on the surface is swallowed here and
        // never reaches the scrim behind it — only clicks that miss the surface dismiss.
        let anim = || {
            Animation::new(std::time::Duration::from_millis(ENTER_MS)).with_easing(ease_out_cubic)
        };

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
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
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
                    .with_animation(panel_id, anim(), move |el, delta| {
                        el.left(width * -(1.0 - delta))
                    });

                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
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
                    .with_animation(panel_id, anim(), move |el, delta| {
                        el.right(width * -(1.0 - delta))
                    });

                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .overflow_hidden()
                    .child(self.render_scrim(true))
                    .child(card)
            }
        };

        // Escape rides the focus path: track the caller's handle so the key event is
        // delivered here, and translate it into the same dismiss the scrim click uses.
        let mut root = root;
        if let (Some(handle), Some(handler)) = (self.escape_focus.as_ref(), self.on_dismiss.clone())
        {
            root = root
                .track_focus(handle)
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key.as_str() == "escape" {
                        handler(window, cx);
                    }
                });
        }

        // A centred overlay enters as one uniform fade (scrim + card); the docked
        // positions fade the scrim and slide the panel on their own timers, so their
        // root stays un-faded.
        match position {
            OverlayPosition::Center => deferred(root.with_animation(
                self.dismiss_id.clone(),
                anim(),
                |el, delta| el.opacity(delta),
            ))
            .with_priority(OVERLAY_PRIORITY)
            .into_any_element(),
            OverlayPosition::Left(_) | OverlayPosition::Right(_) => deferred(root)
                .with_priority(OVERLAY_PRIORITY)
                .into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ease_out_cubic;

    /// Pins the easing to `1 - (1-t)^3`: the two endpoints anchor the [0,1] range, the
    /// midpoint distinguishes ease-OUT from the plausible wrong curves (ease-in `t^3`
    /// gives 0.125, quadratic `1-(1-t)^2` gives 0.75, linear gives 0.5), and the
    /// ascending sweep holds the curve non-decreasing.
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
