use gpui::{
    App, Background, Bounds, Hsla, IntoElement, PathBuilder, Pixels, Point, RenderOnce, Rgba,
    Styled, Window, canvas, linear_color_stop, linear_gradient, point, px,
};

use crate::palette::with_alpha;

const DEFAULT_FILL_ALPHA: f32 = 0.14;
const DEFAULT_STROKE_WIDTH: f32 = 1.5;
const TOP_TO_BOTTOM_DEGREES: f32 = 180.0;
const FLAT_SERIES_FRACTION: f32 = 0.5;

#[derive(Clone, Copy)]
enum SparkFill {
    Flat(Rgba),
    Fade(Rgba),
}

impl SparkFill {
    fn background(self) -> Background {
        match self {
            Self::Flat(color) => Hsla::from(color).into(),
            Self::Fade(color) => linear_gradient(
                TOP_TO_BOTTOM_DEGREES,
                linear_color_stop(Hsla::from(color), 0.0),
                linear_color_stop(Hsla::from(with_alpha(color, 0.0)), 1.0),
            ),
        }
    }
}

#[derive(IntoElement)]
pub struct Sparkline {
    samples: Vec<f32>,
    line: Rgba,
    fill: Option<SparkFill>,
    stroke_width: f32,
    zero_baseline: bool,
}

pub fn sparkline(samples: &[f32], color: Rgba) -> Sparkline {
    Sparkline {
        samples: samples.to_vec(),
        line: color,
        fill: Some(SparkFill::Flat(with_alpha(color, DEFAULT_FILL_ALPHA))),
        stroke_width: DEFAULT_STROKE_WIDTH,
        zero_baseline: false,
    }
}

impl Sparkline {
    #[must_use]
    pub fn fading_fill(mut self, top_alpha: f32) -> Self {
        self.fill = Some(SparkFill::Fade(with_alpha(self.line, top_alpha)));
        self
    }

    #[must_use]
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = width;
        self
    }

    #[must_use]
    pub fn zero_baseline(mut self) -> Self {
        self.zero_baseline = true;
        self
    }
}

fn flat_fraction(zero_baseline: bool) -> f32 {
    if zero_baseline {
        0.0
    } else {
        FLAT_SERIES_FRACTION
    }
}

fn vertical_fractions(samples: &[f32], zero_baseline: bool) -> Vec<f32> {
    let flat = flat_fraction(zero_baseline);
    let mut min = if zero_baseline { 0.0 } else { f32::INFINITY };
    let mut max = f32::NEG_INFINITY;
    for &s in samples {
        if s.is_finite() {
            min = min.min(s);
            max = max.max(s);
        }
    }

    let span = max - min;
    if !span.is_finite() || span <= f32::EPSILON {
        return vec![flat; samples.len()];
    }

    samples
        .iter()
        .map(|&s| {
            if s.is_finite() {
                (s - min) / span
            } else {
                flat
            }
        })
        .collect()
}

impl RenderOnce for Sparkline {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let Sparkline {
            samples,
            line,
            fill,
            stroke_width,
            zero_baseline,
        } = self;
        let flat = flat_fraction(zero_baseline);

        canvas(
            move |_bounds, _window, _cx| {},
            move |bounds: Bounds<Pixels>, _prepaint, window, _cx| {
                let width = f32::from(bounds.size.width);
                let height = f32::from(bounds.size.height);
                if width <= 0.0 || height <= 0.0 {
                    return;
                }

                let left = f32::from(bounds.origin.x);
                let top = f32::from(bounds.origin.y);
                let base_y = top + height;

                let inset = stroke_width.max(1.0).min(height / 2.0);
                let plot_top = top + inset;
                let plot_bottom = base_y - inset;
                let plot_span = plot_bottom - plot_top;

                let fractions = vertical_fractions(&samples, zero_baseline);
                let count = fractions.len();

                let y_at = |frac: f32| plot_bottom - frac * plot_span;
                let x_at = |i: usize| {
                    if count <= 1 {
                        left
                    } else {
                        left + (i as f32 / (count - 1) as f32) * width
                    }
                };

                let pts: Vec<Point<Pixels>> = if count == 0 {
                    vec![
                        point(px(left), px(y_at(flat))),
                        point(px(left + width), px(y_at(flat))),
                    ]
                } else if count == 1 {
                    let y = y_at(fractions[0]);
                    vec![point(px(left), px(y)), point(px(left + width), px(y))]
                } else {
                    fractions
                        .iter()
                        .enumerate()
                        .map(|(i, &frac)| point(px(x_at(i)), px(y_at(frac))))
                        .collect()
                };

                if let Some(fill) = fill {
                    let mut area = PathBuilder::fill();
                    area.move_to(point(px(left), px(base_y)));
                    for p in &pts {
                        area.line_to(*p);
                    }
                    area.line_to(point(px(left + width), px(base_y)));
                    if let Ok(path) = area.build() {
                        window.paint_path(path, fill.background());
                    }
                }

                let mut stroke = PathBuilder::stroke(px(stroke_width));
                let mut segment = pts.iter();
                if let Some(first) = segment.next() {
                    stroke.move_to(*first);
                    for p in segment {
                        stroke.line_to(*p);
                    }
                    if let Ok(path) = stroke.build() {
                        window.paint_path(path, Hsla::from(line));
                    }
                }
            },
        )
        .size_full()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_scale_between_the_series_extremes_without_a_baseline() {
        assert_eq!(
            vertical_fractions(&[2.0, 4.0, 3.0], false),
            vec![0.0, 1.0, 0.5]
        );
    }

    #[test]
    fn a_zero_baseline_measures_positive_samples_from_zero() {
        assert_eq!(vertical_fractions(&[2.0, 4.0], true), vec![0.5, 1.0]);
    }

    #[test]
    fn a_negative_sample_lowers_the_zero_baseline_floor() {
        assert_eq!(
            vertical_fractions(&[-2.0, 0.0, 2.0], true),
            vec![0.0, 0.5, 1.0]
        );
    }

    #[test]
    fn a_series_without_a_measurable_span_draws_at_its_flat_level() {
        let cases: [(&[f32], bool, f32); 7] = [
            (&[3.0, 3.0], false, FLAT_SERIES_FRACTION),
            (&[7.0], false, FLAT_SERIES_FRACTION),
            (&[f32::NAN, f32::INFINITY], false, FLAT_SERIES_FRACTION),
            (&[f32::MIN, f32::MAX], false, FLAT_SERIES_FRACTION),
            (&[0.0, 0.0], true, 0.0),
            (&[f32::NEG_INFINITY], true, 0.0),
            (&[f32::NAN, f32::NAN], true, 0.0),
        ];
        for (samples, zero_baseline, level) in cases {
            assert_eq!(
                vertical_fractions(samples, zero_baseline),
                vec![level; samples.len()],
                "{samples:?} baseline {zero_baseline}"
            );
        }
    }

    #[test]
    fn an_empty_series_yields_no_points() {
        assert!(vertical_fractions(&[], false).is_empty());
    }

    #[test]
    fn a_non_finite_sample_draws_flat_and_leaves_the_scale_of_its_neighbours() {
        assert_eq!(
            vertical_fractions(&[0.0, f32::NAN, 10.0, f32::INFINITY], false),
            vec![0.0, FLAT_SERIES_FRACTION, 1.0, FLAT_SERIES_FRACTION]
        );
    }
}
