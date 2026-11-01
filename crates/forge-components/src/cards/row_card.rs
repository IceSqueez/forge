use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::key_activation::KeyActivated;
use crate::palette::ForgePalette;
use crate::presentation::ActivePresentation;
use crate::tokens::{Density, Spacing, scaled, spacing};

type RowClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

const TRANSPARENT: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};

const ROW_BORDER: Pixels = px(2.0);

const TITLE_META_GAP: Pixels = px(2.0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RowState {
    Idle,
    Hover,
}

#[derive(Clone, Copy)]
pub(crate) struct RowCardColors {
    pub(crate) idle_bg: Rgba,
    pub(crate) hover_bg: Rgba,
    pub(crate) hover_border: Rgba,
    pub(crate) idle_border: Option<Rgba>,
    pub(crate) text: Rgba,
}

impl RowCardColors {
    pub(crate) fn resolve(&self, state: RowState) -> (Rgba, Option<Rgba>) {
        match state {
            RowState::Idle => (self.idle_bg, self.idle_border),
            RowState::Hover => (self.hover_bg, self.idle_border),
        }
    }
}

#[derive(IntoElement)]
pub struct RowCard {
    leading: Option<AnyElement>,
    title: AnyElement,
    meta: Option<AnyElement>,
    trailing: Option<AnyElement>,
    pub(crate) colors: RowCardColors,
    border_width: Pixels,
    corner_radius: Pixels,
    density: Option<Density>,
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
        colors: RowCardColors {
            idle_bg: TRANSPARENT,
            hover_bg: palette.surface_overlay,
            hover_border: palette.border_input,
            idle_border: None,
            text: palette.text_primary,
        },
        border_width: ROW_BORDER,
        corner_radius: px(0.0),
        density: None,
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
    pub fn idle_background(mut self, color: Rgba) -> Self {
        self.colors.idle_bg = color;
        self
    }

    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = Some(density);
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

    #[must_use]
    pub fn align_top(mut self) -> Self {
        self.align_top = true;
        self
    }

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
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let d = self.density.unwrap_or_else(|| cx.density());
        let colors = self.colors;

        let mut title_col = div()
            .flex()
            .flex_col()
            .gap(scaled(TITLE_META_GAP, d))
            .child(self.title);
        if let Some(meta) = self.meta {
            title_col = title_col.child(meta);
        }

        let (rest_bg, rest_border) = colors.resolve(RowState::Idle);

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
        root = root.child(div().flex_1().min_w_0().child(title_col));
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
                let mut r = root.id(id).key_activated(colors.text).cursor_pointer();
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
                r.on_click(handler).into_any_element()
            }
            _ => root.into_any_element(),
        }
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

    #[test]
    fn resolve_maps_each_state_to_its_keyed_fill_and_border() {
        let c = default_colors();
        let cases: [(RowState, Rgba, Option<Rgba>); 2] = [
            (RowState::Idle, TRANSPARENT, None),
            (RowState::Hover, P.surface_overlay, None),
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
    fn idle_and_hover_fills_are_distinguishable_hues() {
        let c = default_colors();
        assert!(
            !same_rgba(c.resolve(RowState::Idle).0, c.resolve(RowState::Hover).0),
            "idle and hover fills collapsed"
        );
    }

    #[test]
    fn bordered_keeps_its_border_while_hovered() {
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
    }
}
