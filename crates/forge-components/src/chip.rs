use gpui::{
    App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels, RenderOnce,
    Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::status::status_dot;
use crate::tokens::{DEFAULT_BODY_FAMILY, Density, FONT_XS, Radius, Spacing, radius, spacing};

/// Boxed click handler carried by a pressable chip. gpui passes the click event
/// plus the window and app contexts, through which the caller reaches its entity.
type ChipClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Leading-affordance diameter for a chip's dot. Mirrors the source's fixed 5px
/// disc — a chip dot is a density-neutral marker, so it sits off the `Spacing`
/// scale as a literal, exactly like the connection dot in [`crate::status`].
const CHIP_DOT: Pixels = px(5.0);

/// The optional leading affordance a chip renders before its label.
///
/// `DotIcon` is the design's category-filter shape: a colored [`status_dot`]
/// (the accent hue lives here, via a `ForgePalette` field) followed by a
/// **monochrome** icon tinted with the chip's own text color — the icon never
/// takes the accent. `Icon` by contrast carries its own explicit tint.
#[derive(Clone, Copy)]
pub enum ChipGlyph {
    None,
    Dot(Rgba),
    Icon(Icon, Rgba),
    /// A colored status dot plus a monochrome icon (the icon inherits the
    /// chip's text color), matching the design's category filter chips.
    DotIcon(Rgba, Icon),
}

/// A pill-shaped filter chip: an optional leading glyph plus a label, with a
/// selected (`active`) and unselected state.
///
/// Selected fills with `surface_overlay` and inks its text `text_primary`;
/// unselected is transparent with `text_secondary` text. Attach [`Chip::on_click`]
/// to make it pressable (the source's `Some(on_press)` case); leave it off for a
/// static chip (the `None` case).
#[derive(IntoElement)]
pub struct Chip {
    label: SharedString,
    glyph: ChipGlyph,
    /// Resolved fill: `Some(surface_overlay)` when selected, `None` otherwise.
    background: Option<Rgba>,
    /// Resolved label/monochrome-icon ink for the current state.
    text_color: Rgba,
    density: Density,
    id: Option<ElementId>,
    on_click: Option<ChipClick>,
}

/// Builds a chip in the given selected state, resolving its fill and ink from the
/// active theme up front so the returned value carries no palette borrow.
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
    /// Overrides the density used to scale padding and the glyph→label gap. A
    /// bare [`chip`] resolves these at `Density::Cozy`.
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    /// Makes the chip pressable. gpui needs a stable [`ElementId`] to promote the
    /// pill to a stateful, clickable element, so the caller supplies one
    /// alongside the handler (which mutates its own entity via the passed `cx`).
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
        let gap = spacing(Spacing::Xxs, self.density);

        let mut root = div()
            .flex()
            .items_center()
            .gap(gap)
            .py(spacing(Spacing::Xxs, self.density))
            .px(spacing(Spacing::Sm, self.density))
            .rounded(radius(Radius::Pill));

        if let Some(background) = self.background {
            root = root.bg(background);
        }

        match self.glyph {
            ChipGlyph::None => {}
            ChipGlyph::Dot(color) => root = root.child(status_dot(color, CHIP_DOT)),
            ChipGlyph::Icon(glyph, color) => root = root.child(icon(glyph, FONT_XS, color)),
            ChipGlyph::DotIcon(dot_color, glyph) => {
                root = root
                    .child(status_dot(dot_color, CHIP_DOT))
                    .child(icon(glyph, FONT_XS, text_color));
            }
        }

        root = root.child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
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

/// Lays a set of chips out in a single centered, evenly-gapped row — the design's
/// filter-pill strip. Each [`Chip`] carries its own selected state and optional
/// click handler, so the row only owns the arrangement.
pub fn filter_chip_row(chips: Vec<Chip>, density: Density) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, density))
        .children(chips)
}
