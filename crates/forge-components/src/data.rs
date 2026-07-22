use gpui::{
    AnyElement, App, Context, Div, ElementId, InteractiveElement, IntoElement, ParentElement,
    Pixels, RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled,
    UniformListScrollHandle, Window, div, px, relative, uniform_list,
};

use crate::palette::ForgePalette;
use crate::tokens::{Density, FONT_XXS, Spacing, mono_family, spacing};

/// `Flex(n)` claims a share of leftover row width proportional to `n` (`Flex(8)` beside `Flex(7)` splits 8:7), ignoring cell content's intrinsic width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnWidth {
    Fixed(Pixels),
    Flex(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderAlign {
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
    pub fn align_end(mut self) -> Self {
        self.align = HeaderAlign::End;
        self
    }
}

pub struct DataRow {
    cells: Vec<AnyElement>,
    reveal_group: Option<SharedString>,
}

impl DataRow {
    pub fn new(cells: Vec<AnyElement>) -> Self {
        Self {
            cells,
            reveal_group: None,
        }
    }

    pub fn with_reveal_group(cells: Vec<AnyElement>, group: impl Into<SharedString>) -> Self {
        Self {
            cells,
            reveal_group: Some(group.into()),
        }
    }
}

/// Stays hidden while reserving its column width until the pointer enters a row tagged with the matching `group`, then becomes visible.
pub fn hover_reveal(content: impl IntoElement, group: impl Into<SharedString>) -> impl IntoElement {
    div()
        .invisible()
        .group_hover(group, |s| s.visible())
        .child(content)
}

struct DataTableColors {
    header_bg: Rgba,
    header_ink: Rgba,
    separator: Rgba,
    hover: Rgba,
}

#[derive(IntoElement)]
pub struct DataTable {
    columns: Vec<Column>,
    rows: Vec<DataRow>,
    colors: DataTableColors,
    density: Density,
    header_pad: Option<(Pixels, Pixels)>,
    row_pad: Option<(Pixels, Pixels)>,
    cell_gap: Pixels,
    trailing_rule: bool,
    scroll_id: Option<ElementId>,
}

pub fn data_table(palette: &ForgePalette, columns: Vec<Column>, rows: Vec<DataRow>) -> DataTable {
    DataTable {
        columns,
        rows,
        colors: DataTableColors {
            header_bg: palette.shell,
            header_ink: palette.text_faint,
            separator: palette.border_regular,
            hover: palette.base,
        },
        density: Density::default(),
        header_pad: None,
        row_pad: None,
        cell_gap: px(0.0),
        trailing_rule: true,
        scroll_id: None,
    }
}

impl DataTable {
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    pub fn header_bg(mut self, color: Rgba) -> Self {
        self.colors.header_bg = color;
        self
    }

    pub fn separator(mut self, color: Rgba) -> Self {
        self.colors.separator = color;
        self
    }

    pub fn row_hover(mut self, color: Rgba) -> Self {
        self.colors.hover = color;
        self
    }

    pub fn header_padding(mut self, vertical: Pixels, horizontal: Pixels) -> Self {
        self.header_pad = Some((vertical, horizontal));
        self
    }

    pub fn row_padding(mut self, vertical: Pixels, horizontal: Pixels) -> Self {
        self.row_pad = Some((vertical, horizontal));
        self
    }

    pub fn cell_gap(mut self, gap: Pixels) -> Self {
        self.cell_gap = gap;
        self
    }

    /// Drops the rule below the final row so an enclosing frame border owns the bottom edge instead of doubling it.
    pub fn trailing_rule(mut self, on: bool) -> Self {
        self.trailing_rule = on;
        self
    }

    /// Pins the header and scrolls the rows inside a `flex_1` viewport.
    pub fn scroll_body(mut self, id: impl Into<ElementId>) -> Self {
        self.scroll_id = Some(id.into());
        self
    }
}

#[allow(clippy::too_many_arguments)]
fn table_header(
    columns: Vec<Column>,
    header_bg: Rgba,
    header_ink: Rgba,
    gap: Pixels,
    h_py: Pixels,
    h_px: Pixels,
) -> Div {
    let header_cells = columns.into_iter().map(move |c| {
        let label_el = div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .whitespace_nowrap()
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
        .gap(gap)
        .py(h_py)
        .px(h_px)
        .bg(header_bg)
        .children(header_cells)
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
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let d = self.density;
        let colors = self.colors;
        let gap = self.cell_gap;
        let (h_py, h_px) = self
            .header_pad
            .unwrap_or((spacing(Spacing::Xs, d), spacing(Spacing::Md, d)));
        let (r_py, r_px) = self
            .row_pad
            .unwrap_or((spacing(Spacing::Xs, d), spacing(Spacing::Md, d)));

        let separator = colors.separator;
        let rule = move || div().flex_none().h(px(1.0)).w_full().bg(separator);

        let widths: Vec<ColumnWidth> = self.columns.iter().map(|c| c.width).collect();

        let header = table_header(
            self.columns,
            colors.header_bg,
            colors.header_ink,
            gap,
            h_py,
            h_px,
        );

        let hover = colors.hover;
        let total = self.rows.len();
        let mut body_children: Vec<AnyElement> = Vec::new();
        for (index, row) in self.rows.into_iter().enumerate() {
            let cells = row
                .cells
                .into_iter()
                .zip(widths.iter().copied())
                .map(|(cell, width)| column_cell(width, cell));

            let mut row_el = div()
                .flex()
                .items_center()
                .w_full()
                .gap(gap)
                .py(r_py)
                .px(r_px)
                .hover(move |s| s.bg(hover))
                .children(cells);

            if let Some(group) = row.reveal_group {
                row_el = row_el.group(group);
            }

            body_children.push(row_el.into_any_element());
            let last = index + 1 == total;
            if !last || self.trailing_rule {
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
                .overflow_y_scroll()
                .child(body_inner)
                .into_any_element(),
            None => body_inner.into_any_element(),
        };

        let mut root = div().flex().flex_col().w_full();
        if has_scroll {
            root = root.flex_1().min_h(px(0.0));
        }
        root.child(header).child(rule()).child(body)
    }
}

/// Rows MUST be single-line (nowrap + truncate): `uniform_list` measures one row and assumes the rest match; a wrapped row breaks scroll and clips text.
pub struct VirtualTable<'a> {
    id: ElementId,
    columns: Vec<Column>,
    row_count: usize,
    scroll: &'a UniformListScrollHandle,
    density: Density,
    header_bg: Rgba,
    header_ink: Rgba,
    separator: Rgba,
    hover: Rgba,
    header_pad: Option<(Pixels, Pixels)>,
    row_pad: Option<(Pixels, Pixels)>,
    cell_gap: Pixels,
    trailing_rule: bool,
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
        header_bg: palette.shell,
        header_ink: palette.text_faint,
        separator: palette.border_regular,
        hover: palette.base,
        header_pad: None,
        row_pad: None,
        cell_gap: px(0.0),
        trailing_rule: true,
    }
}

impl<'a> VirtualTable<'a> {
    #[must_use]
    pub fn header_bg(mut self, color: Rgba) -> Self {
        self.header_bg = color;
        self
    }

    #[must_use]
    pub fn separator(mut self, color: Rgba) -> Self {
        self.separator = color;
        self
    }

    #[must_use]
    pub fn row_hover(mut self, color: Rgba) -> Self {
        self.hover = color;
        self
    }

    #[must_use]
    pub fn header_padding(mut self, vertical: Pixels, horizontal: Pixels) -> Self {
        self.header_pad = Some((vertical, horizontal));
        self
    }

    #[must_use]
    pub fn row_padding(mut self, vertical: Pixels, horizontal: Pixels) -> Self {
        self.row_pad = Some((vertical, horizontal));
        self
    }

    #[must_use]
    pub fn cell_gap(mut self, gap: Pixels) -> Self {
        self.cell_gap = gap;
        self
    }

    /// Drops the rule below the final row so an enclosing frame border owns the bottom edge instead of doubling it.
    #[must_use]
    pub fn trailing_rule(mut self, on: bool) -> Self {
        self.trailing_rule = on;
        self
    }

    pub fn build<V, F>(self, row_builder: F, cx: &mut Context<V>) -> AnyElement
    where
        V: 'static,
        F: Fn(&mut V, usize, &mut Window, &mut Context<V>) -> DataRow + 'static,
    {
        let d = self.density;
        let gap = self.cell_gap;
        let (h_py, h_px) = self
            .header_pad
            .unwrap_or((spacing(Spacing::Xs, d), spacing(Spacing::Md, d)));
        let (r_py, r_px) = self
            .row_pad
            .unwrap_or((spacing(Spacing::Xs, d), spacing(Spacing::Md, d)));
        let separator = self.separator;
        let hover = self.hover;
        let row_count = self.row_count;
        let trailing_rule = self.trailing_rule;

        let widths: Vec<ColumnWidth> = self.columns.iter().map(|c| c.width).collect();
        let header = table_header(
            self.columns,
            self.header_bg,
            self.header_ink,
            gap,
            h_py,
            h_px,
        );

        let list = uniform_list(
            self.id,
            row_count,
            cx.processor(move |view, range: std::ops::Range<usize>, window, cx| {
                let mut out = Vec::with_capacity(range.len());
                for ix in range {
                    let last = ix + 1 == row_count;
                    let row = row_builder(view, ix, window, cx);
                    let cells = row
                        .cells
                        .into_iter()
                        .zip(widths.iter().copied())
                        .map(|(cell, width)| column_cell(width, cell));
                    let mut row_el = div()
                        .flex()
                        .items_center()
                        .w_full()
                        .gap(gap)
                        .py(r_py)
                        .px(r_px)
                        .hover(move |s| s.bg(hover))
                        .children(cells);
                    if trailing_rule || !last {
                        row_el = row_el.border_b(px(1.0)).border_color(separator);
                    }
                    if let Some(group) = row.reveal_group {
                        row_el = row_el.group(group);
                    }
                    out.push(row_el);
                }
                out
            }),
        )
        .track_scroll(self.scroll)
        .flex_1()
        .min_h(px(0.0));

        let rule = div().flex_none().h(px(1.0)).w_full().bg(separator);

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .child(header)
            .child(rule)
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
