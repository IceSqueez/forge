use std::ops::{Range, RangeInclusive};
use std::rc::Rc;

use gpui::{
    App, Bounds, Corners, Element, ElementId, ElementInputHandler, Entity, Font, FontStyle,
    GlobalElementId, Hsla, IntoElement, LayoutId, Pixels, Point, ShapedLine, SharedString, Style,
    TextAlign, TextRun, Window, fill, point, px, relative, size,
};

use super::CodeEditor;
use super::layout::{LineShape, TextMetrics, line_end_point, row_segments};
use crate::palette::{ForgePalette, with_alpha};
use crate::text_drag::paint_drag_tracking;
use crate::text_editing::{CARET_WIDTH, COMPOSITION_UNDERLINE, SELECTION_ALPHA};
use crate::tokens::{BORDER_THIN, Spacing, spacing};

pub(super) const PAD_Y: Pixels = px(10.0);
const TEXT_PAD_X: Pixels = px(14.0);
const GUTTER_PAD_L: Pixels = px(14.0);
const GUTTER_PAD_R: Pixels = px(10.0);
const GUTTER_ACCENT_W: Pixels = px(2.0);
const GUTTER_MARK: &str = "\u{25cf}";
const MIN_GUTTER_DIGITS: usize = 2;
const OVERSCAN_LINES: usize = 4;
const MAX_LAYOUT_PASSES: usize = 2;
const FALLBACK_ADVANCE_RATIO: f32 = 0.6;
const CURRENT_LINE_ALPHA: f32 = 0.06;

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
    gutter: Option<Bounds<Pixels>>,
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

struct Measured {
    font: Font,
    font_size: Pixels,
    line_height: Pixels,
    advance: Pixels,
    gutter_width: Pixels,
    text_left: Pixels,
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
        let measured = self.measure(bounds, window);
        let caret_line = self.lines.line_index_at(self.buffer.cursor());
        let scroll = self.settle_scroll(bounds.size.height, caret_line, window);
        self.build_frame(bounds, measured, scroll, caret_line, focused, window)
    }

    fn measure(&mut self, bounds: Bounds<Pixels>, window: &mut Window) -> Measured {
        let style = window.text_style();
        let font = style.font();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&font);
        let advance = text_system
            .ch_advance(font_id, font_size)
            .unwrap_or(font_size * FALLBACK_ADVANCE_RATIO);
        let (gutter_width, text_pad_x) = if self.compact {
            (px(0.0), spacing(Spacing::Sm, self.density))
        } else {
            let digits = self.lines.len().to_string().len().max(MIN_GUTTER_DIGITS);
            (GUTTER_PAD_L + advance * digits + GUTTER_PAD_R, TEXT_PAD_X)
        };
        let text_left = gutter_width + text_pad_x;
        let wrap_width = (bounds.size.width - text_left - text_pad_x).max(advance);
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
        Measured {
            font,
            font_size,
            line_height,
            advance,
            gutter_width,
            text_left,
        }
    }

    fn settle_scroll(
        &mut self,
        view_height: Pixels,
        caret_line: usize,
        window: &mut Window,
    ) -> Pixels {
        let pad_y = self.pad_y();
        let cursor = self.buffer.cursor();
        self.geometry
            .shape_line(&mut self.lines, caret_line, &self.palette, window);
        self.geometry.refresh(&self.lines);

        let mut scroll = self.scroll_offset.clamp(px(0.0), self.max_scroll());
        if self.follow_caret {
            scroll = self.scroll_to_caret(scroll, cursor, view_height);
        }
        for _ in 0..MAX_LAYOUT_PASSES {
            let anchor = self.geometry.line_at_y(scroll - pad_y);
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
        scroll
    }

    fn build_frame(
        &mut self,
        bounds: Bounds<Pixels>,
        measured: Measured,
        scroll: Pixels,
        caret_line: usize,
        focused: bool,
        window: &mut Window,
    ) -> Frame {
        let view_height = bounds.size.height;
        let painted = self.visible_lines(scroll, view_height, 0);
        for index in painted.clone() {
            self.geometry
                .shape_line(&mut self.lines, index, &self.palette, window);
        }
        self.geometry.refresh(&self.lines);

        let Measured {
            font,
            font_size,
            line_height,
            advance,
            gutter_width,
            text_left,
        } = measured;
        let viewport = Viewport {
            bounds,
            text_x: bounds.left() + text_left,
            origin_y: bounds.top() + self.pad_y() - scroll,
            line_height,
            advance,
            font,
            font_size,
        };
        Frame {
            palette: self.palette,
            line_height,
            gutter: (!self.compact)
                .then(|| Bounds::new(bounds.origin, size(gutter_width, view_height))),
            gutter_corner_radius: self.gutter_corner_radius(),
            band: (focused && !self.compact).then(|| self.caret_band(caret_line, &viewport)),
            selection: self.range_rows(&painted, self.buffer.selected_range(), &viewport, true),
            underlines: self.composition_underlines(&painted, &viewport),
            lines: self.paint_lines(&painted, &viewport),
            marks: self.gutter_mark_bars(&painted, &viewport),
            labels: if self.compact {
                Vec::new()
            } else {
                self.gutter_labels(
                    painted,
                    focused.then_some(caret_line),
                    gutter_width,
                    &viewport,
                    window,
                )
            },
            placeholder: self.placeholder_label(&viewport, window),
            cursor: self
                .geometry
                .point_for_offset(&self.lines, self.buffer.cursor())
                .map(|caret| viewport.row_bounds(caret.y, caret.x, CARET_WIDTH)),
        }
    }

    fn caret_band(&self, caret_line: usize, viewport: &Viewport) -> Bounds<Pixels> {
        let bounds = viewport.bounds;
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
    }

    fn composition_underlines(
        &self,
        painted: &RangeInclusive<usize>,
        viewport: &Viewport,
    ) -> Vec<Bounds<Pixels>> {
        self.buffer
            .marked_range()
            .map(|marked| {
                self.range_rows(painted, marked, viewport, false)
                    .into_iter()
                    .map(|row| {
                        Bounds::new(
                            point(row.left(), row.bottom() - COMPOSITION_UNDERLINE),
                            size(row.size.width, COMPOSITION_UNDERLINE),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn paint_lines(&self, painted: &RangeInclusive<usize>, viewport: &Viewport) -> Vec<PaintLine> {
        painted
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
            .collect()
    }

    fn gutter_mark_bars(
        &self,
        painted: &RangeInclusive<usize>,
        viewport: &Viewport,
    ) -> Vec<Bounds<Pixels>> {
        painted
            .clone()
            .filter(|index| self.gutter_marks.contains(index))
            .map(|index| {
                Bounds::new(
                    point(
                        viewport.bounds.left(),
                        viewport.origin_y + self.geometry.top(index),
                    ),
                    size(GUTTER_ACCENT_W, viewport.line_height),
                )
            })
            .collect()
    }

    fn scroll_to_caret(&self, scroll: Pixels, cursor: usize, view_height: Pixels) -> Pixels {
        let (Some(caret), Some(metrics)) = (
            self.geometry.point_for_offset(&self.lines, cursor),
            self.geometry.metrics(),
        ) else {
            return scroll;
        };
        let pad_y = self.pad_y();
        let reveal_bottom = caret.y + metrics.line_height + pad_y + pad_y;
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
        let pad_y = self.pad_y();
        let first = self.geometry.line_at_y(scroll - pad_y);
        let last = self.geometry.line_at_y(scroll - pad_y + view_height);
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
                let label = shape_label(text, color, FontStyle::Normal, viewport, window);
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
                self.palette.text_faint.into(),
                FontStyle::Italic,
                viewport,
                window,
            ),
        })
    }
}

fn shape_label(
    text: SharedString,
    color: Hsla,
    style: FontStyle,
    viewport: &Viewport,
    window: &mut Window,
) -> ShapedLine {
    let mut font = viewport.font.clone();
    font.style = style;
    let run = TextRun {
        len: text.len(),
        font,
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
        self.editor.update(cx, |editor, cx| {
            let frame = editor.layout_frame(bounds, focused, window);
            if editor.rows_outgrew_render() {
                cx.notify();
            }
            frame
        })
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
        if self.editor.read(cx).motion.drag_selecting {
            paint_drag_tracking(&self.editor, window);
        }

        let palette = frame.palette;
        let line_height = frame.line_height;
        if let Some(gutter) = frame.gutter {
            window.paint_quad(fill(gutter, palette.elevated).corner_radii(Corners {
                top_left: frame.gutter_corner_radius,
                top_right: px(0.0),
                bottom_right: px(0.0),
                bottom_left: frame.gutter_corner_radius,
            }));
            window.paint_quad(fill(
                Bounds::new(
                    point(gutter.right() - BORDER_THIN, gutter.top()),
                    size(BORDER_THIN, gutter.size.height),
                ),
                palette.border_regular,
            ));
        }
        if let Some(band) = frame.band {
            window.paint_quad(fill(band, with_alpha(palette.brand, CURRENT_LINE_ALPHA)));
        }
        for row in &frame.selection {
            window.paint_quad(fill(*row, with_alpha(palette.brand, SELECTION_ALPHA)));
        }
        for line in &frame.lines {
            if let Err(error) =
                line.shape
                    .line
                    .paint(line.origin, line_height, TextAlign::Left, None, window, cx)
            {
                tracing::debug!(%error, "code editor line could not be painted");
            }
        }
        for underline in &frame.underlines {
            window.paint_quad(fill(*underline, palette.text_primary));
        }
        for mark in &frame.marks {
            window.paint_quad(fill(*mark, palette.warning));
        }
        for label in frame.labels.iter().chain(frame.placeholder.as_ref()) {
            if let Err(error) =
                label
                    .label
                    .paint(label.origin, line_height, TextAlign::Left, None, window, cx)
            {
                tracing::debug!(%error, "code editor label could not be painted");
            }
        }
        if focus_handle.is_focused(window)
            && caret_lit
            && let Some(cursor) = frame.cursor
        {
            window.paint_quad(fill(cursor, palette.text_primary));
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext, font};

    use super::*;
    use crate::code_editor::layout::TextMetrics;
    use crate::highlight::Language;
    use crate::palette::FORGE_DEFAULT;
    use crate::test_support::install_presentation;

    const LINE_HEIGHT: Pixels = px(20.0);
    const VIEW_HEIGHT: Pixels = px(100.0);
    const OVERSCAN: usize = 2;

    fn offset_of_line(editor: &CodeEditor, index: usize) -> usize {
        editor.lines.line(index).unwrap().start()
    }

    fn laid_out<'a>(
        cx: &'a mut TestAppContext,
        text: &str,
    ) -> (Entity<CodeEditor>, &'a mut VisualTestContext) {
        install_presentation(cx);
        let (editor, vcx) = cx.add_window_view(|_window, cx| {
            CodeEditor::new(Language::Rhai, "", cx).with_line_height(LINE_HEIGHT)
        });
        let text = text.to_owned();
        vcx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_content(text, cx);
                let metrics = TextMetrics {
                    font: font("Mono"),
                    font_size: px(12.0),
                    line_height: LINE_HEIGHT,
                    advance: px(10.0),
                    wrap_width: px(100_000.0),
                };
                editor.geometry.configure(metrics, &mut editor.lines);
                editor.geometry.refresh(&editor.lines);
                for index in 0..editor.lines.len() {
                    editor
                        .geometry
                        .shape_line(&mut editor.lines, index, &FORGE_DEFAULT, window);
                }
                editor.geometry.refresh(&editor.lines);
            });
        });
        (editor, vcx)
    }

    fn numbered_lines(count: usize) -> String {
        vec!["x"; count].join("\n")
    }

    #[gpui::test]
    fn the_scroll_moves_only_as_far_as_the_caret_row_needs(cx: &mut TestAppContext) {
        let (editor, vcx) = laid_out(cx, &numbered_lines(20));

        let scrolled: Vec<(f32, f32)> = vcx.update(|_window, cx| {
            let editor = editor.read(cx);
            [
                (0, px(0.0)),
                (3, px(0.0)),
                (4, px(0.0)),
                (10, px(0.0)),
                (2, px(100.0)),
                (5, px(100.0)),
            ]
            .map(|(line, scroll)| {
                let cursor = offset_of_line(editor, line);
                let after = editor.scroll_to_caret(scroll, cursor, VIEW_HEIGHT);
                (f32::from(scroll), f32::from(after))
            })
            .to_vec()
        });

        assert_eq!(
            scrolled,
            [
                (0.0, 0.0),
                (0.0, 0.0),
                (0.0, 20.0),
                (0.0, 140.0),
                (100.0, 40.0),
                (100.0, 100.0),
            ]
        );
    }

    #[gpui::test]
    fn a_caret_on_an_unmeasured_document_leaves_the_scroll_alone(cx: &mut TestAppContext) {
        install_presentation(cx);
        let editor = cx.new(|cx| CodeEditor::new(Language::Rhai, "", cx));

        let after = editor.read_with(cx, |editor, _cx| {
            editor.scroll_to_caret(px(37.0), 0, VIEW_HEIGHT)
        });

        assert_eq!(after, px(37.0));
    }

    #[gpui::test]
    fn the_painted_lines_cover_the_view_plus_overscan_clamped_to_the_document(
        cx: &mut TestAppContext,
    ) {
        let (editor, vcx) = laid_out(cx, &numbered_lines(100));

        let cases = vcx.update(|_window, cx| {
            let editor = editor.read(cx);
            [px(0.0), px(1000.0), px(1990.0), px(5000.0)]
                .map(|scroll| editor.visible_lines(scroll, VIEW_HEIGHT, OVERSCAN))
        });

        assert_eq!(cases, [0..=6, 47..=56, 97..=99, 97..=99]);
    }

    fn rows_of(
        cx: &mut VisualTestContext,
        editor: &Entity<CodeEditor>,
        range: Range<usize>,
        cover_newline: bool,
    ) -> Vec<f32> {
        cx.update(|_window, cx| {
            let editor = editor.read(cx);
            let viewport = Viewport {
                bounds: Bounds::default(),
                text_x: px(0.0),
                origin_y: px(0.0),
                line_height: LINE_HEIGHT,
                advance: px(10.0),
                font: font("Mono"),
                font_size: px(12.0),
            };
            let painted = 0..=editor.lines.len() - 1;
            editor
                .range_rows(&painted, &range, &viewport, cover_newline)
                .into_iter()
                .map(|row| f32::from(row.origin.y))
                .collect()
        })
    }

    #[gpui::test]
    fn a_selection_paints_one_row_per_line_it_touches(cx: &mut TestAppContext) {
        let (editor, vcx) = laid_out(cx, "aa\nbb\ncc");

        assert_eq!(rows_of(vcx, &editor, 1..4, false), [0.0, 20.0]);
    }

    #[gpui::test]
    fn a_selection_across_a_newline_marks_it_only_when_asked_to(cx: &mut TestAppContext) {
        let (editor, vcx) = laid_out(cx, "aa\nbb\ncc");

        assert_eq!(rows_of(vcx, &editor, 1..4, true), [0.0, 0.0, 20.0]);
    }

    #[gpui::test]
    fn an_empty_range_paints_nothing_and_a_multibyte_line_is_cut_on_its_end(
        cx: &mut TestAppContext,
    ) {
        let (editor, vcx) = laid_out(cx, "\u{439}\u{439}\nb");

        assert!(rows_of(vcx, &editor, 2..2, true).is_empty());
        assert_eq!(rows_of(vcx, &editor, 0..4, false), [0.0]);
    }
}
