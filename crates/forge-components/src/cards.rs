use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Pixels, RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled,
    Window, div, px,
};

use crate::icons::{Icon, icon, spinner};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS, FONT_XXS,
    Radius, Spacing, radius, spacing,
};

#[derive(IntoElement)]
pub struct FieldLabel {
    label: SharedString,
    control: AnyElement,
    tone: Rgba,
    size: Pixels,
    gap: Pixels,
}

pub fn field_label(
    palette: &ForgePalette,
    label: impl Into<SharedString>,
    control: impl IntoElement,
) -> FieldLabel {
    FieldLabel {
        label: label.into(),
        control: control.into_any_element(),
        tone: palette.text_faint,
        size: FONT_XXS,
        gap: spacing(Spacing::Xxs, Density::Cozy),
    }
}

impl FieldLabel {
    #[must_use]
    pub fn tone(mut self, color: Rgba) -> Self {
        self.tone = color;
        self
    }

    #[must_use]
    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.gap = spacing(Spacing::Xxs, density);
        self
    }
}

impl RenderOnce for FieldLabel {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(self.gap)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(self.size)
                    .text_color(self.tone)
                    .child(self.label),
            )
            .child(self.control)
    }
}

pub fn field_title(text: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_BODY_FAMILY)
        .font_weight(FontWeight::MEDIUM)
        .text_size(FONT_SM)
        .text_color(palette.text_primary)
        .child(text.into())
}

pub fn field_hint(text: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(text.into())
}

const SETTING_LABEL_GAP: Pixels = px(2.0);

pub fn setting_row(
    title: impl Into<SharedString>,
    hint: Option<SharedString>,
    control: impl IntoElement,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    let mut labels = div()
        .flex()
        .flex_col()
        .gap(SETTING_LABEL_GAP)
        .child(field_title(title, palette));
    if let Some(hint) = hint {
        labels = labels.child(field_hint(hint, palette));
    }

    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(spacing(Spacing::Md, density))
        .child(labels)
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
    full_height: bool,
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
        full_height: false,
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
    pub fn full_height(mut self) -> Self {
        self.full_height = true;
        self
    }

    #[must_use]
    pub fn background(mut self, color: Rgba) -> Self {
        self.background = color;
        self
    }

    #[must_use]
    pub fn border_color(mut self, color: Rgba) -> Self {
        self.border = color;
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
        if self.full_height {
            root = root.h_full();
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

const EMPTY_STATE_GLYPH: Pixels = px(24.0);
const EMPTY_STATE_SPINNER: Pixels = px(20.0);

#[derive(IntoElement)]
pub struct EmptyState {
    message: SharedString,
    glyph: Option<Icon>,
    loading: Option<ElementId>,
    cta: Option<AnyElement>,
    density: Density,
    message_color: Rgba,
    glyph_color: Rgba,
    spinner_color: Rgba,
}

pub fn empty_state(message: impl Into<SharedString>, palette: &ForgePalette) -> EmptyState {
    EmptyState {
        message: message.into(),
        glyph: None,
        loading: None,
        cta: None,
        density: Density::Cozy,
        message_color: palette.text_muted,
        glyph_color: palette.text_faint,
        spinner_color: palette.text_muted,
    }
}

impl EmptyState {
    #[must_use]
    pub fn glyph(mut self, glyph: Icon) -> Self {
        self.glyph = Some(glyph);
        self
    }

    /// Swaps the static glyph for an animated `spinner`; `id` must be unique per live instance.
    #[must_use]
    pub fn loading(mut self, id: impl Into<ElementId>) -> Self {
        self.loading = Some(id.into());
        self
    }

    #[must_use]
    pub fn cta(mut self, cta: impl IntoElement) -> Self {
        self.cta = Some(cta.into_any_element());
        self
    }

    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let d = self.density;

        let leading: Option<AnyElement> = match self.loading {
            Some(id) => Some(
                spinner(id, Icon::Loader2, EMPTY_STATE_SPINNER, self.spinner_color)
                    .into_any_element(),
            ),
            None => self
                .glyph
                .map(|g| icon(g, EMPTY_STATE_GLYPH, self.glyph_color).into_any_element()),
        };

        let mut col = div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(spacing(Spacing::Sm, d))
            .py(spacing(Spacing::Lg, d));

        if let Some(leading) = leading {
            col = col.child(leading);
        }
        col = col.child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(self.message_color)
                .child(self.message),
        );
        if let Some(cta) = self.cta {
            col = col.child(cta);
        }
        col
    }
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
    pub(crate) hover_border: Rgba,
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
    align_top: bool,
    pad_v: Option<Pixels>,
    pad_h: Option<Pixels>,
    reveal_trailing_group: Option<SharedString>,
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
            hover_border: palette.border_input,
            selected_bg: palette.elevated,
            accent: palette.brand,
            idle_border: None,
            text: palette.text_primary,
        },
        border_width: ROW_BORDER,
        corner_radius: px(0.0),
        density: Density::default(),
        align_top: false,
        pad_v: None,
        pad_h: None,
        reveal_trailing_group: None,
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

    #[must_use]
    pub fn padding_xy(mut self, vertical: Pixels, horizontal: Pixels) -> Self {
        self.pad_v = Some(vertical);
        self.pad_h = Some(horizontal);
        self
    }

    /// Top-aligns leading / body / trailing instead of centering - for multi-line
    /// cards whose leading tile and trailing glyph should sit at the top edge.
    #[must_use]
    pub fn align_top(mut self) -> Self {
        self.align_top = true;
        self
    }

    /// Keeps the trailing element hidden (its width still reserved) until the pointer
    /// enters the row, then reveals it. `group` must be unique per rendered row.
    #[must_use]
    pub fn trailing_reveal(mut self, group: impl Into<SharedString>) -> Self {
        self.reveal_trailing_group = Some(group.into());
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

        let aligned = div().w_full().flex();
        let aligned = if self.align_top {
            aligned.items_start()
        } else {
            aligned.items_center()
        };
        let mut root = aligned
            .gap(spacing(Spacing::Xs, d))
            .py(self.pad_v.unwrap_or_else(|| spacing(Spacing::Xs, d)))
            .px(self.pad_h.unwrap_or_else(|| spacing(Spacing::Md, d)))
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
            let slot = match &self.reveal_trailing_group {
                Some(group) => {
                    root = root.group(group.clone());
                    div()
                        .flex()
                        .items_center()
                        .invisible()
                        .group_hover(group.clone(), |s| s.visible())
                        .child(trailing)
                }
                None => div().flex().items_center().child(trailing),
            };
            root = root.child(slot);
        }

        match (self.id, self.on_click) {
            (Some(id), Some(handler)) => {
                let mut r = root.id(id).cursor_pointer();
                if !selected {
                    if rest_border.is_some() {
                        let hover_border = colors.hover_border;
                        r = r
                            .hover(move |s| s.border_color(hover_border))
                            .active(move |s| s.border_color(hover_border));
                    } else {
                        let (hover_bg, _) = colors.resolve(RowState::Hover);
                        r = r
                            .hover(move |s| s.bg(hover_bg))
                            .active(move |s| s.bg(hover_bg));
                    }
                }
                r.on_click(handler).into_any_element()
            }
            _ => root.into_any_element(),
        }
    }
}

const NAV_CHEVRON: Pixels = px(16.0);

pub fn nav_card(
    leading: impl IntoElement,
    body: impl IntoElement,
    palette: &ForgePalette,
) -> RowCard {
    row_card(body, palette)
        .leading(leading)
        .trailing(icon(Icon::ChevronRight, NAV_CHEVRON, palette.text_faint))
        .bordered(palette.border_regular, BORDER_THIN, radius(Radius::Md))
        .idle_background(palette.elevated)
        .align_top()
}

#[derive(IntoElement)]
pub struct ToolbarRow {
    left: AnyElement,
    right: AnyElement,
    attached: Option<(Rgba, Rgba)>,
    density: Density,
    pad_top: Option<Pixels>,
    pad_bottom: Option<Pixels>,
    pad_x: Option<Pixels>,
    gap: Option<Pixels>,
    flex_none: bool,
}

pub fn toolbar_row(left: impl IntoElement, right: impl IntoElement) -> ToolbarRow {
    ToolbarRow {
        left: left.into_any_element(),
        right: right.into_any_element(),
        attached: None,
        density: Density::default(),
        pad_top: None,
        pad_bottom: None,
        pad_x: None,
        gap: None,
        flex_none: false,
    }
}

impl ToolbarRow {
    /// Elevated fill + bottom rule - the bar reads as chrome docked above the body.
    #[must_use]
    pub fn attached(mut self, palette: &ForgePalette) -> Self {
        self.attached = Some((palette.elevated, palette.border_regular));
        self
    }

    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    #[must_use]
    pub fn py(mut self, pad: Pixels) -> Self {
        self.pad_top = Some(pad);
        self.pad_bottom = Some(pad);
        self
    }

    #[must_use]
    pub fn pb(mut self, pad: Pixels) -> Self {
        self.pad_bottom = Some(pad);
        self
    }

    #[must_use]
    pub fn px(mut self, pad: Pixels) -> Self {
        self.pad_x = Some(pad);
        self
    }

    #[must_use]
    pub fn gap(mut self, gap: Pixels) -> Self {
        self.gap = Some(gap);
        self
    }

    #[must_use]
    pub fn flex_none(mut self) -> Self {
        self.flex_none = true;
        self
    }
}

impl RenderOnce for ToolbarRow {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let d = self.density;
        let attached = self.attached.is_some();
        let default_v = if attached {
            spacing(Spacing::Xs, d)
        } else {
            px(0.0)
        };
        let default_x = if attached {
            spacing(Spacing::Md, d)
        } else {
            px(0.0)
        };

        let mut root = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .pt(self.pad_top.unwrap_or(default_v))
            .pb(self.pad_bottom.unwrap_or(default_v))
            .px(self.pad_x.unwrap_or(default_x));

        if self.flex_none {
            root = root.flex_none();
        }
        if let Some(gap) = self.gap {
            root = root.gap(gap);
        }
        if let Some((bg, border)) = self.attached {
            root = root.bg(bg).border_b(BORDER_THIN).border_color(border);
        }

        root.child(self.left).child(self.right)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::FORGE_DEFAULT as P;

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
