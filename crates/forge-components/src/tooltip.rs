use gpui::{
    AnyView, App, AppContext, Context, IntoElement, ParentElement, Render, SharedString, Styled,
    Window, div,
};

use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, Density, FONT_XS, Radius, Spacing, radius, spacing,
};

/// The floating label surface a tooltip shows on hover: a compact rounded card
/// lifted off the app on the `elevated` fill with a hairline `border_regular`
/// edge, carrying one line of primary-ink body text. Padding is snug — a shorter
/// vertical inset than horizontal — so the label hugs its text.
///
/// Shared by [`Tooltip`]'s render and exposed on its own so the same surface can
/// be dropped inside a bespoke popover without routing through a view.
pub fn tooltip_surface(
    label: impl Into<SharedString>,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Sm, density))
        .bg(palette.elevated)
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .rounded(radius(Radius::Sm))
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_primary)
        .child(label.into())
}

/// A tooltip content view. gpui attaches a tooltip through a builder closure that
/// yields a fresh view on each hover (`InteractiveElement::tooltip`), so the
/// floating label must be an `Entity`-backed view rather than a bare element.
///
/// Build one with [`tooltip`], then either hand it to `.tooltip(...)` through the
/// ready-made [`tooltip_builder`] closure, or turn it into a view yourself inside
/// a builder closure with [`Tooltip::build`]. The palette is captured by value up
/// front so the view carries no borrow into the deferred hover.
pub struct Tooltip {
    label: SharedString,
    palette: ForgePalette,
    density: Density,
}

/// Starts a [`Tooltip`] for `label`, resolving its inks from `palette`. Defaults to
/// `Density::Cozy`, the neutral scale that reproduces the source surface's padding.
pub fn tooltip(label: impl Into<SharedString>, palette: &ForgePalette) -> Tooltip {
    Tooltip {
        label: label.into(),
        palette: *palette,
        density: Density::Cozy,
    }
}

impl Tooltip {
    /// Overrides the spacing density the padding resolves against.
    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    /// Materialises the tooltip into a view for `.tooltip(...)`. Call this inside a
    /// tooltip builder closure, where gpui hands you the `App` to spawn the view on.
    pub fn build(self, cx: &mut App) -> AnyView {
        cx.new(|_| self).into()
    }
}

impl Render for Tooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        tooltip_surface(self.label.clone(), &self.palette, self.density)
    }
}

/// A ready-made builder closure for `InteractiveElement::tooltip` /
/// `StatefulInteractiveElement::tooltip`: `some_element.tooltip(tooltip_builder("Save", &palette))`.
/// Captures `label` and `palette` by value and re-materialises a fresh [`Tooltip`]
/// view on each hover, as gpui's tooltip contract requires.
pub fn tooltip_builder(
    label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let label = label.into();
    let palette = *palette;
    move |_window, cx| tooltip(label.clone(), &palette).build(cx)
}
