use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
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

/// Boxed click handler carried by a pressable row-card. gpui passes the click
/// event plus the window and app contexts, through which the caller reaches its
/// entity.
type RowClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// A fully transparent fill/border — the row-card's idle background and its
/// unselected border color. The source's idle look is `Color::TRANSPARENT`, an
/// off-palette sentinel rather than a theme field, so it is carried as a literal.
const TRANSPARENT: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};

/// Border-rule width the row draws in every state. The idle border paints
/// transparent so it reserves the inset up front — selecting swaps only the color,
/// never the geometry, so the row never shifts as it selects. The source pins this
/// at a literal 2px rule (off the sub-pixel `BORDER_THIN` / 1px `BORDER_ACCENT`
/// scale), so it is carried as a literal.
const ROW_BORDER: Pixels = px(2.0);

/// Gap between a row's title and its meta line — a tight two-line stack the source
/// pins at a literal 2px, off the `Spacing` scale.
const TITLE_META_GAP: Pixels = px(2.0);

/// The three interaction states a row-card paints in. `Selected` wins over
/// `Hover`: a selected row keeps its selected fill and accent border while hovered.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RowState {
    Idle,
    Hover,
    Selected,
}

/// The resolved fills, accent and ink a row-card paints across its idle, hover and
/// selected states — pinned to concrete `ForgePalette` fields up front so the built
/// [`RowCard`] carries no palette borrow (same discipline as [`crate::buttons`]).
#[derive(Clone, Copy)]
pub(crate) struct RowCardColors {
    /// Idle (unselected, un-hovered) fill. Defaults to transparent so the row
    /// blends with its parent surface.
    pub(crate) idle_bg: Rgba,
    /// Fill under the pointer while unselected.
    pub(crate) hover_bg: Rgba,
    /// Fill when selected (wins over hover).
    pub(crate) selected_bg: Rgba,
    /// Border color when selected.
    pub(crate) accent: Rgba,
    /// Persistent unselected border color; `None` renders a transparent border (the
    /// flat list-row look), `Some` turns the row into a bordered card.
    pub(crate) idle_border: Option<Rgba>,
    /// Row text ink, cascaded to the title/meta/trailing children.
    pub(crate) text: Rgba,
}

impl RowCardColors {
    /// Resolves the `(background, border)` pair the row paints in `state`. Selected
    /// fills `selected_bg` and draws the `accent` border regardless of hover; idle
    /// and hover share the (possibly transparent) `idle_border`, differing only in
    /// fill. A `None` border means the row draws its rule transparent.
    pub(crate) fn resolve(&self, state: RowState) -> (Rgba, Option<Rgba>) {
        match state {
            RowState::Idle => (self.idle_bg, self.idle_border),
            RowState::Hover => (self.hover_bg, self.idle_border),
            RowState::Selected => (self.selected_bg, Some(self.accent)),
        }
    }
}

/// Shared list-row surface: a `leading` visual (status dot or icon), a `title` plus
/// optional `meta` line, and an optional `trailing` control cluster (toggle, badge
/// or overflow menu). Standardizes the row scaffolding — spacing, padding, the
/// selected accent border and the whole-row hover/press affordance — so list
/// screens stop hand-rolling it.
///
/// Built via [`row_card`]. Interactive rows ([`RowCard::on_click`]) hover to a
/// subtle `surface_overlay` and, when [`RowCard::selected`], fill `elevated` with a
/// full `brand` accent border. `leading`/`title`/`meta`/`trailing` accept arbitrary
/// elements so a row can host an inline rename field or a badge cluster verbatim.
/// The idle border always paints (transparent by default) so selecting swaps color
/// without shifting the row. [`RowCard::bordered`] gives it a persistent border and
/// rounded corners, turning the flat list-row into a bordered card (selected still
/// wins, swapping the border for the accent).
#[derive(IntoElement)]
pub struct RowCard {
    leading: Option<AnyElement>,
    title: AnyElement,
    meta: Option<AnyElement>,
    trailing: Option<AnyElement>,
    selected: bool,
    pub(crate) colors: RowCardColors,
    border_width: Pixels,
    corner_radius: Pixels,
    density: Density,
    id: Option<ElementId>,
    on_click: Option<RowClick>,
}

/// Start a row-card carrying `title`. Defaults: no leading/meta/trailing, a
/// transparent idle background, `elevated` selected fill, `surface_overlay` hover
/// fill, `brand` accent border, a `ROW_BORDER` (2px) rule, square (flat) corners,
/// `Spacing::Xs`/`Spacing::Md` inset at the `Cozy` step, `text_primary` ink, not
/// interactive. Fills and ink resolve from `palette` up front so the built value
/// carries no palette borrow.
pub fn row_card(title: impl IntoElement, palette: &ForgePalette) -> RowCard {
    RowCard {
        leading: None,
        title: title.into_any_element(),
        meta: None,
        trailing: None,
        selected: false,
        colors: RowCardColors {
            idle_bg: TRANSPARENT,
            hover_bg: palette.surface_overlay,
            selected_bg: palette.elevated,
            accent: palette.brand,
            idle_border: None,
            text: palette.text_primary,
        },
        border_width: ROW_BORDER,
        corner_radius: px(0.0),
        density: Density::default(),
        id: None,
        on_click: None,
    }
}

impl RowCard {
    /// Leading visual placed before the title column (status dot or icon).
    #[must_use]
    pub fn leading(mut self, el: impl IntoElement) -> Self {
        self.leading = Some(el.into_any_element());
        self
    }

    /// Secondary line rendered under the title (kind id, summary, path).
    #[must_use]
    pub fn meta(mut self, el: impl IntoElement) -> Self {
        self.meta = Some(el.into_any_element());
        self
    }

    /// Trailing control cluster pinned to the row's right edge.
    #[must_use]
    pub fn trailing(mut self, el: impl IntoElement) -> Self {
        self.trailing = Some(el.into_any_element());
        self
    }

    /// Marks the row selected: fills `selected_bg` and draws a full accent border,
    /// overriding any hover feedback.
    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Overrides the accent color used for the selected border (defaults to
    /// `brand`).
    #[must_use]
    pub fn accent(mut self, color: Rgba) -> Self {
        self.colors.accent = color;
        self
    }

    /// Idle (unselected) background fill; defaults to transparent so the row blends
    /// with its parent surface.
    #[must_use]
    pub fn idle_background(mut self, color: Rgba) -> Self {
        self.colors.idle_bg = color;
        self
    }

    /// Overrides the density used to scale the row's padding and inter-cell gap. A
    /// bare [`row_card`] resolves these at `Density::Cozy`, which reproduces the
    /// source's fixed inset exactly.
    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    /// Gives the row a persistent (unselected) border plus rounded corners, turning
    /// the flat list-row into a bordered card. The `selected` state still wins,
    /// swapping the border color for the accent.
    #[must_use]
    pub fn bordered(mut self, color: Rgba, width: Pixels, radius: Pixels) -> Self {
        self.colors.idle_border = Some(color);
        self.border_width = width;
        self.corner_radius = radius;
        self
    }

    /// Makes the row a whole-row press target. gpui needs a stable [`ElementId`] to
    /// promote the frame to a stateful, clickable element, so the caller supplies
    /// one alongside the handler (which mutates its own entity via the passed `cx`).
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

impl RenderOnce for RowCard {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let d = self.density;
        let colors = self.colors;
        let selected = self.selected;

        // Two-line title stack: title over an optional meta line, a tight 2px gap
        // apart so a leading-glyph-less and a glyphed row share the same title x.
        let mut title_col = div()
            .flex()
            .flex_col()
            .gap(TITLE_META_GAP)
            .child(self.title);
        if let Some(meta) = self.meta {
            title_col = title_col.child(meta);
        }

        // Rest-state fill + border. Selected wins here; the hover delta is applied
        // only in the interactive, non-selected arm below (the source's container
        // branch has no hover at all).
        let rest_state = if selected {
            RowState::Selected
        } else {
            RowState::Idle
        };
        let (rest_bg, rest_border) = colors.resolve(rest_state);

        // The whole row: [leading?] [title (fills)] [trailing?], vertically centered,
        // one `Spacing::Xs` gap between cells, with the frame's padding/border/fill.
        let mut root = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, d))
            .py(spacing(Spacing::Xs, d))
            .px(spacing(Spacing::Md, d))
            .rounded(self.corner_radius)
            .border(self.border_width)
            .border_color(rest_border.unwrap_or(TRANSPARENT))
            .bg(rest_bg)
            .text_color(colors.text);

        if let Some(leading) = self.leading {
            root = root.child(
                div()
                    .flex()
                    .items_center()
                    .pr(spacing(Spacing::Xs, d))
                    .child(leading),
            );
        }
        root = root.child(div().flex_1().child(title_col));
        if let Some(trailing) = self.trailing {
            root = root.child(div().flex().items_center().child(trailing));
        }

        match (self.id, self.on_click) {
            (Some(id), Some(handler)) => {
                let mut r = root.id(id).cursor_pointer();
                // Selected fill wins over hover, so only an unselected interactive
                // row lifts to its hover fill under the pointer / while pressed. The
                // fill is resolved through the same state resolver (the border is
                // unchanged on hover, so it is dropped here).
                if !selected {
                    let (hover_bg, _) = colors.resolve(RowState::Hover);
                    r = r
                        .hover(move |s| s.bg(hover_bg))
                        .active(move |s| s.bg(hover_bg));
                }
                r.on_click(handler).into_any_element()
            }
            _ => root.into_any_element(),
        }
    }
}
