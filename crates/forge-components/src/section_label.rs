use gpui::{
    AnyElement, App, IntoElement, ParentElement, Pixels, RenderOnce, Rgba, SharedString, Styled,
    Window, div, px,
};

use crate::palette::ForgePalette;
use crate::tokens::{Density, FONT_SECTION_LABEL, FONT_XXS, Spacing, mono_family, spacing};

const ROW_MARGIN_TOP: Pixels = px(4.0);
const ROW_MARGIN_BOTTOM: Pixels = px(8.0);

pub fn section_label(label: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(label.into())
}

#[derive(IntoElement)]
pub struct SectionLabel {
    label: SharedString,
    text_color: Rgba,
    trailing: Option<AnyElement>,
    row: bool,
    padded: bool,
}

impl SectionLabel {
    pub fn new(label: impl Into<SharedString>, palette: &ForgePalette) -> Self {
        Self {
            label: label.into(),
            text_color: palette.text_muted,
            trailing: None,
            row: false,
            padded: false,
        }
    }

    pub fn row(mut self) -> Self {
        self.row = true;
        self
    }

    pub fn trailing(mut self, trailing: Option<AnyElement>) -> Self {
        self.trailing = trailing;
        self
    }

    pub fn padded(mut self) -> Self {
        self.padded = true;
        self
    }
}

impl RenderOnce for SectionLabel {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let text = if self.row {
            SharedString::from(self.label.to_uppercase())
        } else {
            self.label
        };
        let size = if self.row {
            FONT_SECTION_LABEL
        } else {
            FONT_XXS
        };
        let caption = div()
            .font_family(mono_family())
            .text_size(size)
            .text_color(self.text_color)
            .child(text);
        let mut container = div();
        if self.padded {
            container = container
                .py(spacing(Spacing::Xs, Density::Cozy))
                .px(spacing(Spacing::Md, Density::Cozy));
        }
        if self.row {
            container = container
                .w_full()
                .mt(ROW_MARGIN_TOP)
                .mb(ROW_MARGIN_BOTTOM)
                .flex()
                .items_center()
                .justify_between();
        }
        container.child(caption).children(self.trailing)
    }
}
