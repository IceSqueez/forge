use gpui::{
    AnyElement, App, Div, InteractiveElement, IntoElement, ParentElement, Pixels, RenderOnce, Rgba,
    SharedString, Styled, Window, div, px, relative,
};

use crate::palette::ForgePalette;
use crate::tokens::{DEFAULT_MONO_FAMILY, Density, FONT_XS, Spacing, spacing};

/// A column's horizontal sizing. `Fixed` pins an exact pixel width that never
/// grows or shrinks; `Flex(n)` claims a share of the leftover row width in
/// proportion to `n` (so `Flex(8)` beside `Flex(7)` splits 8:7), ignoring its
/// content's intrinsic width — the equivalent of a fill-portion column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnWidth {
    Fixed(Pixels),
    Flex(f32),
}

pub(crate) struct FlexSpec {
    pub grow: f32,
    pub shrink: f32,
    pub fixed: Option<Pixels>,
}

/// Resolves a column's sizing into flexbox terms: a `Fixed` column is inflexible
/// at its pixel width (grow 0, shrink 0), while a `Flex(n)` column grows at rate
/// `n`, may shrink, and takes a zero flex-basis so leftover space is shared
/// purely by the grow ratio rather than biased by cell content.
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

/// One table row: the already-built cells (one per column, zipped positionally
/// against the table's widths) plus an optional hover group. When a group name is
/// set, the row is tagged so companion [`hover_reveal`] cells built with the same
/// name fade in only while the pointer is over the row.
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

/// A cell that stays hidden (but keeps reserving its column width) until the
/// pointer enters the row carrying the matching `group` name, at which point it
/// becomes visible — the row-hover reveal used for per-row action controls.
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
    headers: Vec<SharedString>,
    widths: Vec<ColumnWidth>,
    rows: Vec<DataRow>,
    colors: DataTableColors,
    density: Density,
}

/// Start a data table: a header strip over separator-ruled rows. Header labels
/// render monospace at `FONT_XS` in the faint ink over the `shell` fill; each row
/// paints a `base` tint on hover; a 1px `border_regular` rule sits under the
/// header and under every row. Padding resolves at `Density::Cozy` by default.
/// Fills and inks resolve from `palette` up front so the built value holds no
/// borrow.
pub fn data_table(
    palette: &ForgePalette,
    headers: Vec<SharedString>,
    widths: Vec<ColumnWidth>,
    rows: Vec<DataRow>,
) -> DataTable {
    DataTable {
        headers,
        widths,
        rows,
        colors: DataTableColors {
            header_bg: palette.shell,
            header_ink: palette.text_faint,
            separator: palette.border_regular,
            hover: palette.base,
        },
        density: Density::default(),
    }
}

impl DataTable {
    /// Overrides the density used to scale the header and row inset. A bare
    /// [`data_table`] resolves it at `Density::Cozy`.
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
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
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let d = self.density;
        let colors = self.colors;
        let widths = self.widths;
        let separator = colors.separator;
        let rule = move || div().flex_none().h(px(1.0)).w_full().bg(separator);

        let header_ink = colors.header_ink;
        let header_cells =
            self.headers
                .into_iter()
                .zip(widths.iter().copied())
                .map(move |(label, width)| {
                    let label_el = div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(header_ink)
                        .child(label);
                    column_cell(width, label_el)
                });

        let header = div()
            .flex()
            .items_center()
            .w_full()
            .py(spacing(Spacing::Xs, d))
            .px(spacing(Spacing::Md, d))
            .bg(colors.header_bg)
            .children(header_cells);

        let mut root = div().flex().flex_col().w_full().child(header).child(rule());

        let hover = colors.hover;
        for row in self.rows {
            let cells = row
                .cells
                .into_iter()
                .zip(widths.iter().copied())
                .map(|(cell, width)| column_cell(width, cell));

            let mut row_el = div()
                .flex()
                .items_center()
                .w_full()
                .py(spacing(Spacing::Xs, d))
                .px(spacing(Spacing::Md, d))
                .hover(move |s| s.bg(hover))
                .children(cells);

            if let Some(group) = row.reveal_group {
                row_el = row_el.group(group);
            }

            root = root.child(row_el).child(rule());
        }

        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_flex_maps_each_width_to_its_flexbox_spec() {
        // Why: the Fixed -> (grow 0, shrink 0, fixed=Some) vs
        // Flex(n) -> (grow n, shrink 1, basis 0/fixed=None) split is the
        // load-bearing layout decision. A Fixed column that could shrink, or a
        // Flex column carrying a pixel basis, silently breaks row sizing.
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
