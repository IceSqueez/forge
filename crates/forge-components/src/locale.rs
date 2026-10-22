use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use fluent::{FluentArgs, FluentBundle, FluentResource, FluentValue};

const EN_CATALOG: &str = include_str!("../locales/en/main.ftl");
const UK_CATALOG: &str = include_str!("../locales/uk/main.ftl");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogLanguage {
    En,
    Uk,
}

pub fn catalog_source(language: CatalogLanguage) -> &'static str {
    match language {
        CatalogLanguage::En => EN_CATALOG,
        CatalogLanguage::Uk => UK_CATALOG,
    }
}

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

pub(crate) fn active_locale() -> &'static str {
    LOCALE_ID.with(|cell| *cell.borrow())
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

pub(crate) fn relative_within_week(elapsed: time::Duration) -> Option<String> {
    let secs = elapsed.whole_seconds().max(0) as u64;

    if secs < 60 {
        let args = ArgsBuilder::new().set("count", secs as i64).build();
        Some(tr_lookup("fmt_relative_seconds", Some(&args)))
    } else if secs < 3_600 {
        let mins = secs / 60;
        let args = ArgsBuilder::new().set("count", mins as i64).build();
        Some(tr_lookup("fmt_relative_minutes", Some(&args)))
    } else if secs < 86_400 {
        let hours = secs / 3_600;
        let args = ArgsBuilder::new().set("count", hours as i64).build();
        Some(tr_lookup("fmt_relative_hours", Some(&args)))
    } else if secs < 7 * 86_400 {
        let days = secs / 86_400;
        let args = ArgsBuilder::new().set("count", days as i64).build();
        Some(tr_lookup("fmt_relative_days", Some(&args)))
    } else {
        None
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

pub fn tr_lookup(key: &str, args: Option<&FluentArgs<'_>>) -> String {
    tr_find(key, args).unwrap_or_else(|| {
        warn_missing_once(key);
        key.to_owned()
    })
}

pub fn tr_find(key: &str, args: Option<&FluentArgs<'_>>) -> Option<String> {
    let fluent_id: std::borrow::Cow<'_, str> = if key.contains('.') {
        std::borrow::Cow::Owned(key.replace('.', "_"))
    } else {
        std::borrow::Cow::Borrowed(key)
    };

    BUNDLE.with(|cell| {
        let borrow = cell.borrow();
        let bundle = borrow.as_ref()?;
        let pattern = bundle.get_message(fluent_id.as_ref())?.value()?;
        let mut errors = vec![];
        let formatted = bundle.format_pattern(pattern, args, &mut errors);
        if !errors.is_empty() {
            #[cfg(debug_assertions)]
            tracing::warn!(key, "fluent format errors: {:?}", errors);
        }
        Some(formatted.into_owned())
    })
}

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

    #[allow(clippy::unwrap_used)]
    fn install_source(source: &str) {
        let mut bundle = FluentBundle::new(vec!["en-US".parse().unwrap()]);
        bundle
            .add_resource(FluentResource::try_new(source.to_owned()).unwrap())
            .unwrap();
        install_bundle(Rc::new(bundle));
    }

    fn warned(key: &str) -> bool {
        WARNED_KEYS.with(|cell| cell.borrow().contains(key))
    }

    #[test]
    fn tr_find_reads_a_present_key_including_the_dotted_form() {
        install_source("nav_home = Home\n");
        assert_eq!(tr_find("nav.home", None).as_deref(), Some("Home"));
    }

    #[test]
    fn tr_find_misses_without_recording_a_missing_key_warning() {
        install_source("present = Here\n");
        assert_eq!(tr_find("absent_key", None), None);
        assert!(!warned("absent_key"));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn tr_lookup_records_the_missing_key_warning_that_tr_find_skips() {
        install_source("present = Here\n");
        assert_eq!(tr_lookup("absent_key", None), "absent_key");
        assert!(warned("absent_key"));
    }

    #[test]
    fn tr_find_returns_none_when_no_bundle_is_installed() {
        BUNDLE.with(|cell| *cell.borrow_mut() = None);
        assert_eq!(tr_find("present", None), None);
    }

    #[test]
    fn tr_lookup_returns_key_verbatim_when_no_bundle_installed() {
        for key in ["some_missing_key", "nav.home", "a.b.c", "plain_key"] {
            assert_eq!(tr_lookup(key, None), key);
        }
    }

    #[test]
    fn fmt_number_groups_and_separates_per_locale() {
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
    fn fmt_bytes_switches_unit_exactly_at_each_binary_boundary() {
        let cases = [
            ("en", 0, "0 B"),
            ("en", 1_023, "1,023 B"),
            ("en", 1_024, "1.0 KB"),
            ("en", 1_536, "1.5 KB"),
            ("en", 1_048_576, "1.0 MB"),
            ("en", 1_073_741_824, "1.00 GB"),
            ("en", u64::MAX, "17,179,869,184.00 GB"),
            ("uk", 1_536, "1,5 KB"),
            ("uk", 1_073_741_824, "1,00 GB"),
        ];
        for (locale, bytes, expected) in cases {
            set_locale_id(locale);
            assert_eq!(fmt_bytes(bytes), expected, "fmt_bytes({bytes}) in {locale}");
        }
    }
}
