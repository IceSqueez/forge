use forge_components::{DateShape, ForgePalette, PaletteColor, TextTint, fmt_display};
use gpui::{FontWeight, Rgba};
use tracing::Level;

use crate::app::log_tail::LogLine;

const COLUMN_GAP: &str = "  ";
const LEVEL_COLUMN_CHARS: usize = 5;

#[derive(Default)]
pub(crate) struct LogTailText {
    pub text: String,
    pub tints: Vec<TextTint>,
}

impl LogTailText {
    fn column(&mut self, value: &str, color: PaletteColor, weight: Option<FontWeight>) {
        let start = self.text.len();
        self.text.push_str(value);
        self.tints.push(TextTint {
            range: start..self.text.len(),
            color,
            weight,
        });
    }
}

pub(crate) fn log_tail_text(lines: &[LogLine]) -> LogTailText {
    let mut shown = LogTailText::default();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            shown.text.push('\n');
        }
        shown.column(
            &fmt_display(DateShape::TimeWithMillis, line.at),
            |palette| palette.text_faint,
            None,
        );
        shown.text.push_str(COLUMN_GAP);
        shown.column(
            &format!("{:<LEVEL_COLUMN_CHARS$}", line.level.as_str()),
            level_color(line.level),
            Some(FontWeight::MEDIUM),
        );
        shown.text.push_str(COLUMN_GAP);
        shown.column(&line.target, |palette| palette.brand, None);
        shown.text.push_str(COLUMN_GAP);
        shown.column(&line.message, |palette| palette.text_primary, None);
    }
    shown
}

fn level_color(level: Level) -> PaletteColor {
    if level == Level::ERROR {
        |palette: &ForgePalette| -> Rgba { palette.danger }
    } else if level == Level::WARN {
        |palette: &ForgePalette| -> Rgba { palette.warning }
    } else if level == Level::INFO {
        |palette: &ForgePalette| -> Rgba { palette.success }
    } else {
        |palette: &ForgePalette| -> Rgba { palette.text_muted }
    }
}
