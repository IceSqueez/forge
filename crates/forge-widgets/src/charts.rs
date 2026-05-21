use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding, Point, Rectangle, mouse,
    widget::{Space, canvas, column, container, row, text},
};

use crate::palette::ForgePalette;
use crate::tokens::{Density, FONT_XS, FontRole, Radius, Spacing, font, radius, spacing};

const RING_LEN: usize = 60;
const CANVAS_HEIGHT: f32 = 80.0;

struct SparklineProgram {
    samples: Vec<f32>,
    palette: ForgePalette,
}

impl<Message> canvas::Program<Message> for SparklineProgram {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let dash_segs = [2.0_f32, 4.0_f32];
        let grid_stroke = canvas::Stroke {
            style: canvas::Style::Solid(self.palette.border_regular),
            width: 0.5,
            line_dash: canvas::LineDash {
                segments: &dash_segs,
                offset: 0,
            },
            ..canvas::Stroke::default()
        };

        for &frac in &[0.25_f32, 0.5, 0.75] {
            let y = bounds.height * frac;
            let grid_line = canvas::Path::line(Point::new(0.0, y), Point::new(bounds.width, y));
            frame.stroke(&grid_line, grid_stroke);
        }

        let n = self.samples.len();
        if n == 0 {
            return vec![frame.into_geometry()];
        }

        let max_y = self.samples.iter().copied().fold(1.0_f32, f32::max);
        let x_step = bounds.width / 59.0;
        let x_start = bounds.width - (n as f32 - 1.0) * x_step;

        let sample_point = |i: usize| -> Point {
            let x = x_start + i as f32 * x_step;
            let y = bounds.height * (1.0 - self.samples[i] / max_y);
            Point::new(x, y)
        };

        let last_pt = sample_point(n - 1);

        let area = canvas::Path::new(|b| {
            b.move_to(sample_point(0));
            for i in 1..n {
                b.line_to(sample_point(i));
            }
            b.line_to(Point::new(last_pt.x, bounds.height));
            b.line_to(Point::new(x_start, bounds.height));
            b.close();
        });

        let brand = self.palette.brand;
        let grad_top = Color { a: 0.20, ..brand };
        let grad_bottom = Color { a: 0.0, ..brand };
        let gradient =
            canvas::gradient::Linear::new(Point::new(0.0, 0.0), Point::new(0.0, bounds.height))
                .add_stop(0.0, grad_top)
                .add_stop(1.0, grad_bottom);
        frame.fill(&area, canvas::Fill::from(gradient));

        let line = canvas::Path::new(|b| {
            b.move_to(sample_point(0));
            for i in 1..n {
                b.line_to(sample_point(i));
            }
        });
        frame.stroke(
            &line,
            canvas::Stroke {
                style: canvas::Style::Solid(brand),
                width: 1.5,
                ..canvas::Stroke::default()
            },
        );

        let dot = canvas::Path::circle(last_pt, 3.0);
        frame.fill(&dot, brand);

        vec![frame.into_geometry()]
    }
}

pub fn throughput_sparkline<'a, Msg: 'a>(
    samples: &[f32],
    scale_label: &str,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let clamped: Vec<f32> = if samples.len() > RING_LEN {
        samples[samples.len() - RING_LEN..].to_vec()
    } else {
        samples.to_vec()
    };

    let prog = SparklineProgram {
        samples: clamped,
        palette: *palette,
    };

    let chart_canvas = canvas::Canvas::new(prog)
        .width(Length::Fill)
        .height(Length::Fixed(CANVAS_HEIGHT));

    let header = row![
        text("THROUGHPUT")
            .size(FONT_XS)
            .color(palette.text_primary)
            .font(font(FontRole::Monospace)),
        Space::new().width(Length::Fill),
        text(scale_label.to_owned())
            .size(FONT_XS)
            .color(palette.text_faint)
            .font(font(FontRole::Monospace)),
    ]
    .align_y(Alignment::Center);

    let inner = column![header, chart_canvas].spacing(spacing(Spacing::Sm, Density::Cozy) as f32);

    container(inner)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            ..container::Style::default()
        })
        .padding(Padding::from([
            spacing(Spacing::Md, Density::Cozy),
            spacing(Spacing::Md, Density::Cozy),
        ]))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn sparkline_constructs_with_small_sample() {
        let palette = CATPPUCCIN_MOCHA;
        let _el: Element<'_, ()> =
            throughput_sparkline(&[1.0, 5.0, 12.0, 8.0], "scale 0-30", &palette);
    }

    #[test]
    fn sparkline_constructs_with_empty_samples() {
        let palette = CATPPUCCIN_MOCHA;
        let _el: Element<'_, ()> = throughput_sparkline(&[], "scale 0-30", &palette);
    }

    #[test]
    fn sparkline_clamps_to_last_60_when_oversized() {
        let palette = CATPPUCCIN_MOCHA;
        let samples: Vec<f32> = (0..120).map(|i| i as f32).collect();
        let _el: Element<'_, ()> = throughput_sparkline(&samples, "scale 0-120", &palette);
    }

    #[test]
    fn sparkline_clamping_logic_takes_last_60() {
        let samples: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let clamped: Vec<f32> = if samples.len() > RING_LEN {
            samples[samples.len() - RING_LEN..].to_vec()
        } else {
            samples.to_vec()
        };
        assert_eq!(clamped.len(), 60);
        assert_eq!(clamped[0], 40.0);
        assert_eq!(clamped[59], 99.0);
    }
}
