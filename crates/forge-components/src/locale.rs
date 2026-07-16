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
