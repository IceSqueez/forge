use super::DatePattern;

const COMPONENT_OPEN: char = '[';
const COMPONENT_CLOSE: char = ']';
const ESCAPE: char = '\\';
const MODIFIER_SEPARATOR: char = ':';
const LDML_QUOTE: char = '\'';
const LDML_QUOTED_QUOTE: &str = "''";
const OFFSET_SEPARATOR: char = ':';
const SUBSECOND_DIGIT_COUNTS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

const LEGACY_COMPONENT_NAMES: [&str; 19] = [
    "day",
    "end",
    "first",
    "hour",
    "ignore",
    "minute",
    "month",
    "offset_hour",
    "offset_minute",
    "offset_second",
    "optional",
    "ordinal",
    "period",
    "second",
    "subsecond",
    "unix_timestamp",
    "week_number",
    "weekday",
    "year",
];

const PADDING: (&str, &[&str]) = ("padding", &["zero", "none"]);
const CASE_SENSITIVE: (&str, &[&str]) = ("case_sensitive", &["true", "false"]);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyTranslation {
    NotLegacy,
    Translated(String),
    Untranslatable { component: String },
}

pub fn translate_legacy_pattern(text: &str) -> LegacyTranslation {
    if !looks_legacy(text) {
        return LegacyTranslation::NotLegacy;
    }
    let items = match lex(text) {
        Ok(items) => items,
        Err(component) => return LegacyTranslation::Untranslatable { component },
    };
    match translate(&items) {
        Ok(pattern) if DatePattern::parse(&pattern).is_ok() => {
            LegacyTranslation::Translated(pattern)
        }
        Ok(_) => LegacyTranslation::Untranslatable {
            component: text.to_owned(),
        },
        Err(component) => LegacyTranslation::Untranslatable { component },
    }
}

fn looks_legacy(text: &str) -> bool {
    let names_a_component = text.match_indices(COMPONENT_OPEN).any(|(open, _)| {
        let after = &text[open + COMPONENT_OPEN.len_utf8()..];
        let name_end = after
            .find(|c: char| c == COMPONENT_CLOSE || c.is_ascii_whitespace())
            .unwrap_or(after.len());
        name_end < after.len()
            && LEGACY_COMPONENT_NAMES
                .iter()
                .any(|name| after[..name_end].eq_ignore_ascii_case(name))
    });
    let quoted_ldml = text.contains(LDML_QUOTE) && DatePattern::parse(text).is_ok();
    names_a_component && !quoted_ldml
}

enum Item {
    Literal(char),
    Component(Component),
}

struct Component {
    raw: String,
    name: String,
    modifiers: Vec<(String, String)>,
}

impl Component {
    fn value(&self, key: &str) -> Option<&str> {
        self.modifiers
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    fn admits_only(&self, allowed: &[(&str, &[&str])]) -> bool {
        self.modifiers.iter().all(|(key, value)| {
            allowed
                .iter()
                .any(|(name, values)| name == key && values.contains(&value.as_str()))
        })
    }

    fn untranslatable(&self) -> String {
        self.raw.clone()
    }
}

fn lex(text: &str) -> Result<Vec<Item>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut items = Vec::new();
    let mut index = 0;
    while let Some(&current) = chars.get(index) {
        match current {
            ESCAPE => match chars.get(index + 1) {
                Some(&escaped @ (ESCAPE | COMPONENT_OPEN | COMPONENT_CLOSE)) => {
                    items.push(Item::Literal(escaped));
                    index += 2;
                }
                _ => return Err(chars[index..].iter().collect()),
            },
            COMPONENT_OPEN => {
                let close = matching_close(&chars, index)
                    .ok_or_else(|| chars[index..].iter().collect::<String>())?;
                items.push(Item::Component(component(&chars[index..=close])));
                index = close + 1;
            }
            COMPONENT_CLOSE => return Err(chars[index..].iter().collect()),
            other => {
                items.push(Item::Literal(other));
                index += 1;
            }
        }
    }
    Ok(items)
}

fn matching_close(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = open;
    while let Some(&current) = chars.get(index) {
        match current {
            ESCAPE => index += 1,
            COMPONENT_OPEN => depth += 1,
            COMPONENT_CLOSE => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn component(bracketed: &[char]) -> Component {
    let raw: String = bracketed.iter().collect();
    let inner = raw
        .strip_prefix(COMPONENT_OPEN)
        .and_then(|rest| rest.strip_suffix(COMPONENT_CLOSE))
        .unwrap_or_default();
    let mut words = inner.split_ascii_whitespace();
    let name = words.next().unwrap_or_default().to_ascii_lowercase();
    let modifiers = words
        .map(|word| match word.split_once(MODIFIER_SEPARATOR) {
            Some((key, value)) => (key.to_ascii_lowercase(), value.to_ascii_lowercase()),
            None => (word.to_ascii_lowercase(), String::new()),
        })
        .collect();
    Component {
        raw,
        name,
        modifiers,
    }
}

fn translate(items: &[Item]) -> Result<String, String> {
    let mut pattern = String::new();
    let mut literal = String::new();
    let mut index = 0;
    while let Some(item) = items.get(index) {
        match item {
            Item::Literal(c) => {
                literal.push(*c);
                index += 1;
            }
            Item::Component(component) => {
                push_literal(&mut pattern, &std::mem::take(&mut literal));
                let (spelling, consumed) = translate_component(component, &items[index + 1..])?;
                pattern.push_str(&spelling);
                index += 1 + consumed;
            }
        }
    }
    push_literal(&mut pattern, &literal);
    Ok(pattern)
}

fn push_literal(pattern: &mut String, literal: &str) {
    if literal.is_empty() {
        return;
    }
    let escaped = literal.replace(LDML_QUOTE, LDML_QUOTED_QUOTE);
    if literal.chars().any(|c| c.is_ascii_alphabetic()) {
        pattern.push(LDML_QUOTE);
        pattern.push_str(&escaped);
        pattern.push(LDML_QUOTE);
    } else {
        pattern.push_str(&escaped);
    }
}

fn translate_component(component: &Component, rest: &[Item]) -> Result<(String, usize), String> {
    let zero_padded = component.value(PADDING.0) != Some("none");
    let pick = |padded: &str, minimal: &str| if zero_padded { padded } else { minimal }.to_owned();
    let spelling = match component.name.as_str() {
        "day" if component.admits_only(&[PADDING]) => pick("dd", "d"),
        "minute" if component.admits_only(&[PADDING]) => pick("mm", "m"),
        "second" if component.admits_only(&[PADDING]) => pick("ss", "s"),
        "hour" if component.admits_only(&[PADDING, ("repr", &["24", "12"])]) => {
            match component.value("repr") {
                Some("12") => pick("hh", "h"),
                _ => pick("HH", "H"),
            }
        }
        "month"
            if component.admits_only(&[
                PADDING,
                ("repr", &["numerical", "long", "short"]),
                CASE_SENSITIVE,
            ]) =>
        {
            match component.value("repr") {
                Some("long") => "MMMM".to_owned(),
                Some("short") => "MMM".to_owned(),
                _ => pick("MM", "M"),
            }
        }
        "weekday"
            if component.admits_only(&[
                ("repr", &["long", "short"]),
                ("one_indexed", &["true", "false"]),
                CASE_SENSITIVE,
            ]) =>
        {
            match component.value("repr") {
                Some("short") => "EEE".to_owned(),
                _ => "EEEE".to_owned(),
            }
        }
        "period" if component.admits_only(&[("case", &["upper"]), CASE_SENSITIVE]) => {
            "a".to_owned()
        }
        "year"
            if component.admits_only(&[
                PADDING,
                ("repr", &["full", "last_two"]),
                ("base", &["calendar"]),
                ("sign", &["automatic"]),
                ("range", &["standard", "extended"]),
            ]) =>
        {
            match (component.value("repr"), zero_padded) {
                (Some("last_two"), true) => "yy".to_owned(),
                (Some("last_two"), false) => return Err(component.untranslatable()),
                _ => pick("yyyy", "y"),
            }
        }
        "subsecond" if component.admits_only(&[("digits", &SUBSECOND_DIGIT_COUNTS)]) => {
            let digits = component
                .value("digits")
                .and_then(|count| count.parse::<usize>().ok())
                .ok_or_else(|| component.untranslatable())?;
            "S".repeat(digits)
        }
        "offset_hour" => return offset_hours_minutes(component, rest),
        _ => return Err(component.untranslatable()),
    };
    Ok((spelling, 0))
}

fn offset_hours_minutes(hour: &Component, rest: &[Item]) -> Result<(String, usize), String> {
    let hour_ok = hour.value("sign") == Some("mandatory")
        && hour.admits_only(&[("sign", &["mandatory"]), ("padding", &["zero"])]);
    let followed_by_minutes = matches!(
        rest,
        [Item::Literal(OFFSET_SEPARATOR), Item::Component(minute), ..]
            if minute.name == "offset_minute" && minute.admits_only(&[("padding", &["zero"])])
    );
    if hour_ok && followed_by_minutes {
        Ok(("xxx".to_owned(), 2))
    } else {
        Err(hour.untranslatable())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use time::{Date, Month, OffsetDateTime, UtcOffset};

    use super::{LegacyTranslation, translate_legacy_pattern};
    use crate::date_pattern::{DatePattern, NamesLanguage, PatternZone};

    fn sample_instants() -> Vec<OffsetDateTime> {
        let at = |year: i32, month: Month, day: u8, hms: (u8, u8, u8), nanos: u32, offset: i32| {
            Date::from_calendar_date(year, month, day)
                .unwrap()
                .with_hms_nano(hms.0, hms.1, hms.2, nanos)
                .unwrap()
                .assume_offset(UtcOffset::from_whole_seconds(offset).unwrap())
        };
        vec![
            at(2026, Month::March, 7, (4, 5, 9), 123_456_789, 0),
            at(2026, Month::October, 20, (14, 35, 59), 999_999_999, 0),
            at(2000, Month::January, 1, (0, 0, 0), 0, 0),
            at(2009, Month::December, 31, (12, 30, 1), 5_000_000, 0),
            at(2026, Month::October, 21, (1, 15, 0), 0, 19_800),
        ]
    }

    fn time_v2_output(legacy: &str, value: OffsetDateTime) -> String {
        let description = time::format_description::parse_owned::<2>(legacy).unwrap();
        value
            .to_offset(UtcOffset::UTC)
            .format(&description)
            .unwrap()
    }

    #[test]
    fn plain_and_quoted_patterns_are_not_legacy() {
        for text in [
            "HH:mm",
            "dd.MM.yyyy",
            "'[day]'",
            "'[hour]:[minute]'",
            "[day",
            "[days]",
            "[unknown]",
            "[]",
            "",
            "day",
        ] {
            assert_eq!(
                translate_legacy_pattern(text),
                LegacyTranslation::NotLegacy,
                "text {text:?}"
            );
        }
    }

    #[test]
    fn translated_pattern_renders_like_time_v2_in_utc_english() {
        for legacy in [
            "[year]-[month]-[day] [hour]:[minute]:[second]",
            "[day padding:none].[month padding:none].[year padding:none]",
            "[day padding:zero]/[month padding:zero]/[year]",
            "[hour repr:12]:[minute] [period]",
            "[hour repr:12 padding:none] [period case:upper]",
            "[hour repr:24 padding:none]:[minute padding:none]:[second padding:none]",
            "[month repr:long] [day], [year]",
            "[month repr:short] [day padding:none]",
            "[month repr:short case_sensitive:false]",
            "[month repr:numerical]",
            "[weekday], [day] [month repr:long]",
            "[weekday repr:short]",
            "[weekday repr:long one_indexed:false case_sensitive:true]",
            "[year repr:last_two]",
            "[year repr:full base:calendar sign:automatic]",
            "[hour]:[minute]:[second].[subsecond digits:3]",
            "[subsecond digits:1] [subsecond digits:9]",
            "[offset_hour sign:mandatory]:[offset_minute]",
            "[offset_hour sign:mandatory padding:zero]:[offset_minute padding:zero]",
            "at [hour]h [minute]m",
            "it's [hour]",
            "\\[[hour]\\] \\\\",
            "[hour]'[minute]",
            "[hour][minute]",
            "[day] числа [month repr:long]",
        ] {
            let LegacyTranslation::Translated(pattern) = translate_legacy_pattern(legacy) else {
                panic!(
                    "{legacy:?} was not translated: {:?}",
                    translate_legacy_pattern(legacy)
                );
            };
            let parsed = DatePattern::parse(&pattern).unwrap();
            for value in sample_instants() {
                assert_eq!(
                    parsed.format(value, PatternZone::Utc, NamesLanguage::En),
                    time_v2_output(legacy, value),
                    "{legacy:?} -> {pattern:?} at {value}"
                );
            }
        }
    }

    #[test]
    fn untranslatable_component_is_reported_with_its_bracketed_text() {
        for (legacy, component) in [
            ("[day padding:space]", "[day padding:space]"),
            ("[hour padding:space]:[minute]", "[hour padding:space]"),
            ("[subsecond]", "[subsecond]"),
            ("[subsecond digits:1+]", "[subsecond digits:1+]"),
            ("[hour]:[minute] [period case:lower]", "[period case:lower]"),
            ("[unix_timestamp]", "[unix_timestamp]"),
            ("[year][optional [[month]]]", "[optional [[month]]]"),
            ("[offset_hour]", "[offset_hour]"),
            (
                "[offset_hour sign:mandatory]",
                "[offset_hour sign:mandatory]",
            ),
            (
                "[offset_hour sign:mandatory][offset_minute]",
                "[offset_hour sign:mandatory]",
            ),
            ("[weekday repr:monday]", "[weekday repr:monday]"),
            (
                "[year repr:last_two padding:none]",
                "[year repr:last_two padding:none]",
            ),
            ("[year sign:mandatory]", "[year sign:mandatory]"),
            ("[ordinal]", "[ordinal]"),
            ("[week_number]", "[week_number]"),
            ("[hour][hour]", "[hour][hour]"),
            ("[hour]]", "]"),
            ("[hour] [day", "[day"),
            ("\\x[day]", "\\x[day]"),
        ] {
            assert_eq!(
                translate_legacy_pattern(legacy),
                LegacyTranslation::Untranslatable {
                    component: component.to_owned()
                },
                "legacy {legacy:?}"
            );
        }
    }
}
