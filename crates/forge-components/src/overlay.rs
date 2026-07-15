use std::rc::Rc;

use gpui::{
    Animation, AnimationExt, App, Div, ElementId, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, ParentElement, Pixels, RenderOnce, Rgba,
    SharedString, Styled, Window, deferred, div,
};

use crate::palette::{ForgePalette, with_alpha};

const ENTER_MS: u64 = 200;

const OVERLAY_PRIORITY: usize = 1;

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OverlayPosition {
    Center,
    /// Docked full-height to the left edge; the `Pixels` is the panel width and the
    /// slide-in distance.
    Left(Pixels),
    /// Docked full-height to the right edge; the `Pixels` is the panel width and the
    /// slide-in distance.
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
    escape_focus: Option<FocusHandle>,
}

/// Defaults to centred with no dismissal wired; add position and dismissal via the
/// builder methods.
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
    #[must_use]
    pub fn position(mut self, position: OverlayPosition) -> Self {
        self.position = position;
        self
    }

    /// Wires scrim-click dismissal; Escape also dismisses only once
    /// [`Overlay::dismiss_on_escape`] is set.
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

    /// The caller must focus `focus_handle` when the overlay opens or Escape stays inert
    /// (gpui routes keys only down the focus path); scrim-click dismissal is unaffected.
    #[must_use]
    pub fn dismiss_on_escape(mut self, focus_handle: &FocusHandle) -> Self {
        self.escape_focus = Some(focus_handle.clone());
        self
    }

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
