use gpui::{
    AnyElement, App, IntoElement, ParentElement, Pixels, RenderOnce, Rgba, SharedString, Styled,
    Window, div, px,
};

use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS, Radius,
    Spacing, radius, spacing,
};

/// Column gap between a [`metric_card`]'s label, value and sublabel lines. The
/// source pins this at a literal 4px — a tight caption stack that sits off the
/// `Spacing` scale as a fixed metric, so it is carried as a literal.
const METRIC_LINE_GAP: Pixels = px(4.0);

/// Elevated surface panel matching the design-system card.
///
/// Built via [`card`]. A bordered, rounded container that places its child
/// verbatim — no forced flex wrapper — so callers own their inner layout. The
/// resolved fill, border and ink are read from a [`ForgePalette`] up front, so
/// the built value carries no palette borrow. This is the static container
/// surface only; the hover/press-reactive card lives with the interactive
/// row-card family.
#[derive(IntoElement)]
pub struct Card {
    child: AnyElement,
    pad_v: Pixels,
    pad_h: Pixels,
    top_radius: Pixels,
    bottom_radius: Pixels,
    full_width: bool,
    background: Rgba,
    border: Rgba,
    text_color: Rgba,
}

/// Wrap `child` in a standard card surface. Defaults reproduce the baseline
/// chrome: `Spacing::Md` padding on every side, `Radius::Md` on every corner,
/// `elevated` background, a thin `border_regular` border, shrink-to-content
/// width, `text_primary` ink. Padding is density-neutral (the source fixes it at
/// the `Cozy` step), matching the panel's fixed inset.
pub fn card(child: impl IntoElement, palette: &ForgePalette) -> Card {
    let pad = spacing(Spacing::Md, Density::Cozy);
    let r = radius(Radius::Md);
    Card {
        child: child.into_any_element(),
        pad_v: pad,
        pad_h: pad,
        top_radius: r,
        bottom_radius: r,
        full_width: false,
        background: palette.elevated,
        border: palette.border_regular,
        text_color: palette.text_primary,
    }
}

impl Card {
    /// Override inner padding uniformly; pass `px(0.0)` for a flush, zero-inset
    /// surface.
    #[must_use]
    pub fn padding(mut self, padding: Pixels) -> Self {
        self.pad_v = padding;
        self.pad_h = padding;
        self
    }

    /// Override inner padding per axis: `vertical` insets top and bottom,
    /// `horizontal` insets left and right — for a header bar that hugs its row
    /// vertically while keeping the panel's horizontal inset.
    #[must_use]
    pub fn padding_xy(mut self, vertical: Pixels, horizontal: Pixels) -> Self {
        self.pad_v = vertical;
        self.pad_h = horizontal;
        self
    }

    /// Fill the available width instead of shrinking to the child (the default).
    #[must_use]
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    /// Override the surface background (defaults to `elevated`).
    #[must_use]
    pub fn background(mut self, color: Rgba) -> Self {
        self.background = color;
        self
    }

    /// Uniform corner radius on all four corners.
    #[must_use]
    pub fn radius(mut self, r: Radius) -> Self {
        let v = radius(r);
        self.top_radius = v;
        self.bottom_radius = v;
        self
    }

    /// Split the corner radius: `top` rounds the header edge, `bottom` the body
    /// edge — used to butt a header bar flush against the panel below it. Pass
    /// `px(0.0)` for a square edge (feed a token via [`radius`] for rounded ones).
    #[must_use]
    pub fn split_radius(mut self, top: Pixels, bottom: Pixels) -> Self {
        self.top_radius = top;
        self.bottom_radius = bottom;
        self
    }
}

impl RenderOnce for Card {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mut root = div()
            .py(self.pad_v)
            .px(self.pad_h)
            .rounded_t(self.top_radius)
            .rounded_b(self.bottom_radius)
            .border(BORDER_THIN)
            .border_color(self.border)
            .bg(self.background)
            .text_color(self.text_color)
            .child(self.child);

        if self.full_width {
            root = root.w_full();
        }

        root
    }
}

/// A labeled statistic tile: an uppercase monospace caption over a value, with an
/// optional sublabel line, wrapped in the standard card surface (`elevated` fill,
/// thin `border_regular` border, `Radius::Md`, `Spacing::Md` inset).
///
/// The label inks `text_muted` and renders in the monospace family uppercased;
/// the value inks `text_primary` in the body family; the sublabel inks
/// `sublabel_color` when supplied, falling back to `text_faint`.
pub fn metric_card(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    sublabel: Option<impl Into<SharedString>>,
    sublabel_color: Option<Rgba>,
    palette: &ForgePalette,
) -> impl IntoElement {
    let label_upper = SharedString::from(label.into().to_uppercase());

    let mut col = div()
        .flex()
        .flex_col()
        .gap(METRIC_LINE_GAP)
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(label_upper),
        )
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .child(value.into()),
        );

    if let Some(sub) = sublabel {
        col = col.child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(sublabel_color.unwrap_or(palette.text_faint))
                .child(sub.into()),
        );
    }

    div()
        .p(spacing(Spacing::Md, Density::Cozy))
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.elevated)
        .child(col)
}

/// A single label/value line: the label pinned left inking `text_muted`, the
/// value pinned right inking `text_primary`, both in the body family at
/// `FONT_SM`, vertically centered. The card-content row for a stacked stat list.
pub fn stat_row(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(label.into()),
        )
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .child(value.into()),
        )
}
