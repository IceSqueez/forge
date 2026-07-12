use gpui::{
    App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels, RenderOnce,
    Rgba, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::palette::ForgePalette;
use crate::tokens::{Radius, radius};

/// Boxed click handler carried by a pressable toggle. gpui passes the click event
/// plus the window and app contexts, through which the caller reaches its entity to
/// flip its own bool and `cx.notify()`.
type ToggleClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

// Switch geometry. A toggle is a density-neutral control, so its dimensions sit off
// the `Spacing` scale as fixed literals — the same convention the status/chip dots
// follow. `THUMB_ON_OFFSET` is the thumb's left inset when on:
// TRACK_WIDTH - THUMB_SIZE - THUMB_INSET (32 - 14 - 2), so the thumb clears the
// track's right edge by `THUMB_INSET`, mirroring its `THUMB_INSET` gap on the left
// when off.
const TRACK_WIDTH: Pixels = px(32.0);
const TRACK_HEIGHT: Pixels = px(18.0);
const THUMB_SIZE: Pixels = px(14.0);
const THUMB_INSET: Pixels = px(2.0);
const THUMB_ON_OFFSET: Pixels = px(16.0);

/// The track fill and thumb ink a toggle paints for its current on/off state.
#[derive(Clone, Copy)]
pub(crate) struct ToggleColors {
    pub(crate) track: Rgba,
    pub(crate) thumb: Rgba,
}

/// Resolves a toggle's track and thumb colors from its on/off state.
///
/// On: the track takes `on_track` (the accent, defaulting to `success`) and the
/// thumb inks `shell` — a light disc riding a filled track. Off: the track takes
/// `surface_overlay` and the thumb inks `text_faint` — a muted disc on a recessed
/// track.
pub(crate) fn toggle_colors(on: bool, on_track: Rgba, palette: &ForgePalette) -> ToggleColors {
    if on {
        ToggleColors {
            track: on_track,
            thumb: palette.shell,
        }
    } else {
        ToggleColors {
            track: palette.surface_overlay,
            thumb: palette.text_faint,
        }
    }
}

/// A boolean switch: a pill track with a circular thumb that sits at the leading
/// edge when off and the trailing edge when on.
///
/// The on/off value is caller-owned state — pass it in via [`toggle`]. Attach
/// [`Toggle::on_click`] to make it pressable (the handler flips the caller's bool
/// and `cx.notify()`s); leave it off for a static, read-only switch. Override the
/// on-track accent with [`Toggle::on_color`]; it otherwise defaults to `success`.
#[derive(IntoElement)]
pub struct Toggle {
    on: bool,
    /// Resolved track/thumb colors for the current `on` state.
    colors: ToggleColors,
    id: Option<ElementId>,
    on_click: Option<ToggleClick>,
}

/// Builds a switch in the given on/off state, resolving its track and thumb colors
/// from the active theme up front so the returned value carries no palette borrow.
pub fn toggle(on: bool, palette: &ForgePalette) -> Toggle {
    Toggle {
        on,
        colors: toggle_colors(on, palette.success, palette),
        id: None,
        on_click: None,
    }
}

impl Toggle {
    /// Overrides the on-track accent (the fill shown when on). The off state and the
    /// thumb are unchanged; a bare [`toggle`] uses `success`. No effect while off.
    pub fn on_color(mut self, color: Rgba) -> Self {
        if self.on {
            self.colors.track = color;
        }
        self
    }

    /// Makes the switch pressable. gpui needs a stable [`ElementId`] to promote the
    /// track to a stateful, clickable element, so the caller supplies one alongside
    /// the handler (which mutates its own entity via the passed contexts).
    pub fn on_click(
        mut self,
        id: impl Into<ElementId>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.id = Some(id.into());
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Toggle {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let thumb_offset = if self.on {
            THUMB_ON_OFFSET
        } else {
            THUMB_INSET
        };

        let track = div()
            .relative()
            .w(TRACK_WIDTH)
            .h(TRACK_HEIGHT)
            .rounded(radius(Radius::Pill))
            .bg(self.colors.track)
            .child(
                div()
                    .absolute()
                    .top(THUMB_INSET)
                    .left(thumb_offset)
                    .size(THUMB_SIZE)
                    .rounded(radius(Radius::Pill))
                    .bg(self.colors.thumb),
            );

        match (self.id, self.on_click) {
            (Some(id), Some(handler)) => track
                .id(id)
                .cursor_pointer()
                .on_click(handler)
                .into_any_element(),
            _ => track.into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    /// Pins the on/off → (track, thumb) field mapping AND that the `on_track` accent
    /// is honored only while on. The accent sentinel is `brand`, deliberately distinct
    /// from both the bare-`toggle` default (`success`) and the off-track fill
    /// (`surface_overlay`): so if the off arm wrongly returned `on_track`, the off row
    /// would paint `brand` instead of `surface_overlay` and fail here.
    #[test]
    fn resolves_track_and_thumb_per_state_applying_accent_only_when_on() {
        let p = &CATPPUCCIN_MOCHA;
        let accent = p.brand;

        // Guard: the four keyed hues must be pairwise distinct, else a swapped field
        // assignment in production could satisfy the assertions by coincidence.
        let keyed = [p.brand, p.shell, p.surface_overlay, p.text_faint];
        for (i, a) in keyed.iter().enumerate() {
            for b in &keyed[i + 1..] {
                assert_ne!(a, b, "keyed palette hues must be distinct");
            }
        }

        for (on, want_track, want_thumb) in [
            (true, accent, p.shell),
            (false, p.surface_overlay, p.text_faint),
        ] {
            let c = toggle_colors(on, accent, p);
            assert_eq!(c.track, want_track, "track mismatch for on={on}");
            assert_eq!(c.thumb, want_thumb, "thumb mismatch for on={on}");
        }
    }
}
