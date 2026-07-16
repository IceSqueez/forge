use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS, FONT_XXS,
    Radius, Spacing, radius, spacing,
};

pub fn field_label(
    palette: &ForgePalette,
    label: impl Into<SharedString>,
    control: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(label.into()),
        )
        .child(control)
}

const METRIC_LINE_GAP: Pixels = px(4.0);

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
    #[must_use]
    pub fn padding(mut self, padding: Pixels) -> Self {
        self.pad_v = padding;
        self.pad_h = padding;
        self
    }

    #[must_use]
    pub fn padding_xy(mut self, vertical: Pixels, horizontal: Pixels) -> Self {
        self.pad_v = vertical;
        self.pad_h = horizontal;
        self
    }

    #[must_use]
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    #[must_use]
    pub fn background(mut self, color: Rgba) -> Self {
        self.background = color;
        self
    }

    #[must_use]
    pub fn radius(mut self, r: Radius) -> Self {
        let v = radius(r);
        self.top_radius = v;
        self.bottom_radius = v;
        self
    }

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

type RowClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

const TRANSPARENT: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};

// Idle paints transparent so selecting swaps only color, never geometry - the row
// never shifts as it selects.
const ROW_BORDER: Pixels = px(2.0);

const TITLE_META_GAP: Pixels = px(2.0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RowState {
    Idle,
    Hover,
    Selected,
}

#[derive(Clone, Copy)]
pub(crate) struct RowCardColors {
    pub(crate) idle_bg: Rgba,
    pub(crate) hover_bg: Rgba,
    pub(crate) selected_bg: Rgba,
    pub(crate) accent: Rgba,
    /// `None` draws a transparent border (flat list-row); `Some` makes it a bordered card.
    pub(crate) idle_border: Option<Rgba>,
    pub(crate) text: Rgba,
}

impl RowCardColors {
    pub(crate) fn resolve(&self, state: RowState) -> (Rgba, Option<Rgba>) {
        match state {
            RowState::Idle => (self.idle_bg, self.idle_border),
            RowState::Hover => (self.hover_bg, self.idle_border),
            RowState::Selected => (self.selected_bg, Some(self.accent)),
        }
    }
}

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
    #[must_use]
    pub fn leading(mut self, el: impl IntoElement) -> Self {
        self.leading = Some(el.into_any_element());
        self
    }

    #[must_use]
    pub fn meta(mut self, el: impl IntoElement) -> Self {
        self.meta = Some(el.into_any_element());
        self
    }

    #[must_use]
    pub fn trailing(mut self, el: impl IntoElement) -> Self {
        self.trailing = Some(el.into_any_element());
        self
    }

    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    #[must_use]
    pub fn accent(mut self, color: Rgba) -> Self {
        self.colors.accent = color;
        self
    }

    #[must_use]
    pub fn idle_background(mut self, color: Rgba) -> Self {
        self.colors.idle_bg = color;
        self
    }

    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    #[must_use]
    pub fn bordered(mut self, color: Rgba, width: Pixels, radius: Pixels) -> Self {
        self.colors.idle_border = Some(color);
        self.border_width = width;
        self.corner_radius = radius;
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

impl RenderOnce for RowCard {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let d = self.density;
        let colors = self.colors;
        let selected = self.selected;

        let mut title_col = div()
            .flex()
            .flex_col()
            .gap(TITLE_META_GAP)
            .child(self.title);
        if let Some(meta) = self.meta {
            title_col = title_col.child(meta);
        }

        let rest_state = if selected {
            RowState::Selected
        } else {
            RowState::Idle
        };
        let (rest_bg, rest_border) = colors.resolve(rest_state);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA as P;

    const EPS: f32 = 1e-6;

    fn same_rgba(a: Rgba, b: Rgba) -> bool {
        (a.r - b.r).abs() < EPS
            && (a.g - b.g).abs() < EPS
            && (a.b - b.b).abs() < EPS
            && (a.a - b.a).abs() < EPS
    }

    /// Assert a resolved border slot: `None` = transparent rule, `Some` compared
    /// channel-wise (including alpha).
    #[allow(clippy::panic)]
    fn assert_border(actual: Option<Rgba>, want: Option<Rgba>, label: &str) {
        match (actual, want) {
            (None, None) => {}
            (Some(got), Some(w)) => assert!(
                same_rgba(got, w),
                "{label}: border hue mismatch - got {got:?}, want {w:?}",
            ),
            _ => panic!("{label}: border presence mismatch - got {actual:?}, want {want:?}"),
        }
    }

    fn default_colors() -> RowCardColors {
        row_card(SharedString::from("row"), &P).colors
    }

    // The load-bearing contract of a default row-card: which `ForgePalette` field
    // each interaction state fills with, and that ONLY the selected state draws the
    // accent border. Fills are compared channel-wise so a mis-wire to a neighbouring
    // field (hover→elevated, selected→surface_overlay) fails - see the distinct-hue
    // guard below, which pins that these three fields really are different.
    #[test]
    fn resolve_maps_each_state_to_its_keyed_fill_and_border() {
        let c = default_colors();
        let cases: [(RowState, Rgba, Option<Rgba>); 3] = [
            (RowState::Idle, TRANSPARENT, None),
            (RowState::Hover, P.surface_overlay, None),
            (RowState::Selected, P.elevated, Some(P.brand)),
        ];
        for (state, want_fill, want_border) in cases {
            let (fill, border) = c.resolve(state);
            assert!(
                same_rgba(fill, want_fill),
                "{state:?}: fill mismatch - got {fill:?}, want {want_fill:?}",
            );
            assert_border(border, want_border, &format!("{state:?}"));
        }
    }

    #[test]
    fn idle_fill_is_fully_transparent_not_an_opaque_field() {
        // Why: idle blends into the parent surface via alpha 0.0, NOT by borrowing an
        // opaque near-black palette field. Pin the literal so wiring idle to any
        // opaque field (which would carry alpha 1.0) fails here.
        let idle_fill = default_colors().resolve(RowState::Idle).0;
        assert_eq!(idle_fill.a, 0.0);
    }

    #[test]
    fn idle_hover_and_selected_fills_are_distinguishable_hues() {
        // Why: the per-state test compares fills channel-wise, so it only catches a
        // mis-wire between states if their source palette fields are actually
        // distinct. Guard that assumption so the main test keeps its teeth.
        let c = default_colors();
        let idle = c.resolve(RowState::Idle).0;
        let hover = c.resolve(RowState::Hover).0;
        let selected = c.resolve(RowState::Selected).0;
        assert!(!same_rgba(idle, hover), "idle and hover fills collapsed");
        assert!(
            !same_rgba(hover, selected),
            "hover and selected fills collapsed"
        );
        assert!(
            !same_rgba(idle, selected),
            "idle and selected fills collapsed"
        );
    }

    #[test]
    fn bordered_gives_idle_a_persistent_border_that_selected_overrides() {
        // `.bordered` promotes the flat list-row to a bordered card: idle and hover
        // now paint the explicit border, but the selected accent must still win.
        let border_color = P.border_regular;
        let c = row_card(SharedString::from("row"), &P)
            .bordered(border_color, px(1.0), px(6.0))
            .colors;
        assert_border(
            c.resolve(RowState::Idle).1,
            Some(border_color),
            "bordered idle",
        );
        assert_border(
            c.resolve(RowState::Hover).1,
            Some(border_color),
            "bordered hover",
        );
        assert_border(
            c.resolve(RowState::Selected).1,
            Some(P.brand),
            "bordered selected",
        );
    }
}
