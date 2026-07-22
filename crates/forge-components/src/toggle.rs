use gpui::{
    App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels, RenderOnce,
    Rgba, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::palette::ForgePalette;
use crate::tokens::{Radius, radius};

type ToggleClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

// THUMB_ON_OFFSET = TRACK_WIDTH - THUMB_SIZE - THUMB_INSET (32 - 14 - 2): when on, the thumb clears the track's right edge by THUMB_INSET, mirroring its left gap when off.
const TRACK_WIDTH: Pixels = px(32.0);
const TRACK_HEIGHT: Pixels = px(18.0);
const THUMB_SIZE: Pixels = px(14.0);
const THUMB_INSET: Pixels = px(2.0);
const THUMB_ON_OFFSET: Pixels = px(16.0);

#[derive(Clone, Copy)]
pub(crate) struct ToggleColors {
    pub(crate) track: Rgba,
    pub(crate) thumb: Rgba,
}

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

#[derive(IntoElement)]
pub struct Toggle {
    on: bool,
    colors: ToggleColors,
    id: Option<ElementId>,
    on_click: Option<ToggleClick>,
}

pub fn toggle(on: bool, palette: &ForgePalette) -> Toggle {
    Toggle {
        on,
        colors: toggle_colors(on, palette.success, palette),
        id: None,
        on_click: None,
    }
}

impl Toggle {
    /// Overrides the on-track accent; no effect while off.
    pub fn on_color(mut self, color: Rgba) -> Self {
        if self.on {
            self.colors.track = color;
        }
        self
    }

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
    use crate::palette::FORGE_DEFAULT;

    #[test]
    fn resolves_track_and_thumb_per_state_applying_accent_only_when_on() {
        let p = &FORGE_DEFAULT;
        let accent = p.brand;

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
