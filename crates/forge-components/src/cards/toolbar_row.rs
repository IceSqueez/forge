use gpui::{
    AnyElement, App, IntoElement, ParentElement, Pixels, RenderOnce, Rgba, Styled, Window, div, px,
};

use crate::palette::ForgePalette;
use crate::presentation::ActivePresentation;
use crate::tokens::{BORDER_THIN, Density, Spacing, spacing};

#[derive(IntoElement)]
pub struct ToolbarRow {
    left: AnyElement,
    right: AnyElement,
    attached: Option<(Rgba, Rgba)>,
    density: Option<Density>,
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
        density: None,
        pad_top: None,
        pad_bottom: None,
        pad_x: None,
        gap: None,
        flex_none: false,
    }
}

impl ToolbarRow {
    #[must_use]
    pub fn attached(mut self, palette: &ForgePalette) -> Self {
        self.attached = Some((palette.elevated, palette.border_regular));
        self
    }

    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = Some(density);
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
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let d = self.density.unwrap_or_else(|| cx.density());
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
