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
