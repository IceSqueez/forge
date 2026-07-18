use gpui::{
    App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels, RenderOnce,
    Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::status::status_dot;
use crate::tokens::{DEFAULT_BODY_FAMILY, Density, Spacing, spacing};

type ChipClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

const CHIP_DOT: Pixels = px(5.0);
const CHIP_GAP: Pixels = px(5.0);
const CHIP_PAD_Y: Pixels = px(3.0);
const CHIP_PAD_X: Pixels = px(9.0);
const CHIP_RADIUS: Pixels = px(11.0);
const CHIP_FS: Pixels = px(11.0);

#[derive(Clone, Copy)]
pub enum ChipGlyph {
    None,
    Dot(Rgba),
    Icon(Icon, Rgba),
    DotIcon(Rgba, Icon),
}

#[derive(IntoElement)]
pub struct Chip {
    label: SharedString,
    glyph: ChipGlyph,
    background: Option<Rgba>,
    text_color: Rgba,
    density: Density,
    id: Option<ElementId>,
    on_click: Option<ChipClick>,
}

pub fn chip(
    label: impl Into<SharedString>,
    glyph: ChipGlyph,
    active: bool,
    palette: &ForgePalette,
) -> Chip {
    let background = if active {
        Some(palette.surface_overlay)
    } else {
        None
    };
    let text_color = if active {
        palette.text_primary
    } else {
        palette.text_secondary
    };
    Chip {
        label: label.into(),
        glyph,
        background,
        text_color,
        density: Density::default(),
        id: None,
        on_click: None,
    }
}

impl Chip {
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
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

impl RenderOnce for Chip {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let text_color = self.text_color;
        let _ = self.density;

        let mut root = div()
            .flex()
            .items_center()
            .gap(CHIP_GAP)
            .py(CHIP_PAD_Y)
            .px(CHIP_PAD_X)
            .rounded(CHIP_RADIUS);

        if let Some(background) = self.background {
            root = root.bg(background);
        }

        match self.glyph {
            ChipGlyph::None => {}
            ChipGlyph::Dot(color) => root = root.child(status_dot(color, CHIP_DOT)),
            ChipGlyph::Icon(glyph, color) => root = root.child(icon(glyph, CHIP_FS, color)),
            ChipGlyph::DotIcon(dot_color, glyph) => {
                root = root
                    .child(status_dot(dot_color, CHIP_DOT))
                    .child(icon(glyph, CHIP_FS, text_color));
            }
        }

        root = root.child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(CHIP_FS)
                .text_color(text_color)
                .child(self.label),
        );

        match (self.id, self.on_click) {
            (Some(id), Some(handler)) => root
                .id(id)
                .cursor_pointer()
                .on_click(handler)
                .into_any_element(),
            _ => root.into_any_element(),
        }
    }
}

pub fn filter_chip_row(chips: Vec<Chip>, density: Density) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, density))
        .children(chips)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    // Pins the selected-vs-unselected contract of `chip`: active fills with
    // `surface_overlay` and inks `text_primary`; inactive carries no fill and inks
    // `text_secondary`. Asserted against a real palette whose fill/ink fields hold
    // distinct hues (surface_overlay 0x313244 ≠ base; text_primary ≠ text_secondary
    // ≠ text_muted), so mis-wiring any arm to a neighbouring field - active→base,
    // ink→text_muted, or dropping/adding the fill - fails here. Not a literal
    // restatement: the child module reaches the private resolved fields directly.
    #[test]
    fn chip_resolves_fill_and_ink_from_active_state() {
        let p = &CATPPUCCIN_MOCHA;
        for (active, expected_bg, expected_ink) in [
            (true, Some(p.surface_overlay), p.text_primary),
            (false, None, p.text_secondary),
        ] {
            let c = chip("filter", ChipGlyph::None, active, p);
            assert_eq!(c.background, expected_bg, "background for active={active}");
            assert_eq!(c.text_color, expected_ink, "text_color for active={active}");
        }
    }
}
