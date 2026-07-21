use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use fluent::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use time::OffsetDateTime;

thread_local! {
    static BUNDLE: RefCell<Option<Rc<FluentBundle<FluentResource>>>> = const { RefCell::new(None) };

    static WARNED_KEYS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());

    static LOCALE_ID: RefCell<&'static str> = const { RefCell::new("en") };
}

pub fn install_bundle(bundle: Rc<FluentBundle<FluentResource>>) {
    BUNDLE.with(|cell| {
        *cell.borrow_mut() = Some(bundle);
    });
    WARNED_KEYS.with(|cell| {
        cell.borrow_mut().clear();
    });
}

pub fn set_locale_id(id: &'static str) {
    LOCALE_ID.with(|cell| {
        *cell.borrow_mut() = id;
    });
}

fn active_locale() -> &'static str {
    LOCALE_ID.with(|cell| *cell.borrow())
}

pub fn fmt_feed_time(ts: &OffsetDateTime) -> String {
    let pattern = tr_lookup("fmt_feed_time_pattern", None);
    if pattern == "fmt_feed_time_pattern" {
        return format!(
            "{:02}:{:02}:{:02}.{:03}",
            ts.hour(),
            ts.minute(),
            ts.second(),
            ts.millisecond()
        );
    }
    // Pattern carries %HH%/%MM%/%SS%/%mmm% literal tokens, not Fluent placeables.
    pattern
        .replace("%HH%", &format!("{:02}", ts.hour()))
        .replace("%MM%", &format!("{:02}", ts.minute()))
        .replace("%SS%", &format!("{:02}", ts.second()))
        .replace("%mmm%", &format!("{:03}", ts.millisecond()))
}

pub fn fmt_short_date(ts: &OffsetDateTime) -> String {
    let locale = active_locale();
    let month_key = format!("fmt_month_abbr_{:02}", ts.month() as u8);
    let month = tr_lookup(&month_key, None);
    match locale {
        "uk" => format!("{} {} {}", ts.day(), month, ts.year()),
        _ => format!("{} {}, {}", month, ts.day(), ts.year()),
    }
}

pub fn fmt_number(value: f64, decimal_places: usize) -> String {
    let locale = active_locale();
    let (group_sep, decimal_sep) = match locale {
        "uk" => (" ", ","),
        _ => (",", "."),
    };
    let scaled =
        (value * 10f64.powi(decimal_places as i32)).round() / 10f64.powi(decimal_places as i32);
    let int_part = scaled.abs().floor() as u64;
    let int_str = group_integer(int_part, group_sep);
    let signed = if value < 0.0 { "-" } else { "" };
    if decimal_places == 0 {
        format!("{signed}{int_str}")
    } else {
        let frac = (scaled.abs().fract() * 10f64.powi(decimal_places as i32)).round() as u64;
        format!(
            "{signed}{int_str}{decimal_sep}{frac:0>width$}",
            width = decimal_places
        )
    }
}

pub fn fmt_relative_time(opt: Option<OffsetDateTime>) -> String {
    let Some(dt) = opt else {
        return tr_lookup("fmt_relative_never", None);
    };
    let delta = OffsetDateTime::now_utc() - dt;
    let secs = delta.whole_seconds().max(0) as u64;

    if secs < 60 {
        let args = ArgsBuilder::new().set("count", secs as i64).build();
        tr_lookup("fmt_relative_seconds", Some(&args))
    } else if secs < 3_600 {
        let mins = secs / 60;
        let args = ArgsBuilder::new().set("count", mins as i64).build();
        tr_lookup("fmt_relative_minutes", Some(&args))
    } else if secs < 86_400 {
        let hours = secs / 3_600;
        let args = ArgsBuilder::new().set("count", hours as i64).build();
        tr_lookup("fmt_relative_hours", Some(&args))
    } else if secs < 7 * 86_400 {
        let days = secs / 86_400;
        let args = ArgsBuilder::new().set("count", days as i64).build();
        tr_lookup("fmt_relative_days", Some(&args))
    } else {
        fmt_short_date(&dt)
    }
}

pub fn fmt_bytes(bytes: u64) -> String {
    const KIB: u64 = 1_024;
    const MIB: u64 = 1_048_576;
    const GIB: u64 = 1_073_741_824;
    if bytes < KIB {
        format!("{} B", fmt_number(bytes as f64, 0))
    } else if bytes < MIB {
        format!("{} KB", fmt_number(bytes as f64 / KIB as f64, 1))
    } else if bytes < GIB {
        format!("{} MB", fmt_number(bytes as f64 / MIB as f64, 1))
    } else {
        format!("{} GB", fmt_number(bytes as f64 / GIB as f64, 2))
    }
}

pub fn fmt_clock(secs: u64) -> String {
    let hours = secs / 3_600;
    let minutes = (secs % 3_600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

pub fn fmt_uptime(secs: u64) -> String {
    let hours = secs / 3_600;
    let minutes = (secs % 3_600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

pub fn fmt_uptime_short(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3_600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3_600)
    }
}

fn group_integer(n: u64, sep: &str) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        let pos = s.len() - i;
        if i > 0 && pos.is_multiple_of(3) {
            out.push_str(sep);
        }
        out.push(ch);
    }
    out
}

// Missing key → returns the raw key string. Dotted keys map to Fluent underscore IDs.
pub fn tr_lookup(key: &str, args: Option<&FluentArgs<'_>>) -> String {
    let fluent_id: std::borrow::Cow<'_, str> = if key.contains('.') {
        std::borrow::Cow::Owned(key.replace('.', "_"))
    } else {
        std::borrow::Cow::Borrowed(key)
    };

    BUNDLE.with(|cell| {
        let borrow = cell.borrow();
        let Some(bundle) = borrow.as_ref() else {
            warn_missing_once(key);
            return key.to_owned();
        };

        let Some(msg) = bundle.get_message(fluent_id.as_ref()) else {
            warn_missing_once(key);
            return key.to_owned();
        };

        let Some(pattern) = msg.value() else {
            warn_missing_once(key);
            return key.to_owned();
        };

        let mut errors = vec![];
        let formatted = bundle.format_pattern(pattern, args, &mut errors);
        if !errors.is_empty() {
            #[cfg(debug_assertions)]
            tracing::warn!(key, "fluent format errors: {:?}", errors);
        }
        formatted.into_owned()
    })
}

#[allow(dead_code)]
fn warn_missing_once(key: &str) {
    #[cfg(not(debug_assertions))]
    let _ = key;
    #[cfg(debug_assertions)]
    {
        WARNED_KEYS.with(|cell| {
            let mut warned = cell.borrow_mut();
            if !warned.contains(key) {
                warned.insert(key.to_owned());
                tracing::warn!(key, "tr!: key not found in active bundle");
            }
        });
    }
}

pub struct ArgsBuilder<'a>(FluentArgs<'a>);

impl<'a> Default for ArgsBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> ArgsBuilder<'a> {
    pub fn new() -> Self {
        Self(FluentArgs::new())
    }

    pub fn set(mut self, key: &'a str, value: impl Into<FluentValue<'a>>) -> Self {
        self.0.set(key, value);
        self
    }

    pub fn build(self) -> FluentArgs<'a> {
        self.0
    }
}

#[macro_export]
macro_rules! tr {
    ($key:expr) => {
        $crate::locale::tr_lookup($key, None)
    };
    ($key:expr, $($name:ident = $val:expr),+ $(,)?) => {{
        let args = $crate::locale::ArgsBuilder::new()
            $(.set(stringify!($name), $val))+
            .build();
        $crate::locale::tr_lookup($key, Some(&args))
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Date, Month, Time};

    // These tests deliberately never call `install_bundle`, so the thread-local
    // BUNDLE stays `None` on every worker thread. That keeps `tr_lookup` on its
    // documented miss path (returns the key verbatim) and makes the formatters
    // that delegate to it deterministic regardless of test execution order.

    fn utc(year: i32, month: Month, day: u8, h: u8, m: u8, s: u8, milli: u16) -> OffsetDateTime {
        let date = Date::from_calendar_date(year, month, day).unwrap_or(Date::MIN);
        let time = Time::from_hms_milli(h, m, s, milli).unwrap_or(Time::MIDNIGHT);
        OffsetDateTime::new_utc(date, time)
    }

    #[test]
    fn tr_lookup_returns_key_verbatim_when_no_bundle_installed() {
        // Miss path returns the ORIGINAL key, not the dotted->underscore
        // Fluent id it would have looked up. The dotted case pins that.
        for key in ["some_missing_key", "nav.home", "a.b.c", "plain_key"] {
            assert_eq!(tr_lookup(key, None), key);
        }
    }

    #[test]
    fn fmt_number_groups_and_separates_per_locale() {
        // (locale, value, decimal_places, expected)
        let cases = [
            ("en", 0.0, 0, "0"),
            ("en", 999.0, 0, "999"),
            ("en", 1000.0, 0, "1,000"),
            ("en", 1_234_567.0, 0, "1,234,567"),
            ("en", 1234.5, 2, "1,234.50"),
            ("en", -1234.5, 1, "-1,234.5"),
            ("uk", 1_234_567.0, 0, "1 234 567"),
            ("uk", 1234.5, 1, "1 234,5"),
        ];
        for (locale, value, dp, expected) in cases {
            set_locale_id(locale);
            assert_eq!(
                fmt_number(value, dp),
                expected,
                "fmt_number({value}, {dp}) in {locale}"
            );
        }
    }

    #[test]
    fn fmt_short_date_orders_components_per_locale() {
        // Why: with no bundle the month resolves to its raw key for BOTH
        // locales, so the only difference under test is the locale-specific
        // component ORDER and punctuation - which is the branching logic here.
        let date = utc(2026, Month::March, 15, 0, 0, 0, 0);

        set_locale_id("en");
        assert_eq!(fmt_short_date(&date), "fmt_month_abbr_03 15, 2026");

        set_locale_id("uk");
        assert_eq!(fmt_short_date(&date), "15 fmt_month_abbr_03 2026");
    }

    #[test]
    fn fmt_feed_time_falls_back_to_zero_padded_pattern_without_bundle() {
        // No bundle -> pattern key is unresolved -> hardcoded HH:MM:SS.mmm
        // fallback. Single-digit components must be zero-padded.
        let ts = utc(2026, Month::March, 15, 4, 5, 9, 7);
        assert_eq!(fmt_feed_time(&ts), "04:05:09.007");
    }

    #[test]
    fn fmt_relative_time_none_maps_to_the_never_key() {
        // The `None` input short-circuits before any wall-clock subtraction.
        assert_eq!(fmt_relative_time(None), "fmt_relative_never");
    }
}
