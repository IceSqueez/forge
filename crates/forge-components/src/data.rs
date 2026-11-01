use crate::focus_reveal::OverflowRevealingFocus;
use gpui::{
    AnyElement, App, Context, Div, ElementId, InteractiveElement, IntoElement, ParentElement,
    Pixels, RenderOnce, Rgba, SharedString, Styled, UniformListScrollHandle, Window, div, px,
    relative, uniform_list,
};

use crate::palette::ForgePalette;
use crate::presentation::ActivePresentation;
use crate::tokens::{BORDER_THIN, Density, FONT_XXS, Spacing, spacing};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnWidth {
    Fixed(Pixels),
    Flex(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeaderAlign {
    Start,
    End,
}

pub(crate) struct FlexSpec {
    pub grow: f32,
    pub shrink: f32,
    pub fixed: Option<Pixels>,
}

pub(crate) fn column_flex(width: ColumnWidth) -> FlexSpec {
    match width {
        ColumnWidth::Fixed(p) => FlexSpec {
            grow: 0.0,
            shrink: 0.0,
            fixed: Some(p),
        },
        ColumnWidth::Flex(n) => FlexSpec {
            grow: n,
            shrink: 1.0,
            fixed: None,
        },
    }
}

pub struct Column {
    label: SharedString,
    width: ColumnWidth,
    align: HeaderAlign,
}

pub fn column(label: impl Into<SharedString>, width: ColumnWidth) -> Column {
    Column {
        label: label.into(),
        width,
        align: HeaderAlign::Start,
    }
}

impl Column {
    #[must_use]
    pub fn align_end(mut self) -> Self {
        self.align = HeaderAlign::End;
        self
    }
}

pub struct DataRow {
    cells: Vec<AnyElement>,
    reveal_group: Option<SharedString>,
    highlighted: bool,
}

impl DataRow {
    pub fn new(cells: Vec<AnyElement>) -> Self {
        Self {
            cells,
            reveal_group: None,
            highlighted: false,
        }
    }

    pub fn with_reveal_group(cells: Vec<AnyElement>, group: impl Into<SharedString>) -> Self {
        Self {
            cells,
            reveal_group: Some(group.into()),
            highlighted: false,
        }
    }

    #[must_use]
    pub fn highlighted(mut self, highlighted: bool) -> Self {
        self.highlighted = highlighted;
        self
    }
}

pub fn hover_reveal(content: impl IntoElement, group: impl Into<SharedString>) -> impl IntoElement {
    div()
        .invisible()
        .group_hover(group, |s| s.visible())
        .child(content)
}

#[derive(Clone, Copy)]
struct TablePadding {
    vertical: Pixels,
    horizontal: Pixels,
}

#[derive(Clone, Copy)]
struct TableStyle {
    header_bg: Rgba,
    header_ink: Rgba,
    separator: Rgba,
    header_rule: Option<Rgba>,
    hover: Rgba,
    header_pad: Option<(Pixels, Pixels)>,
    row_pad: Option<(Pixels, Pixels)>,
    cell_gap: Pixels,
    trailing_rule: bool,
}

impl TableStyle {
    fn new(palette: &ForgePalette) -> Self {
        Self {
            header_bg: palette.shell,
            header_ink: palette.text_faint,
            separator: palette.border_regular,
            header_rule: None,
            hover: palette.base,
            header_pad: None,
            row_pad: None,
            cell_gap: px(0.0),
            trailing_rule: true,
        }
    }

    fn padding(custom: Option<(Pixels, Pixels)>, density: Density) -> TablePadding {
        let (vertical, horizontal) =
            custom.unwrap_or((spacing(Spacing::Xs, density), spacing(Spacing::Md, density)));
        TablePadding {
            vertical,
            horizontal,
        }
    }

    fn header_rule_color(&self) -> Rgba {
        self.header_rule.unwrap_or(self.separator)
    }

    fn header(&self, columns: Vec<Column>, density: Density, cx: &App) -> Div {
        let pad = Self::padding(self.header_pad, density);
        let header_ink = self.header_ink;
        let header_cells = columns.into_iter().map(move |c| {
            let label_el = div()
                .font_family(cx.mono_family())
                .text_size(FONT_XXS)
                .truncate()
                .text_color(header_ink)
                .child(c.label);
            let cell = column_cell(c.width, label_el);
            match c.align {
                HeaderAlign::Start => cell,
                HeaderAlign::End => cell.flex().justify_end(),
            }
        });
        div()
            .flex()
            .items_center()
            .w_full()
            .gap(self.cell_gap)
            .py(pad.vertical)
            .px(pad.horizontal)
            .bg(self.header_bg)
            .children(header_cells)
    }

    fn header_rule_line(&self) -> Div {
        div()
            .flex_none()
            .h(BORDER_THIN)
            .w_full()
            .bg(self.header_rule_color())
    }

    fn row(&self, row: DataRow, widths: &[ColumnWidth], density: Density) -> Div {
        let pad = Self::padding(self.row_pad, density);
        let hover = self.hover;
        let cells = row
            .cells
            .into_iter()
            .zip(widths.iter().copied())
            .map(|(cell, width)| column_cell(width, cell));
        let mut row_el = div()
            .flex()
            .items_center()
            .w_full()
            .gap(self.cell_gap)
            .py(pad.vertical)
            .px(pad.horizontal)
            .hover(move |s| s.bg(hover))
            .children(cells);
        if row.highlighted {
            row_el = row_el.bg(hover);
        }
        if let Some(group) = row.reveal_group {
            row_el = row_el.group(group);
        }
        row_el
    }

    fn rule_follows(&self, index: usize, row_count: usize) -> bool {
        self.trailing_rule || index + 1 != row_count
    }
}

macro_rules! table_style_setters {
    () => {
        #[must_use]
        pub fn header_bg(mut self, color: Rgba) -> Self {
            self.style.header_bg = color;
            self
        }

        #[must_use]
        pub fn separator(mut self, color: Rgba) -> Self {
            self.style.separator = color;
            self
        }

        #[must_use]
        pub fn header_rule(mut self, color: Rgba) -> Self {
            self.style.header_rule = Some(color);
            self
        }

        #[must_use]
        pub fn row_hover(mut self, color: Rgba) -> Self {
            self.style.hover = color;
            self
        }

        #[must_use]
        pub fn header_padding(mut self, vertical: Pixels, horizontal: Pixels) -> Self {
            self.style.header_pad = Some((vertical, horizontal));
            self
        }

        #[must_use]
        pub fn row_padding(mut self, vertical: Pixels, horizontal: Pixels) -> Self {
            self.style.row_pad = Some((vertical, horizontal));
            self
        }

        #[must_use]
        pub fn cell_gap(mut self, gap: Pixels) -> Self {
            self.style.cell_gap = gap;
            self
        }

        #[must_use]
        pub fn trailing_rule(mut self, on: bool) -> Self {
            self.style.trailing_rule = on;
            self
        }
    };
}

#[derive(IntoElement)]
pub struct DataTable {
    columns: Vec<Column>,
    rows: Vec<DataRow>,
    style: TableStyle,
    density: Option<Density>,
    scroll_id: Option<ElementId>,
}

pub fn data_table(palette: &ForgePalette, columns: Vec<Column>, rows: Vec<DataRow>) -> DataTable {
    DataTable {
        columns,
        rows,
        style: TableStyle::new(palette),
        density: None,
        scroll_id: None,
    }
}

impl DataTable {
    table_style_setters!();

    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = Some(density);
        self
    }

    #[must_use]
    pub fn scroll_body(mut self, id: impl Into<ElementId>) -> Self {
        self.scroll_id = Some(id.into());
        self
    }
}

fn column_cell(width: ColumnWidth, child: impl IntoElement) -> Div {
    let spec = column_flex(width);
    let mut cell = div().child(child);
    match spec.fixed {
        Some(w) => cell.w(w).flex_none(),
        None => {
            cell = cell.min_w(px(0.0));
            let style = cell.style();
            style.flex_grow = Some(spec.grow);
            style.flex_shrink = Some(spec.shrink);
            style.flex_basis = Some(relative(0.0).into());
            cell
        }
    }
}

impl RenderOnce for DataTable {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let d = self.density.unwrap_or_else(|| cx.density());
        let style = self.style;
        let separator = style.separator;
        let rule = move || div().flex_none().h(BORDER_THIN).w_full().bg(separator);

        let widths: Vec<ColumnWidth> = self.columns.iter().map(|c| c.width).collect();
        let header = style.header(self.columns, d, cx);

        let total = self.rows.len();
        let mut body_children: Vec<AnyElement> = Vec::new();
        for (index, row) in self.rows.into_iter().enumerate() {
            body_children.push(style.row(row, &widths, d).into_any_element());
            if style.rule_follows(index, total) {
                body_children.push(rule().into_any_element());
            }
        }

        let body_inner = div().flex().flex_col().w_full().children(body_children);

        let has_scroll = self.scroll_id.is_some();
        let body = match self.scroll_id {
            Some(id) => div()
                .id(id)
                .flex_1()
                .min_h(px(0.0))
                .w_full()
                .overflow_y_revealing_focus()
                .child(body_inner)
                .into_any_element(),
            None => body_inner.into_any_element(),
        };

        let mut root = div().flex().flex_col().w_full();
        if has_scroll {
            root = root.flex_1().min_h(px(0.0));
        }
        root.child(header)
            .child(style.header_rule_line())
            .child(body)
    }
}

pub struct VirtualTable<'a> {
    id: ElementId,
    columns: Vec<Column>,
    row_count: usize,
    scroll: &'a UniformListScrollHandle,
    density: Density,
    style: TableStyle,
}

pub fn virtual_table<'a>(
    id: impl Into<ElementId>,
    palette: &ForgePalette,
    columns: Vec<Column>,
    row_count: usize,
    scroll: &'a UniformListScrollHandle,
    density: Density,
) -> VirtualTable<'a> {
    VirtualTable {
        id: id.into(),
        columns,
        row_count,
        scroll,
        density,
        style: TableStyle::new(palette),
    }
}

impl<'a> VirtualTable<'a> {
    table_style_setters!();

    pub fn build<V, F>(self, row_builder: F, cx: &mut Context<V>) -> AnyElement
    where
        V: 'static,
        F: Fn(&mut V, usize, &mut Window, &mut Context<V>) -> DataRow + 'static,
    {
        let d = self.density;
        let style = self.style;
        let row_count = self.row_count;

        let widths: Vec<ColumnWidth> = self.columns.iter().map(|c| c.width).collect();
        let header = style.header(self.columns, d, cx);

        let list = uniform_list(
            self.id,
            row_count,
            cx.processor(move |view, range: std::ops::Range<usize>, window, cx| {
                let mut out = Vec::with_capacity(range.len());
                for ix in range {
                    let row = row_builder(view, ix, window, cx);
                    let mut row_el = style.row(row, &widths, d);
                    if style.rule_follows(ix, row_count) {
                        row_el = row_el.border_b(BORDER_THIN).border_color(style.separator);
                    }
                    out.push(row_el);
                }
                out
            }),
        )
        .track_scroll(self.scroll)
        .flex_1()
        .min_h(px(0.0));

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .child(header)
            .child(style.header_rule_line())
            .child(list)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_flex_maps_each_width_to_its_flexbox_spec() {
        let cases = [
            (
                ColumnWidth::Fixed(px(120.0)),
                0.0_f32,
                0.0_f32,
                Some(px(120.0)),
            ),
            (ColumnWidth::Flex(1.0), 1.0, 1.0, None),
            (ColumnWidth::Flex(2.0), 2.0, 1.0, None),
        ];

        for (width, grow, shrink, fixed) in cases {
            let spec = column_flex(width);
            assert_eq!(spec.grow, grow, "grow for {width:?}");
            assert_eq!(spec.shrink, shrink, "shrink for {width:?}");
            assert_eq!(spec.fixed, fixed, "fixed for {width:?}");
        }
    }
}
