use gpui::{App, IntoElement, ParentElement, Pixels, RenderOnce, Styled, Window, div, px};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::BORDER_THIN;

const CHECK_BOX: Pixels = px(14.0);
const CHECK_GLYPH: Pixels = px(10.0);
const CHECK_RADIUS: Pixels = px(3.0);

#[derive(IntoElement)]
pub struct Checkbox {
    checked: bool,
    disabled: bool,
    palette: ForgePalette,
}

pub fn checkbox(checked: bool, palette: &ForgePalette) -> Checkbox {
    Checkbox {
        checked,
        disabled: false,
        palette: *palette,
    }
}

impl Checkbox {
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for Checkbox {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let palette = self.palette;
        let fill = if self.disabled {
            palette.disabled
        } else {
            palette.brand
        };
        let frame = div()
            .flex_none()
            .size(CHECK_BOX)
            .flex()
            .items_center()
            .justify_center()
            .rounded(CHECK_RADIUS)
            .border(BORDER_THIN);
        if self.checked {
            frame
                .border_color(fill)
                .bg(fill)
                .child(icon(Icon::Check, CHECK_GLYPH, palette.base))
        } else {
            frame.border_color(palette.border_input)
        }
    }
}
