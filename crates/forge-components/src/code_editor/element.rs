use std::ops::{Range, RangeInclusive};
use std::rc::Rc;

use gpui::{
    App, Bounds, Corners, Element, ElementId, ElementInputHandler, Entity, Font, GlobalElementId,
    Hsla, IntoElement, LayoutId, Pixels, Point, ShapedLine, SharedString, Style, TextAlign,
    TextRun, Window, fill, point, px, relative, size,
};

use super::CodeEditor;
use super::layout::{LineShape, TextMetrics, line_end_point, row_segments};
use crate::palette::{ForgePalette, with_alpha};
use crate::tokens::BORDER_THIN;

pub(super) const PAD_Y: Pixels = px(10.0);
const TEXT_PAD_X: Pixels = px(14.0);
const GUTTER_PAD_L: Pixels = px(14.0);
const GUTTER_PAD_R: Pixels = px(10.0);
const GUTTER_ACCENT_W: Pixels = px(2.0);
const GUTTER_MARK: &str = "\u{25cf}";
const MIN_GUTTER_DIGITS: usize = 2;
const CARET_W: Pixels = px(1.5);
const UNDERLINE_H: Pixels = px(1.0);
const OVERSCAN_LINES: usize = 4;
const MAX_LAYOUT_PASSES: usize = 2;
const FALLBACK_ADVANCE_RATIO: f32 = 0.6;
const CURRENT_LINE_ALPHA: f32 = 0.06;
const SELECTION_ALPHA: f32 = 0.25;

struct PaintLine {
    origin: Point<Pixels>,
    shape: Rc<LineShape>,
}

struct PaintLabel {
    origin: Point<Pixels>,
    label: ShapedLine,
}

pub(super) struct Frame {
    palette: ForgePalette,
    line_height: Pixels,
    gutter: Bounds<Pixels>,
    gutter_corner_radius: Pixels,
    band: Option<Bounds<Pixels>>,
    selection: Vec<Bounds<Pixels>>,
    underlines: Vec<Bounds<Pixels>>,
    lines: Vec<PaintLine>,
    marks: Vec<Bounds<Pixels>>,
    labels: Vec<PaintLabel>,
    placeholder: Option<PaintLabel>,
    cursor: Option<Bounds<Pixels>>,
}

struct Viewport {
    bounds: Bounds<Pixels>,
    text_x: Pixels,
    origin_y: Pixels,
    line_height: Pixels,
    advance: Pixels,
    font: Font,
    font_size: Pixels,
}

impl Viewport {
    fn row_bounds(&self, top: Pixels, x: Pixels, width: Pixels) -> Bounds<Pixels> {
        Bounds::new(
            point(self.text_x + x, self.origin_y + top),
            size(width, self.line_height),
        )
    }
}

impl CodeEditor {
    fn layout_frame(
        &mut self,
        bounds: Bounds<Pixels>,
        focused: bool,
        window: &mut Window,
    ) -> Frame {
        let style = window.text_style();
        let font = style.font();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&font);
        let advance = text_system
            .ch_advance(font_id, font_size)
            .unwrap_or(font_size * FALLBACK_ADVANCE_RATIO);
        let digits = self.lines.len().to_string().len().max(MIN_GUTTER_DIGITS);
        let gutter_width = GUTTER_PAD_L + advance * digits + GUTTER_PAD_R;
        let text_left = gutter_width + TEXT_PAD_X;
        let wrap_width = (bounds.size.width - text_left - TEXT_PAD_X).max(advance);
        self.geometry.configure(
            TextMetrics {
                font: font.clone(),
                font_size,
                line_height,
                advance,
                wrap_width,
            },
            &mut self.lines,
        );
        self.text_left = text_left;
        self.last_bounds = Some(bounds);

        let cursor = self.buffer.cursor();
        let caret_line = self.lines.line_index_at(cursor);
        self.geometry
            .shape_line(&mut self.lines, caret_line, &self.palette, window);
        self.geometry.refresh(&self.lines);

        let view_height = bounds.size.height;
        let mut scroll = self.scroll_offset.clamp(px(0.0), self.max_scroll());
        if self.follow_caret {
            scroll = self.scroll_to_caret(scroll, cursor, view_height);
        }
        for _ in 0..MAX_LAYOUT_PASSES {
            let anchor = self.geometry.line_at_y(scroll - PAD_Y);
            let anchor_shift = scroll - self.geometry.top(anchor);
            let mut changed = false;
            for index in self.visible_lines(scroll, view_height, OVERSCAN_LINES) {
                changed |= self
                    .geometry
                    .shape_line(&mut self.lines, index, &self.palette, window);
            }
            if !changed {
                break;
            }
            self.geometry.refresh(&self.lines);
            scroll = if self.follow_caret {
                self.scroll_to_caret(scroll, cursor, view_height)
            } else {
                self.geometry.top(anchor) + anchor_shift
            };
            scroll = scroll.clamp(px(0.0), self.max_scroll());
        }
        self.scroll_offset = scroll;

        let painted = self.visible_lines(scroll, view_height, 0);
        for index in painted.clone() {
            self.geometry
                .shape_line(&mut self.lines, index, &self.palette, window);
        }
        self.geometry.refresh(&self.lines);

        let viewport = Viewport {
            bounds,
            text_x: bounds.left() + text_left,
            origin_y: bounds.top() + PAD_Y - scroll,
            line_height,
            advance,
            font,
            font_size,
        };
        let band = focused.then(|| {
            Bounds::from_corners(
                point(
                    bounds.left(),
                    viewport.origin_y + self.geometry.top(caret_line),
                ),
                point(
                    bounds.right(),
                    viewport.origin_y + self.geometry.bottom(caret_line),
                ),
            )
        });
        let cursor_bounds = self
            .geometry
            .point_for_offset(&self.lines, cursor)
            .map(|caret| viewport.row_bounds(caret.y, caret.x, CARET_W));

        Frame {
            palette: self.palette,
            line_height,
            gutter: Bounds::new(bounds.origin, size(gutter_width, view_height)),
            gutter_corner_radius: self.gutter_corner_radius(),
            band,
            selection: self.range_rows(&painted, self.buffer.selected_range(), &viewport, true),
            underlines: self
                .buffer
                .marked_range()
                .map(|marked| {
                    self.range_rows(&painted, marked, &viewport, false)
                        .into_iter()
                        .map(|row| {
                            Bounds::new(
                                point(row.left(), row.bottom() - UNDERLINE_H),
                                size(row.size.width, UNDERLINE_H),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            lines: painted
                .clone()
                .filter_map(|index| {
                    let shape = self.geometry.shape_of(&self.lines, index)?;
                    Some(PaintLine {
                        origin: point(
                            viewport.text_x,
                            viewport.origin_y + self.geometry.top(index),
                        ),
                        shape,
                    })
                })
                .collect(),
            marks: painted
                .clone()
                .filter(|index| self.gutter_marks.contains(index))
                .map(|index| {
                    Bounds::new(
                        point(bounds.left(), viewport.origin_y + self.geometry.top(index)),
                        size(GUTTER_ACCENT_W, line_height),
                    )
                })
                .collect(),
            labels: self.gutter_labels(
                painted,
                focused.then_some(caret_line),
                gutter_width,
                &viewport,
                window,
            ),
            placeholder: self.placeholder_label(&viewport, window),
            cursor: cursor_bounds,
        }
    }

    fn scroll_to_caret(&self, scroll: Pixels, cursor: usize, view_height: Pixels) -> Pixels {
        let (Some(caret), Some(metrics)) = (
            self.geometry.point_for_offset(&self.lines, cursor),
            self.geometry.metrics(),
        ) else {
            return scroll;
        };
        let reveal_bottom = caret.y + metrics.line_height + PAD_Y + PAD_Y;
        if caret.y < scroll {
            caret.y
        } else if reveal_bottom > scroll + view_height {
            reveal_bottom - view_height
        } else {
            scroll
        }
    }

    fn visible_lines(
        &self,
        scroll: Pixels,
        view_height: Pixels,
        overscan: usize,
    ) -> RangeInclusive<usize> {
        let first = self.geometry.line_at_y(scroll - PAD_Y);
        let last = self.geometry.line_at_y(scroll - PAD_Y + view_height);
        let last_line = self.lines.len().saturating_sub(1);
        first.saturating_sub(overscan)..=(last + overscan).min(last_line)
    }

    fn range_rows(
        &self,
        painted: &RangeInclusive<usize>,
        range: &Range<usize>,
        viewport: &Viewport,
        cover_newline: bool,
    ) -> Vec<Bounds<Pixels>> {
        if range.is_empty() {
            return Vec::new();
        }
        let mut rows = Vec::new();
        for index in painted.clone() {
            let Some(line) = self.lines.line(index) else {
                continue;
            };
            if range.end < line.start() || range.start > line.end() {
                continue;
            }
            let Some(shape) = self.geometry.shape_of(&self.lines, index) else {
                continue;
            };
            let top = self.geometry.top(index);
            let local = range.start.saturating_sub(line.start())
                ..(range.end - line.start()).min(line.text().len());
            for segment in row_segments(&shape, local, viewport.line_height) {
                rows.push(viewport.row_bounds(
                    top + segment.y,
                    segment.x.start,
                    segment.x.end - segment.x.start,
                ));
            }
            if cover_newline && range.end > line.end() {
                let end = line_end_point(&shape, viewport.line_height);
                rows.push(viewport.row_bounds(top + end.y, end.x, viewport.advance));
            }
        }
        rows
    }

    fn gutter_labels(
        &self,
        painted: RangeInclusive<usize>,
        current_line: Option<usize>,
        gutter_width: Pixels,
        viewport: &Viewport,
        window: &mut Window,
    ) -> Vec<PaintLabel> {
        painted
            .map(|index| {
                let marked = self.gutter_marks.contains(&index);
                let (text, color): (SharedString, Hsla) = if marked {
                    (GUTTER_MARK.into(), self.palette.warning.into())
                } else if current_line == Some(index) {
                    (
                        (index + 1).to_string().into(),
                        self.palette.text_secondary.into(),
                    )
                } else {
                    (
                        (index + 1).to_string().into(),
                        self.palette.text_extreme_faint.into(),
                    )
                };
                let label = shape_label(text, color, viewport, window);
                PaintLabel {
                    origin: point(
                        viewport.bounds.left() + gutter_width - GUTTER_PAD_R - label.width(),
                        viewport.origin_y + self.geometry.top(index),
                    ),
                    label,
                }
            })
            .collect()
    }

    fn placeholder_label(&self, viewport: &Viewport, window: &mut Window) -> Option<PaintLabel> {
        if !self.buffer.as_str().is_empty() || self.placeholder.is_empty() {
            return None;
        }
        Some(PaintLabel {
            origin: point(viewport.text_x, viewport.origin_y),
            label: shape_label(
                self.placeholder.clone(),
                self.palette.text_muted.into(),
                viewport,
                window,
            ),
        })
    }
}

fn shape_label(
    text: SharedString,
    color: Hsla,
    viewport: &Viewport,
    window: &mut Window,
) -> ShapedLine {
    let run = TextRun {
        len: text.len(),
        font: viewport.font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(text, viewport.font_size, std::slice::from_ref(&run), None)
}

pub(super) struct CodeEditorElement {
    editor: Entity<CodeEditor>,
}

impl CodeEditorElement {
    pub(super) fn new(editor: Entity<CodeEditor>) -> Self {
        Self { editor }
    }
}

impl IntoElement for CodeEditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for CodeEditorElement {
    type RequestLayoutState = ();
    type PrepaintState = Frame;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let focused = self.editor.read(cx).focus_handle.is_focused(window);
        self.editor
            .update(cx, |editor, _| editor.layout_frame(bounds, focused, window))
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        frame: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (focus_handle, caret_lit) = {
            let editor = self.editor.read(cx);
            (editor.focus_handle.clone(), editor.caret.is_lit())
        };
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );

        let palette = frame.palette;
        let line_height = frame.line_height;
        window.paint_quad(fill(frame.gutter, palette.elevated).corner_radii(Corners {
            top_left: frame.gutter_corner_radius,
            top_right: px(0.0),
            bottom_right: px(0.0),
            bottom_left: frame.gutter_corner_radius,
        }));
        window.paint_quad(fill(
            Bounds::new(
                point(frame.gutter.right() - BORDER_THIN, frame.gutter.top()),
                size(BORDER_THIN, frame.gutter.size.height),
            ),
            palette.border_regular,
        ));
        if let Some(band) = frame.band {
            window.paint_quad(fill(band, with_alpha(palette.brand, CURRENT_LINE_ALPHA)));
        }
        for row in &frame.selection {
            window.paint_quad(fill(*row, with_alpha(palette.brand, SELECTION_ALPHA)));
        }
        for line in &frame.lines {
            let _ =
                line.shape
                    .line
                    .paint(line.origin, line_height, TextAlign::Left, None, window, cx);
        }
        for underline in &frame.underlines {
            window.paint_quad(fill(*underline, palette.text_primary));
        }
        for mark in &frame.marks {
            window.paint_quad(fill(*mark, palette.warning));
        }
        for label in frame.labels.iter().chain(frame.placeholder.as_ref()) {
            let _ = label
                .label
                .paint(label.origin, line_height, TextAlign::Left, None, window, cx);
        }
        if focus_handle.is_focused(window)
            && caret_lit
            && let Some(cursor) = frame.cursor
        {
            window.paint_quad(fill(cursor, palette.text_primary));
        }
    }
}
