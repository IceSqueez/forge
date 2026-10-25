use thiserror::Error;

use super::names::{NameForm, NameWidth};

const QUOTE: char = '\'';
const MAX_FRACTION_DIGITS: usize = 9;
const FRACTION_LETTER: char = 'S';
const SUPPORTED_LETTERS: &str = "yMLdEcHhamsSx";

const DOT_NET_SPELLINGS: [(&str, &str); 3] = [("dddd", "EEEE"), ("ddd", "EEE"), ("tt", "a")];

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DatePatternError {
    #[error("the date pattern is empty")]
    Empty,
    #[error("quoted text starting at position {position} is never closed with '")]
    UnterminatedQuote { position: usize },
    #[error(
        "'{letter}' at position {position} is not a date pattern letter; put literal text in single quotes"
    )]
    UnsupportedLetter { letter: char, position: usize },
    #[error("'{spelling}' at position {position} is not a supported length for '{letter}'")]
    UnsupportedLength {
        letter: char,
        spelling: String,
        position: usize,
    },
    #[error("'{spelling}' at position {position} is not supported; use '{replacement}'")]
    DotNetSpelling {
        spelling: String,
        replacement: &'static str,
        position: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Padding {
    Minimal,
    TwoDigits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DateField {
    Year,
    YearFourDigits,
    YearTwoDigits,
    MonthNumber(Padding),
    MonthName(NameForm, NameWidth),
    Day(Padding),
    WeekdayName(NameForm, NameWidth),
    Hour24(Padding),
    Hour12(Padding),
    DayPeriod,
    Minute(Padding),
    Second(Padding),
    Fraction(usize),
    OffsetHoursMinutes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PatternPiece {
    Literal(String),
    Field(DateField),
}

pub(super) fn parse_pieces(pattern: &str) -> Result<Vec<PatternPiece>, DatePatternError> {
    if pattern.is_empty() {
        return Err(DatePatternError::Empty);
    }
    let chars: Vec<char> = pattern.chars().collect();
    let mut pieces = Vec::new();
    let mut literal = String::new();
    let mut index = 0;
    while let Some(&current) = chars.get(index) {
        if current == QUOTE {
            index = read_quoted(&chars, index, &mut literal)?;
        } else if current.is_ascii_alphabetic() {
            let run_end = chars[index..]
                .iter()
                .position(|&c| c != current)
                .map_or(chars.len(), |offset| index + offset);
            let field = field_for_run(current, run_end - index, index)?;
            if !literal.is_empty() {
                pieces.push(PatternPiece::Literal(std::mem::take(&mut literal)));
            }
            pieces.push(PatternPiece::Field(field));
            index = run_end;
        } else {
            literal.push(current);
            index += 1;
        }
    }
    if !literal.is_empty() {
        pieces.push(PatternPiece::Literal(literal));
    }
    Ok(pieces)
}

fn read_quoted(
    chars: &[char],
    open: usize,
    literal: &mut String,
) -> Result<usize, DatePatternError> {
    if chars.get(open + 1) == Some(&QUOTE) {
        literal.push(QUOTE);
        return Ok(open + 2);
    }
    let mut index = open + 1;
    loop {
        match chars.get(index) {
            None => return Err(DatePatternError::UnterminatedQuote { position: open }),
            Some(&QUOTE) if chars.get(index + 1) == Some(&QUOTE) => {
                literal.push(QUOTE);
                index += 2;
            }
            Some(&QUOTE) => return Ok(index + 1),
            Some(&other) => {
                literal.push(other);
                index += 1;
            }
        }
    }
}

fn field_for_run(
    letter: char,
    length: usize,
    position: usize,
) -> Result<DateField, DatePatternError> {
    let spelling: String = std::iter::repeat_n(letter, length).collect();
    if let Some(field) = field_for_spelling(&spelling) {
        return Ok(field);
    }
    if letter == FRACTION_LETTER && length <= MAX_FRACTION_DIGITS {
        return Ok(DateField::Fraction(length));
    }
    if let Some((_, replacement)) = DOT_NET_SPELLINGS
        .iter()
        .find(|(dot_net, _)| *dot_net == spelling)
    {
        return Err(DatePatternError::DotNetSpelling {
            spelling,
            replacement,
            position,
        });
    }
    if SUPPORTED_LETTERS.contains(letter) {
        Err(DatePatternError::UnsupportedLength {
            letter,
            spelling,
            position,
        })
    } else {
        Err(DatePatternError::UnsupportedLetter { letter, position })
    }
}

fn field_for_spelling(spelling: &str) -> Option<DateField> {
    use DateField::*;
    use NameForm::{Format, StandAlone};
    use NameWidth::{Abbreviated, Wide};
    Some(match spelling {
        "y" => Year,
        "yy" => YearTwoDigits,
        "yyyy" => YearFourDigits,
        "M" => MonthNumber(Padding::Minimal),
        "MM" => MonthNumber(Padding::TwoDigits),
        "MMM" => MonthName(Format, Abbreviated),
        "MMMM" => MonthName(Format, Wide),
        "LLL" => MonthName(StandAlone, Abbreviated),
        "LLLL" => MonthName(StandAlone, Wide),
        "d" => Day(Padding::Minimal),
        "dd" => Day(Padding::TwoDigits),
        "EEE" => WeekdayName(Format, Abbreviated),
        "EEEE" => WeekdayName(Format, Wide),
        "ccc" => WeekdayName(StandAlone, Abbreviated),
        "cccc" => WeekdayName(StandAlone, Wide),
        "H" => Hour24(Padding::Minimal),
        "HH" => Hour24(Padding::TwoDigits),
        "h" => Hour12(Padding::Minimal),
        "hh" => Hour12(Padding::TwoDigits),
        "a" => DayPeriod,
        "m" => Minute(Padding::Minimal),
        "mm" => Minute(Padding::TwoDigits),
        "s" => Second(Padding::Minimal),
        "ss" => Second(Padding::TwoDigits),
        "xxx" => OffsetHoursMinutes,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use time::{Date, Month, UtcOffset};

    use super::DatePatternError::{self, *};
    use crate::date_pattern::{DatePattern, NamesLanguage, PatternZone};

    fn render_utc(pattern: &str) -> String {
        let value = Date::from_calendar_date(2026, Month::March, 7)
            .unwrap()
            .with_hms(4, 5, 9)
            .unwrap()
            .assume_offset(UtcOffset::UTC);
        DatePattern::parse(pattern)
            .unwrap()
            .format(value, PatternZone::Utc, NamesLanguage::En)
    }

    fn length(letter: char, spelling: &str, position: usize) -> DatePatternError {
        UnsupportedLength {
            letter,
            spelling: spelling.to_owned(),
            position,
        }
    }

    fn dot_net(spelling: &str, replacement: &'static str, position: usize) -> DatePatternError {
        DotNetSpelling {
            spelling: spelling.to_owned(),
            replacement,
            position,
        }
    }

    #[test]
    fn literal_text_and_quotes_render_verbatim() {
        for (pattern, expected) in [
            ("''", "'"),
            ("'o''clock'", "o'clock"),
            ("h 'o''clock'", "4 o'clock"),
            ("HH''mm", "04'05"),
            ("'it''s' HH", "it's 04"),
            ("'yyyy MM'", "yyyy MM"),
            ("HHmm", "0405"),
            ("HH:mm:ss", "04:05:09"),
            ("привіт HH", "привіт 04"),
            ("dd.MM.yyyy, HH:mm", "07.03.2026, 04:05"),
            ("'' ''", "' '"),
        ] {
            assert_eq!(render_utc(pattern), expected, "pattern {pattern:?}");
        }
    }

    #[test]
    fn parse_rejects_with_exact_variant_and_char_position() {
        for (pattern, expected) in [
            ("", Empty),
            (
                "Q",
                UnsupportedLetter {
                    letter: 'Q',
                    position: 0,
                },
            ),
            (
                "yyyy-MM-dd Q",
                UnsupportedLetter {
                    letter: 'Q',
                    position: 11,
                },
            ),
            (
                "привіт Q",
                UnsupportedLetter {
                    letter: 'Q',
                    position: 7,
                },
            ),
            (
                "HH:mm G",
                UnsupportedLetter {
                    letter: 'G',
                    position: 6,
                },
            ),
            ("yyy", length('y', "yyy", 0)),
            ("yyyyy", length('y', "yyyyy", 0)),
            ("MMMMM", length('M', "MMMMM", 0)),
            ("L", length('L', "L", 0)),
            ("LL", length('L', "LL", 0)),
            ("E", length('E', "E", 0)),
            ("EEEEE", length('E', "EEEEE", 0)),
            ("c", length('c', "c", 0)),
            ("aa", length('a', "aa", 0)),
            ("x", length('x', "x", 0)),
            ("xx", length('x', "xx", 0)),
            ("HHH", length('H', "HHH", 0)),
            ("SSSSSSSSSS", length('S', "SSSSSSSSSS", 0)),
            ("ss.SSSSSSSSSS", length('S', "SSSSSSSSSS", 3)),
            ("dddd", dot_net("dddd", "EEEE", 0)),
            ("ddd", dot_net("ddd", "EEE", 0)),
            ("HH:mm tt", dot_net("tt", "a", 6)),
            ("ddddd", length('d', "ddddd", 0)),
            (
                "t",
                UnsupportedLetter {
                    letter: 't',
                    position: 0,
                },
            ),
            ("'", UnterminatedQuote { position: 0 }),
            ("HH 'abc", UnterminatedQuote { position: 3 }),
            ("'abc''", UnterminatedQuote { position: 0 }),
            ("привіт 'x", UnterminatedQuote { position: 7 }),
        ] {
            assert_eq!(
                DatePattern::parse(pattern),
                Err(expected),
                "pattern {pattern:?}"
            );
        }
    }
}
