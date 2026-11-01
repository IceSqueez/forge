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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use forge_components::FORGE_DEFAULT;
    use time::OffsetDateTime;

    use super::*;

    const COLUMNS: usize = 4;

    fn at() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::from_millis(1_234)
    }

    fn line(level: Level, target: &'static str, message: &str) -> LogLine {
        LogLine {
            at: at(),
            level,
            target: target.into(),
            message: message.to_owned(),
        }
    }

    fn slices(shown: &LogTailText) -> Vec<&str> {
        shown
            .tints
            .iter()
            .map(|tint| &shown.text[tint.range.clone()])
            .collect()
    }

    #[test]
    fn no_lines_show_nothing() {
        let shown = log_tail_text(&[]);

        assert_eq!(shown.text, "");
        assert!(shown.tints.is_empty());
    }

    #[test]
    fn a_line_reads_time_level_target_message_with_a_tint_on_each_column() {
        let shown = log_tail_text(&[line(Level::ERROR, "hyper::client", "закрито")]);
        let time = fmt_display(DateShape::TimeWithMillis, at());

        assert_eq!(shown.text, format!("{time}  ERROR  hyper::client  закрито"));
        assert_eq!(
            slices(&shown),
            vec![time.as_str(), "ERROR", "hyper::client", "закрито"],
        );
    }

    #[test]
    fn every_level_pads_to_the_same_width_so_the_targets_line_up() {
        let target_starts: Vec<usize> = [
            (Level::ERROR, "ERROR"),
            (Level::WARN, "WARN "),
            (Level::INFO, "INFO "),
            (Level::DEBUG, "DEBUG"),
            (Level::TRACE, "TRACE"),
        ]
        .into_iter()
        .map(|(level, column)| {
            let shown = log_tail_text(&[line(level, "probe", "m")]);
            assert_eq!(slices(&shown)[1], column);
            shown.tints[2].range.start
        })
        .collect();

        assert!(target_starts.iter().all(|start| *start == target_starts[0]));
    }

    #[test]
    fn the_level_column_alone_is_weighted_and_colored_by_severity() {
        for (level, expected) in [
            (Level::ERROR, FORGE_DEFAULT.danger),
            (Level::WARN, FORGE_DEFAULT.warning),
            (Level::INFO, FORGE_DEFAULT.success),
            (Level::DEBUG, FORGE_DEFAULT.text_muted),
            (Level::TRACE, FORGE_DEFAULT.text_muted),
        ] {
            let shown = log_tail_text(&[line(level, "probe", "m")]);
            let columns: Vec<_> = shown
                .tints
                .iter()
                .map(|tint| ((tint.color)(&FORGE_DEFAULT), tint.weight))
                .collect();

            assert_eq!(
                columns,
                vec![
                    (FORGE_DEFAULT.text_faint, None),
                    (expected, Some(FontWeight::MEDIUM)),
                    (FORGE_DEFAULT.brand, None),
                    (FORGE_DEFAULT.text_primary, None),
                ],
                "{level}",
            );
        }
    }

    #[test]
    fn lines_sit_on_their_own_rows_with_tints_in_order_on_character_boundaries() {
        let shown = log_tail_text(&[
            line(Level::INFO, "перший", "повідомлення"),
            line(Level::WARN, "second", "другий рядок"),
            line(Level::ERROR, "third", "третій"),
        ]);

        assert_eq!(shown.text.lines().count(), 3);
        assert!(!shown.text.ends_with('\n'));
        assert_eq!(shown.tints.len(), 3 * COLUMNS);
        assert_eq!(slices(&shown)[2 * COLUMNS - 1], "другий рядок");
        assert!(
            shown
                .tints
                .windows(2)
                .all(|pair| pair[0].range.end <= pair[1].range.start),
            "a tint out of order is skipped when the text is painted",
        );
        assert_eq!(
            shown.tints.last().map(|tint| tint.range.end),
            Some(shown.text.len()),
        );
    }
}
