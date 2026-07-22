use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, FONT_SM, FONT_XXS, Radius, radius,
};

type RowClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

const DOT_SIZE: Pixels = px(15.0);
const DOT_INNER: Pixels = px(7.0);
const DOT_BORDER: Pixels = px(1.5);
const ROW_GAP: Pixels = px(10.0);
const ROW_PAD_X: Pixels = px(11.0);
const ROW_PAD_Y: Pixels = px(9.0);
const DISABLED_OPACITY: f32 = 0.5;

#[derive(IntoElement)]
pub struct RadioRow {
    id: ElementId,
    selected: bool,
    disabled: bool,
    align_start: bool,
    accent: Rgba,
    dot_size: Pixels,
    dot_inner: Pixels,
    dot_border_width: Pixels,
    dot_unselected: Rgba,
    gap: Pixels,
    pad_x: Pixels,
    pad_y: Pixels,
    corner_radius: Pixels,
    row_border_selected: Pixels,
    row_border_unselected: Pixels,
    row_border_color: Rgba,
    row_bg_selected: Rgba,
    row_bg_unselected: Rgba,
    disabled_opacity: f32,
    content: AnyElement,
    on_click: Option<RowClick>,
}

pub fn radio_row(
    id: impl Into<ElementId>,
    selected: bool,
    accent: Rgba,
    content: impl IntoElement,
    palette: &ForgePalette,
) -> RadioRow {
    RadioRow {
        id: id.into(),
        selected,
        disabled: false,
        align_start: false,
        accent,
        dot_size: DOT_SIZE,
        dot_inner: DOT_INNER,
        dot_border_width: DOT_BORDER,
        dot_unselected: palette.border_regular,
        gap: ROW_GAP,
        pad_x: ROW_PAD_X,
        pad_y: ROW_PAD_Y,
        corner_radius: radius(Radius::Sm),
        row_border_selected: BORDER_THIN,
        row_border_unselected: BORDER_THIN,
        row_border_color: palette.border_regular,
        row_bg_selected: palette.surface_overlay,
        row_bg_unselected: palette.shell,
        disabled_opacity: DISABLED_OPACITY,
        content: content.into_any_element(),
        on_click: None,
    }
}

pub fn radio_row_label(
    label: impl Into<SharedString>,
    hint: Option<SharedString>,
    selected: bool,
    palette: &ForgePalette,
) -> AnyElement {
    let label_color = if selected {
        palette.text_primary
    } else {
        palette.text_secondary
    };

    let mut row = div().flex().items_center().gap(ROW_GAP).child(
        div()
            .flex_1()
            .min_w(px(0.0))
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(label_color)
            .child(label.into()),
    );

    if let Some(hint) = hint {
        row = row.child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(hint),
        );
    }

    row.into_any_element()
}

impl RadioRow {
    #[must_use]
    pub fn dot_metrics(mut self, size: Pixels, inner: Pixels, border_width: Pixels) -> Self {
        self.dot_size = size;
        self.dot_inner = inner;
        self.dot_border_width = border_width;
        self
    }

    #[must_use]
    pub fn dot_unselected(mut self, color: Rgba) -> Self {
        self.dot_unselected = color;
        self
    }

    #[must_use]
    pub fn align_start(mut self) -> Self {
        self.align_start = true;
        self
    }

    #[must_use]
    pub fn gap(mut self, gap: Pixels) -> Self {
        self.gap = gap;
        self
    }

    #[must_use]
    pub fn padding(mut self, x: Pixels, y: Pixels) -> Self {
        self.pad_x = x;
        self.pad_y = y;
        self
    }

    #[must_use]
    pub fn corner_radius(mut self, radius: Pixels) -> Self {
        self.corner_radius = radius;
        self
    }

    #[must_use]
    pub fn row_border(mut self, selected_width: Pixels, unselected_width: Pixels) -> Self {
        self.row_border_selected = selected_width;
        self.row_border_unselected = unselected_width;
        self
    }

    #[must_use]
    pub fn row_border_color(mut self, color: Rgba) -> Self {
        self.row_border_color = color;
        self
    }

    #[must_use]
    pub fn background(mut self, selected: Rgba, unselected: Rgba) -> Self {
        self.row_bg_selected = selected;
        self.row_bg_unselected = unselected;
        self
    }

    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    #[must_use]
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for RadioRow {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mut dot = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(self.dot_size)
            .rounded(radius(Radius::Pill))
            .border(self.dot_border_width)
            .border_color(if self.selected {
                self.accent
            } else {
                self.dot_unselected
            });
        if self.selected {
            dot = dot.child(
                div()
                    .size(self.dot_inner)
                    .rounded(radius(Radius::Pill))
                    .bg(self.accent),
            );
        }
        if self.align_start {
            dot = dot.mt(px(1.0));
        }

        let mut row = div()
            .id(self.id)
            .flex()
            .gap(self.gap)
            .px(self.pad_x)
            .py(self.pad_y)
            .rounded(self.corner_radius)
            .bg(if self.selected {
                self.row_bg_selected
            } else {
                self.row_bg_unselected
            })
            .border(if self.selected {
                self.row_border_selected
            } else {
                self.row_border_unselected
            })
            .border_color(if self.selected {
                self.accent
            } else {
                self.row_border_color
            })
            .child(dot)
            .child(self.content);

        row = if self.align_start {
            row.items_start()
        } else {
            row.items_center()
        };

        if self.disabled {
            row.opacity(self.disabled_opacity).into_any_element()
        } else if let Some(handler) = self.on_click {
            row.cursor_pointer().on_click(handler).into_any_element()
        } else {
            row.into_any_element()
        }
    }
}
