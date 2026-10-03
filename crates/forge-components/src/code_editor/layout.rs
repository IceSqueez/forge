use std::iter;
use std::ops::Range;
use std::rc::Rc;

use gpui::{Font, Pixels, Point, SharedString, TextRun, Window, WrappedLine, point, px};

use super::lines::{CodeLine, CodeLines};
use crate::palette::ForgePalette;
use crate::syntax_color::token_color;

pub(super) struct LineShape {
    wrap_width: Pixels,
    pub(super) line: WrappedLine,
}

pub(super) type Lines = CodeLines<Option<Rc<LineShape>>>;
type Line = CodeLine<Option<Rc<LineShape>>>;

#[derive(Clone, PartialEq)]
pub(super) struct TextMetrics {
    pub(super) font: Font,
    pub(super) font_size: Pixels,
    pub(super) line_height: Pixels,
    pub(super) advance: Pixels,
    pub(super) wrap_width: Pixels,
}

pub(super) struct RowSegment {
    pub(super) y: Pixels,
    pub(super) x: Range<Pixels>,
}

#[derive(Default)]
pub(super) struct Geometry {
    metrics: Option<TextMetrics>,
    tops: Vec<Pixels>,
    stale: bool,
}

impl Geometry {
    pub(super) fn metrics(&self) -> Option<&TextMetrics> {
        self.metrics.as_ref()
    }

    pub(super) fn configure(&mut self, metrics: TextMetrics, lines: &mut Lines) {
        if self.metrics.as_ref() == Some(&metrics) {
            return;
        }
        let font_changed = self
            .metrics
            .as_ref()
            .is_some_and(|held| held.font != metrics.font || held.font_size != metrics.font_size);
        if font_changed {
            lines.reset_payloads();
        }
        self.metrics = Some(metrics);
        self.stale = true;
    }

    pub(super) fn invalidate(&mut self) {
        self.stale = true;
    }

    pub(super) fn refresh(&mut self, lines: &Lines) {
        let Some(metrics) = self.metrics.as_ref() else {
            return;
        };
        if !self.stale && self.tops.len() == lines.len() + 1 {
            return;
        }
        self.tops.clear();
        let mut y = px(0.0);
        self.tops.push(y);
        for line in lines.lines() {
            y += metrics.line_height * rows(line, metrics);
            self.tops.push(y);
        }
        self.stale = false;
    }

    pub(super) fn top(&self, index: usize) -> Pixels {
        self.tops.get(index).copied().unwrap_or_default()
    }

    pub(super) fn bottom(&self, index: usize) -> Pixels {
        self.top(index + 1)
    }

    pub(super) fn total(&self) -> Pixels {
        self.tops.last().copied().unwrap_or_default()
    }

    pub(super) fn line_at_y(&self, y: Pixels) -> usize {
        let last_line = self.tops.len().saturating_sub(2);
        self.tops
            .partition_point(|top| *top <= y)
            .saturating_sub(1)
            .min(last_line)
    }

    pub(super) fn shape_line(
        &mut self,
        lines: &mut Lines,
        index: usize,
        palette: &ForgePalette,
        window: &mut Window,
    ) -> bool {
        let language = lines.language();
        let Some(metrics) = self.metrics.as_ref() else {
            return false;
        };
        let Some(line) = lines.line_mut(index) else {
            return false;
        };
        if current_shape(line, metrics).is_some() {
            return false;
        }
        let rows_before = rows(line, metrics);
        let runs: Vec<TextRun> = line
            .spans()
            .iter()
            .map(|span| TextRun {
                len: span.len,
                font: metrics.font.clone(),
                color: token_color(language, span.class, palette),
                background_color: None,
                underline: None,
                strikethrough: None,
            })
            .collect();
        let shaped = window
            .text_system()
            .shape_text(
                SharedString::from(line.text()),
                metrics.font_size,
                &runs,
                Some(metrics.wrap_width),
                None,
            )
            .ok()
            .and_then(|shaped| shaped.into_iter().next());
        let Some(shaped) = shaped else {
            return false;
        };
        line.payload = Some(Rc::new(LineShape {
            wrap_width: metrics.wrap_width,
            line: shaped,
        }));
        let changed = rows(line, metrics) != rows_before;
        if changed {
            self.stale = true;
        }
        changed
    }

    pub(super) fn shape_of(&self, lines: &Lines, index: usize) -> Option<Rc<LineShape>> {
        let metrics = self.metrics.as_ref()?;
        current_shape(lines.line(index)?, metrics).cloned()
    }

    pub(super) fn point_for_offset(&self, lines: &Lines, offset: usize) -> Option<Point<Pixels>> {
        let metrics = self.metrics.as_ref()?;
        let index = lines.line_index_at(offset);
        let line = lines.line(index)?;
        let shape = current_shape(line, metrics)?;
        let local = offset.saturating_sub(line.start()).min(line.text().len());
        let inner = shape.line.position_for_index(local, metrics.line_height)?;
        Some(point(inner.x, self.top(index) + inner.y))
    }

    pub(super) fn offset_for_point(&self, lines: &Lines, target: Point<Pixels>) -> usize {
        let Some(metrics) = self.metrics.as_ref() else {
            return 0;
        };
        let index = self.line_at_y(target.y);
        let Some(line) = lines.line(index) else {
            return 0;
        };
        let Some(shape) = current_shape(line, metrics) else {
            return line.start();
        };
        let top = self.top(index);
        let last_row_y = (self.bottom(index) - top - metrics.line_height).max(px(0.0));
        let local_y = (target.y - top).clamp(px(0.0), last_row_y);
        let local = shape
            .line
            .closest_index_for_position(point(target.x, local_y), metrics.line_height)
            .unwrap_or_else(|clamped| clamped);
        line.start() + local.min(line.text().len())
    }
}

fn current_shape<'a>(line: &'a Line, metrics: &TextMetrics) -> Option<&'a Rc<LineShape>> {
    line.payload
        .as_ref()
        .filter(|shape| shape.wrap_width == metrics.wrap_width)
}

fn rows(line: &Line, metrics: &TextMetrics) -> usize {
    match current_shape(line, metrics) {
        Some(shape) => shape.line.wrap_boundaries().len() + 1,
        None => estimated_rows(line.chars(), metrics),
    }
}

fn estimated_rows(chars: usize, metrics: &TextMetrics) -> usize {
    if chars == 0 || metrics.wrap_width <= px(0.0) {
        return 1;
    }
    let rows = (metrics.advance * chars) / metrics.wrap_width;
    (rows.ceil() as usize).max(1)
}

pub(super) fn row_segments(
    shape: &LineShape,
    range: Range<usize>,
    line_height: Pixels,
) -> Vec<RowSegment> {
    let layout = &shape.line.unwrapped_layout;
    let starts: Vec<usize> = iter::once(0)
        .chain(shape.line.wrap_boundaries().iter().filter_map(|boundary| {
            layout
                .runs
                .get(boundary.run_ix)
                .and_then(|run| run.glyphs.get(boundary.glyph_ix))
                .map(|glyph| glyph.index)
        }))
        .collect();
    let ends = starts.iter().skip(1).copied().chain(iter::once(layout.len));
    starts
        .iter()
        .zip(ends)
        .enumerate()
        .filter_map(|(row, (&row_start, row_end))| {
            let from = range.start.max(row_start);
            let to = range.end.min(row_end);
            (from < to).then(|| {
                let origin = layout.x_for_index(row_start);
                RowSegment {
                    y: line_height * row,
                    x: layout.x_for_index(from) - origin..layout.x_for_index(to) - origin,
                }
            })
        })
        .collect()
}

pub(super) fn line_end_point(shape: &LineShape, line_height: Pixels) -> Point<Pixels> {
    shape
        .line
        .position_for_index(shape.line.len(), line_height)
        .unwrap_or_default()
}
