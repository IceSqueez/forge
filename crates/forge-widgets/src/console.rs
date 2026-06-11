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
            text(crate::tr!("widget_console_no_output"))
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
