use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding,
    widget::{column, container, scrollable, text},
};

use crate::{
    ForgePalette,
    tokens::{FONT_XS, FontRole, Spacing, font, sp, spf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleLevel {
    Run,
    Info,
    Ok,
    Stats,
    Warn,
    Err,
}

impl ConsoleLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Info => "info",
            Self::Ok => "ok",
            Self::Stats => "stats",
            Self::Warn => "warn",
            Self::Err => "err",
        }
    }

    pub fn color(self, palette: &ForgePalette) -> Color {
        match self {
            Self::Run => palette.info,
            Self::Info => palette.success,
            Self::Ok => palette.success,
            Self::Stats => palette.brand,
            Self::Warn => palette.warning,
            Self::Err => palette.random,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConsoleLine {
    pub level: ConsoleLevel,
    pub timestamp: Option<String>,
    pub text: String,
}

pub fn console<'a, Msg: 'a>(
    palette: &'a ForgePalette,
    lines: &'a [ConsoleLine],
) -> Element<'a, Msg> {
    let shell = palette.shell;
    let mono = font(FontRole::Monospace);

    let inner: Element<'a, Msg> = if lines.is_empty() {
        container(
            text("No output yet")
                .font(mono)
                .size(FONT_XS)
                .color(palette.text_faint),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    } else {
        let rows: Vec<Element<'a, Msg>> = lines
            .iter()
            .map(|line| {
                let level_color = line.level.color(palette);
                let text_muted = palette.text_muted;
                let text_faint = palette.text_faint;
                let bracket_label = format!("[{}]", line.level.label());
                let line_text = line.text.clone();

                let mut items: Vec<Element<'a, Msg>> = Vec::new();

                if let Some(ts) = &line.timestamp {
                    items.push(
                        text(ts.clone())
                            .font(mono)
                            .size(FONT_XS)
                            .color(text_faint)
                            .into(),
                    );
                    items.push(text("  ").font(mono).size(FONT_XS).into());
                }

                items.push(
                    text(bracket_label)
                        .font(mono)
                        .size(FONT_XS)
                        .color(level_color)
                        .into(),
                );
                items.push(text("  ").font(mono).size(FONT_XS).into());
                items.push(
                    text(line_text)
                        .font(mono)
                        .size(FONT_XS)
                        .color(text_muted)
                        .into(),
                );

                iced::widget::row(items)
                    .padding(Padding {
                        top: spf(Spacing::Xxs),
                        bottom: spf(Spacing::Xxs),
                        left: 0.0,
                        right: 0.0,
                    })
                    .into()
            })
            .collect();

        scrollable(column(rows)).height(Length::Fill).into()
    };

    container(inner)
        .width(Length::Fill)
        .padding([sp(Spacing::Xs), sp(Spacing::Md)])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(shell)),
            border: Border::default(),
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn console_level_labels_all_variants() {
        assert_eq!(ConsoleLevel::Run.label(), "run");
        assert_eq!(ConsoleLevel::Info.label(), "info");
        assert_eq!(ConsoleLevel::Ok.label(), "ok");
        assert_eq!(ConsoleLevel::Stats.label(), "stats");
        assert_eq!(ConsoleLevel::Warn.label(), "warn");
        assert_eq!(ConsoleLevel::Err.label(), "err");
    }

    #[test]
    fn console_level_colors_six_distinct_values() {
        let p = &CATPPUCCIN_MOCHA;
        let colors = [
            ConsoleLevel::Run.color(p),
            ConsoleLevel::Info.color(p),
            ConsoleLevel::Ok.color(p),
            ConsoleLevel::Stats.color(p),
            ConsoleLevel::Warn.color(p),
            ConsoleLevel::Err.color(p),
        ];
        assert_eq!(colors[0], p.info);
        assert_eq!(colors[1], p.success);
        assert_eq!(colors[2], p.success);
        assert_eq!(colors[3], p.brand);
        assert_eq!(colors[4], p.warning);
        assert_eq!(colors[5], p.random);
    }

    #[test]
    fn console_level_run_and_ok_differ() {
        let p = &CATPPUCCIN_MOCHA;
        assert_ne!(ConsoleLevel::Run.color(p), ConsoleLevel::Ok.color(p));
    }

    #[test]
    fn console_level_stats_is_brand() {
        let p = &CATPPUCCIN_MOCHA;
        assert_eq!(ConsoleLevel::Stats.color(p), p.brand);
    }

    #[test]
    fn console_widget_empty_state_compiles() {
        let _: Element<'_, ()> = console(&CATPPUCCIN_MOCHA, &[]);
    }

    #[test]
    fn console_widget_with_lines_compiles() {
        let lines = vec![
            ConsoleLine {
                level: ConsoleLevel::Run,
                timestamp: Some("14:23:14".to_string()),
                text: "format_quote.rhai with sample inputs".to_string(),
            },
            ConsoleLine {
                level: ConsoleLevel::Ok,
                timestamp: Some("14:23:14".to_string()),
                text: "returned: ok".to_string(),
            },
            ConsoleLine {
                level: ConsoleLevel::Stats,
                timestamp: None,
                text: "executed in 1.84ms".to_string(),
            },
            ConsoleLine {
                level: ConsoleLevel::Warn,
                timestamp: None,
                text: "sandbox limit at 80%".to_string(),
            },
            ConsoleLine {
                level: ConsoleLevel::Err,
                timestamp: Some("14:23:15".to_string()),
                text: "script error: undefined variable".to_string(),
            },
        ];
        let _: Element<'_, ()> = console(&CATPPUCCIN_MOCHA, &lines);
    }

    #[test]
    fn console_line_without_timestamp_compiles() {
        let lines = vec![ConsoleLine {
            level: ConsoleLevel::Info,
            timestamp: None,
            text: "no timestamp line".to_string(),
        }];
        let _: Element<'_, ()> = console(&CATPPUCCIN_MOCHA, &lines);
    }
}
