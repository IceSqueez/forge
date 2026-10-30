use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use fluent::memoizer::MemoizerKind;
use fluent::{FluentArgs, FluentBundle, FluentError, FluentResource, FluentValue};
use forge_types::NamesLanguage;
use unic_langid::LanguageIdentifier;

const EN_CATALOG: &str = include_str!("../locales/en/main.ftl");
const UK_CATALOG: &str = include_str!("../locales/uk/main.ftl");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogLanguage {
    En,
    Uk,
}

impl CatalogLanguage {
    pub fn names_language(self) -> NamesLanguage {
        match self {
            Self::En => NamesLanguage::En,
            Self::Uk => NamesLanguage::Uk,
        }
    }

    fn bundle_tag(self) -> &'static str {
        match self {
            Self::En => "en-US",
            Self::Uk => "uk-UA",
        }
    }

    fn language_identifier(self) -> LanguageIdentifier {
        self.bundle_tag().parse().unwrap_or_default()
    }
}

pub fn catalog_source(language: CatalogLanguage) -> &'static str {
    match language {
        CatalogLanguage::En => EN_CATALOG,
        CatalogLanguage::Uk => UK_CATALOG,
    }
}

type ThreadBundle = FluentBundle<FluentResource>;
type ProcessBundle = fluent::concurrent::FluentBundle<FluentResource>;

struct ProcessCatalog {
    bundle: ProcessBundle,
    language: CatalogLanguage,
}

static PROCESS_CATALOG: RwLock<Option<Arc<ProcessCatalog>>> = RwLock::new(None);

static PROCESS_WARNED_KEYS: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

thread_local! {
    static BUNDLE: RefCell<Option<Rc<ThreadBundle>>> = const { RefCell::new(None) };

    static WARNED_KEYS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());

    static THREAD_LANGUAGE: Cell<Option<CatalogLanguage>> = const { Cell::new(None) };
}

enum ActiveBundle {
    Thread(Rc<ThreadBundle>),
    Process(Arc<ProcessCatalog>),
}

impl ActiveBundle {
    fn format(&self, key: &str, args: Option<&FluentArgs<'_>>) -> Option<String> {
        match self {
            Self::Thread(bundle) => format_message(bundle, key, args),
            Self::Process(catalog) => format_message(&catalog.bundle, key, args),
        }
    }

    fn warn_missing_once(&self, key: &str) {
        match self {
            Self::Thread(_) => warn_missing_once_on_thread(key),
            Self::Process(_) => warn_missing_once_in_process(key),
        }
    }
}

fn process_catalog() -> Option<Arc<ProcessCatalog>> {
    PROCESS_CATALOG
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

fn active_bundle() -> Option<ActiveBundle> {
    BUNDLE
        .with(|cell| cell.borrow().clone())
        .map(ActiveBundle::Thread)
        .or_else(|| process_catalog().map(ActiveBundle::Process))
}

fn assemble_bundle<M: MemoizerKind>(
    mut bundle: fluent::bundle::FluentBundle<FluentResource, M>,
    language: CatalogLanguage,
    app_sources: &[&str],
) -> (fluent::bundle::FluentBundle<FluentResource, M>, Vec<String>) {
    if let Err(error) = bundle.add_function(NUMBER_FUNCTION, move |positional, named| {
        number_function(language, positional, named)
    }) {
        tracing::warn!(?error, ?language, "FTL number function unavailable");
    }
    let mut overridden = Vec::new();
    for source in app_sources
        .iter()
        .copied()
        .chain([catalog_source(language)])
    {
        let resource =
            FluentResource::try_new(source.to_owned()).unwrap_or_else(|(resource, errors)| {
                tracing::warn!(?errors, ?language, "FTL parse errors in catalog");
                resource
            });
        if let Err(errors) = bundle.add_resource(resource) {
            for error in errors {
                match error {
                    FluentError::Overriding { id, .. } => overridden.push(id),
                    other => tracing::warn!(error = ?other, ?language, "FTL resource add error"),
                }
            }
        }
    }
    (bundle, overridden)
}

pub fn duplicate_message_ids(language: CatalogLanguage, app_sources: &[&str]) -> Vec<String> {
    assemble_bundle(
        ThreadBundle::new(vec![language.language_identifier()]),
        language,
        app_sources,
    )
    .1
}

pub fn install_process_language(language: CatalogLanguage, app_sources: &[&str]) {
    let (bundle, _) = assemble_bundle(
        ProcessBundle::new_concurrent(vec![language.language_identifier()]),
        language,
        app_sources,
    );
    let catalog = Arc::new(ProcessCatalog { bundle, language });
    *PROCESS_CATALOG
        .write()
        .unwrap_or_else(PoisonError::into_inner) = Some(catalog);
    PROCESS_WARNED_KEYS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
}

pub fn install_thread_language(language: CatalogLanguage, app_sources: &[&str]) {
    let (bundle, _) = assemble_bundle(
        ThreadBundle::new(vec![language.language_identifier()]),
        language,
        app_sources,
    );
    install_bundle(Rc::new(bundle));
    THREAD_LANGUAGE.set(Some(language));
}

pub fn resolve_in_language(language: CatalogLanguage, app_sources: &[&str], key: &str) -> String {
    let (bundle, _) = assemble_bundle(
        ThreadBundle::new(vec![language.language_identifier()]),
        language,
        app_sources,
    );
    format_message(&bundle, key, None).unwrap_or_else(|| key.to_owned())
}

pub fn install_bundle(bundle: Rc<ThreadBundle>) {
    BUNDLE.with(|cell| {
        *cell.borrow_mut() = Some(bundle);
    });
    WARNED_KEYS.with(|cell| {
        cell.borrow_mut().clear();
    });
}

pub fn set_thread_language(language: CatalogLanguage) {
    THREAD_LANGUAGE.set(Some(language));
}

pub(crate) fn active_language() -> CatalogLanguage {
    THREAD_LANGUAGE
        .get()
        .or_else(|| process_catalog().map(|catalog| catalog.language))
        .unwrap_or(CatalogLanguage::En)
}

const FTL_ID_FILLER: char = '_';
const NUMBER_FUNCTION: &str = "NUMBER";
const DEFAULT_MAXIMUM_FRACTION_DIGITS: usize = 3;
const MAXIMUM_DECIMAL_PLACES: usize = 15;

struct NumberSeparators {
    group: &'static str,
    decimal: &'static str,
}

fn separators(language: CatalogLanguage) -> NumberSeparators {
    match language {
        CatalogLanguage::Uk => NumberSeparators {
            group: " ",
            decimal: ",",
        },
        CatalogLanguage::En => NumberSeparators {
            group: ",",
            decimal: ".",
        },
    }
}

fn split_fixed(value: f64, decimal_places: usize, language: CatalogLanguage) -> (String, String) {
    let separators = separators(language);
    let decimal_places = decimal_places.min(MAXIMUM_DECIMAL_PLACES);
    let factor = (0..decimal_places).fold(1.0_f64, |factor, _| factor * 10.0);
    let scaled = (value * factor).round() / factor;
    let int_part = scaled.abs().floor() as u64;
    let signed = if scaled < 0.0 { "-" } else { "" };
    let int_str = format!("{signed}{}", group_integer(int_part, separators.group));
    let frac_str = if decimal_places == 0 {
        String::new()
    } else {
        let frac = (scaled.abs().fract() * factor).round() as u64;
        format!("{frac:0>decimal_places$}")
    };
    (int_str, frac_str)
}

fn join_decimal(int_str: String, frac_str: &str, language: CatalogLanguage) -> String {
    if frac_str.is_empty() {
        int_str
    } else {
        format!("{int_str}{}{frac_str}", separators(language).decimal)
    }
}

fn format_with_fraction_range(
    value: f64,
    minimum_fraction_digits: usize,
    maximum_fraction_digits: usize,
    language: CatalogLanguage,
) -> String {
    let maximum = maximum_fraction_digits.max(minimum_fraction_digits);
    let (int_str, frac_str) = split_fixed(value, maximum, language);
    let mut kept = frac_str.len();
    while kept > minimum_fraction_digits && frac_str.as_bytes()[kept - 1] == b'0' {
        kept -= 1;
    }
    join_decimal(int_str, &frac_str[..kept], language)
}

fn number_function<'a>(
    language: CatalogLanguage,
    positional: &[FluentValue<'a>],
    named: &FluentArgs<'_>,
) -> FluentValue<'a> {
    let Some(FluentValue::Number(number)) = positional.first() else {
        return FluentValue::Error;
    };
    let mut options = number.options.clone();
    options.merge(named);
    let minimum = options.minimum_fraction_digits.unwrap_or(0);
    let maximum = options
        .maximum_fraction_digits
        .unwrap_or(DEFAULT_MAXIMUM_FRACTION_DIGITS);
    FluentValue::String(format_with_fraction_range(number.value, minimum, maximum, language).into())
}

pub fn fmt_number(value: f64, decimal_places: usize) -> String {
    let language = active_language();
    let (int_str, frac_str) = split_fixed(value, decimal_places, language);
    join_decimal(int_str, &frac_str, language)
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
        crate::tr!("fmt_bytes_b", value = fmt_number(bytes as f64, 0))
    } else if bytes < MIB {
        crate::tr!(
            "fmt_bytes_kib",
            value = fmt_number(bytes as f64 / KIB as f64, 1)
        )
    } else if bytes < GIB {
        crate::tr!(
            "fmt_bytes_mib",
            value = fmt_number(bytes as f64 / MIB as f64, 1)
        )
    } else {
        crate::tr!(
            "fmt_bytes_gib",
            value = fmt_number(bytes as f64 / GIB as f64, 2)
        )
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
        uptime_copy(
            "fmt_uptime_hours_minutes",
            &[("hours", hours), ("minutes", minutes)],
        )
    } else if minutes > 0 {
        uptime_copy(
            "fmt_uptime_minutes_seconds",
            &[("minutes", minutes), ("seconds", seconds)],
        )
    } else {
        uptime_copy("fmt_uptime_seconds", &[("seconds", seconds)])
    }
}

pub fn fmt_uptime_short(secs: u64) -> String {
    if secs < 60 {
        uptime_copy("fmt_uptime_seconds", &[("seconds", secs)])
    } else if secs < 3_600 {
        uptime_copy("fmt_uptime_minutes", &[("minutes", secs / 60)])
    } else {
        uptime_copy("fmt_uptime_hours", &[("hours", secs / 3_600)])
    }
}

fn uptime_copy(key: &str, parts: &[(&'static str, u64)]) -> String {
    let args = parts
        .iter()
        .fold(ArgsBuilder::new(), |args, (name, value)| {
            args.set(name, i64::try_from(*value).unwrap_or(i64::MAX))
        })
        .build();
    tr_lookup(key, Some(&args))
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

pub fn ftl_id(raw: &str) -> Cow<'_, str> {
    if raw.chars().all(is_ftl_id_char) {
        Cow::Borrowed(raw)
    } else {
        Cow::Owned(
            raw.chars()
                .map(|ch| {
                    if is_ftl_id_char(ch) {
                        ch
                    } else {
                        FTL_ID_FILLER
                    }
                })
                .collect(),
        )
    }
}

fn is_ftl_id_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == FTL_ID_FILLER
}

pub fn tr_lookup(key: &str, args: Option<&FluentArgs<'_>>) -> String {
    tr_find_warned(key, args).unwrap_or_else(|| key.to_owned())
}

pub fn tr_find_warned(key: &str, args: Option<&FluentArgs<'_>>) -> Option<String> {
    let Some(active) = active_bundle() else {
        warn_unlocalized_once(key);
        return None;
    };
    let found = active.format(key, args);
    if found.is_none() {
        active.warn_missing_once(key);
    }
    found
}

fn warn_unlocalized_once(key: &str) {
    WARNED_KEYS.with(|cell| {
        if cell.borrow_mut().insert(key.to_owned()) {
            let thread = std::thread::current();
            tracing::warn!(
                key,
                thread = thread.name().unwrap_or("unnamed"),
                "tr!: no locale bundle installed for this process or thread"
            );
        }
    });
}

pub fn tr_find(key: &str, args: Option<&FluentArgs<'_>>) -> Option<String> {
    active_bundle()?.format(key, args)
}

fn format_message<M: MemoizerKind>(
    bundle: &fluent::bundle::FluentBundle<FluentResource, M>,
    key: &str,
    args: Option<&FluentArgs<'_>>,
) -> Option<String> {
    let pattern = bundle.get_message(key)?.value()?;
    let mut errors = vec![];
    let formatted = bundle.format_pattern(pattern, args, &mut errors);
    if !errors.is_empty() {
        #[cfg(debug_assertions)]
        tracing::warn!(key, "fluent format errors: {:?}", errors);
    }
    Some(formatted.into_owned())
}

fn warn_missing_once_on_thread(key: &str) {
    #[cfg(not(debug_assertions))]
    let _ = key;
    #[cfg(debug_assertions)]
    {
        WARNED_KEYS.with(|cell| {
            if cell.borrow_mut().insert(key.to_owned()) {
                tracing::warn!(key, "tr!: key not found in active bundle");
            }
        });
    }
}

fn warn_missing_once_in_process(key: &str) {
    #[cfg(not(debug_assertions))]
    let _ = key;
    #[cfg(debug_assertions)]
    {
        let first_miss = PROCESS_WARNED_KEYS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.to_owned());
        if first_miss {
            tracing::warn!(key, "tr!: key not found in active bundle");
        }
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
    fn tr_find_matches_verbatim_so_a_dotted_id_resolves_only_through_ftl_id() {
        install_source("nav_home = Home\n");
        assert_eq!(
            (
                tr_find("nav.home", None),
                tr_find(&ftl_id("nav.home"), None).as_deref()
            ),
            (None, Some("Home"))
        );
    }

    #[test]
    fn ftl_id_replaces_each_non_ascii_identifier_char_with_one_filler() {
        for (raw, expected) in [
            ("twitch.chat.send_message", "twitch_chat_send_message"),
            ("slide-up", "slide_up"),
            ("Lines array", "Lines_array"),
            ("a.b-c d", "a_b_c_d"),
            ("файл", "____"),
            ("easeIn2", "easeIn2"),
            ("", ""),
        ] {
            assert_eq!(ftl_id(raw), expected, "{raw:?}");
        }
    }

    #[test]
    fn ftl_id_borrows_an_id_that_is_already_valid() {
        assert!(matches!(ftl_id("nav_home"), Cow::Borrowed("nav_home")));
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
            (CatalogLanguage::En, 0.0, 0, "0"),
            (CatalogLanguage::En, 999.0, 0, "999"),
            (CatalogLanguage::En, 1000.0, 0, "1,000"),
            (CatalogLanguage::En, 1_234_567.0, 0, "1,234,567"),
            (CatalogLanguage::En, 1234.5, 2, "1,234.50"),
            (CatalogLanguage::En, -1234.5, 1, "-1,234.5"),
            (CatalogLanguage::Uk, 1_234_567.0, 0, "1 234 567"),
            (CatalogLanguage::Uk, 1234.5, 1, "1 234,5"),
        ];
        for (language, value, dp, expected) in cases {
            set_thread_language(language);
            assert_eq!(
                fmt_number(value, dp),
                expected,
                "fmt_number({value}, {dp}) in {language:?}"
            );
        }
    }

    #[test]
    fn fmt_number_never_prints_a_negative_zero() {
        for (language, value, dp, expected) in [
            (CatalogLanguage::En, -0.0, 0, "0"),
            (CatalogLanguage::En, -0.4, 0, "0"),
            (CatalogLanguage::En, -0.04, 1, "0.0"),
            (CatalogLanguage::En, -0.004, 2, "0.00"),
            (CatalogLanguage::Uk, -0.04, 1, "0,0"),
        ] {
            set_thread_language(language);
            assert_eq!(fmt_number(value, dp), expected, "{value} at {dp}");
        }
    }

    #[test]
    fn fmt_number_keeps_the_sign_of_a_negative_that_rounds_away_from_zero() {
        set_thread_language(CatalogLanguage::En);
        for (value, dp, expected) in [(-0.5, 0, "-1"), (-0.06, 1, "-0.1"), (-0.006, 2, "-0.01")] {
            assert_eq!(fmt_number(value, dp), expected, "{value} at {dp}");
        }
    }

    #[test]
    fn fmt_number_caps_requested_decimal_places_at_fifteen() {
        set_thread_language(CatalogLanguage::En);
        for dp in [15, 16, 400] {
            assert_eq!(fmt_number(0.5, dp), "0.500000000000000", "{dp} places");
        }
    }

    #[test]
    fn fmt_bytes_switches_unit_exactly_at_each_binary_boundary() {
        let cases = [
            (CatalogLanguage::En, 0, "0 B"),
            (CatalogLanguage::En, 1_023, "1,023 B"),
            (CatalogLanguage::En, 1_024, "1.0 KiB"),
            (CatalogLanguage::En, 1_536, "1.5 KiB"),
            (CatalogLanguage::En, 1_048_575, "1,024.0 KiB"),
            (CatalogLanguage::En, 1_048_576, "1.0 MiB"),
            (CatalogLanguage::En, 1_073_741_823, "1,024.0 MiB"),
            (CatalogLanguage::En, 1_073_741_824, "1.00 GiB"),
            (CatalogLanguage::En, u64::MAX, "17,179,869,184.00 GiB"),
            (CatalogLanguage::Uk, 0, "0 Б"),
            (CatalogLanguage::Uk, 1_023, "1 023 Б"),
            (CatalogLanguage::Uk, 1_536, "1,5 КіБ"),
            (CatalogLanguage::Uk, 1_048_576, "1,0 МіБ"),
            (CatalogLanguage::Uk, 1_073_741_824, "1,00 ГіБ"),
        ];
        for (language, bytes, expected) in cases {
            install_thread_language(language, &[]);
            assert_eq!(
                unmarked(fmt_bytes(bytes)),
                expected,
                "fmt_bytes({bytes}) in {language:?}"
            );
        }
    }

    fn unmarked(text: String) -> String {
        text.replace(['\u{2068}', '\u{2069}'], "")
    }

    #[test]
    fn fmt_uptime_switches_shape_at_the_minute_and_hour_boundaries_in_each_language() {
        for (language, secs, expected) in [
            (CatalogLanguage::En, 0, "0s"),
            (CatalogLanguage::En, 59, "59s"),
            (CatalogLanguage::En, 60, "1m 0s"),
            (CatalogLanguage::En, 3_599, "59m 59s"),
            (CatalogLanguage::En, 3_600, "1h 0m"),
            (CatalogLanguage::Uk, 0, "0 с"),
            (CatalogLanguage::Uk, 59, "59 с"),
            (CatalogLanguage::Uk, 60, "1 хв 0 с"),
            (CatalogLanguage::Uk, 3_599, "59 хв 59 с"),
            (CatalogLanguage::Uk, 3_600, "1 год 0 хв"),
        ] {
            install_thread_language(language, &[]);
            assert_eq!(unmarked(fmt_uptime(secs)), expected, "{language:?} {secs}");
        }
    }

    #[test]
    fn fmt_uptime_short_keeps_only_the_largest_whole_unit_in_each_language() {
        for (language, secs, expected) in [
            (CatalogLanguage::En, 0, "0s"),
            (CatalogLanguage::En, 59, "59s"),
            (CatalogLanguage::En, 60, "1m"),
            (CatalogLanguage::En, 3_599, "59m"),
            (CatalogLanguage::En, 3_600, "1h"),
            (CatalogLanguage::Uk, 0, "0 с"),
            (CatalogLanguage::Uk, 59, "59 с"),
            (CatalogLanguage::Uk, 60, "1 хв"),
            (CatalogLanguage::Uk, 3_599, "59 хв"),
            (CatalogLanguage::Uk, 3_600, "1 год"),
        ] {
            install_thread_language(language, &[]);
            assert_eq!(
                unmarked(fmt_uptime_short(secs)),
                expected,
                "{language:?} {secs}"
            );
        }
    }

    #[derive(Default)]
    struct WarnFields {
        key: Option<String>,
        thread: Option<String>,
    }

    impl tracing::field::Visit for WarnFields {
        fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}

        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            match field.name() {
                "key" => self.key = Some(value.to_owned()),
                "thread" => self.thread = Some(value.to_owned()),
                _ => {}
            }
        }
    }

    type Warning = (Option<String>, Option<String>);

    struct WarnCapture(std::sync::Arc<std::sync::Mutex<Vec<Warning>>>);

    impl tracing::Subscriber for WarnCapture {
        fn register_callsite(
            &self,
            _: &'static tracing::Metadata<'static>,
        ) -> tracing::subscriber::Interest {
            tracing::subscriber::Interest::sometimes()
        }

        fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
            *metadata.level() == tracing::Level::WARN
        }

        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }

        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

        fn event(&self, event: &tracing::Event<'_>) {
            let mut fields = WarnFields::default();
            event.record(&mut fields);
            self.0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((fields.key, fields.thread));
        }

        fn enter(&self, _: &tracing::span::Id) {}

        fn exit(&self, _: &tracing::span::Id) {}
    }

    fn warnings_on_a_bundleless_thread(name: &str, keys: &'static [&'static str]) -> Vec<Warning> {
        std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
                let capture = WarnCapture(std::sync::Arc::clone(&lines));
                tracing::subscriber::with_default(capture, || {
                    tracing::callsite::rebuild_interest_cache();
                    for key in keys {
                        assert_eq!(tr_lookup(key, None), *key);
                    }
                });
                lines.lock().unwrap().clone()
            })
            .unwrap()
            .join()
            .unwrap()
    }

    fn warning(key: &str, thread: &str) -> Warning {
        (Some(key.to_owned()), Some(thread.to_owned()))
    }

    #[test]
    fn tr_lookup_without_a_bundle_warns_once_per_key_naming_the_thread() {
        let captured = warnings_on_a_bundleless_thread(
            "probe-worker",
            &[
                "unlocalized_a",
                "unlocalized_a",
                "unlocalized_b",
                "unlocalized_a",
            ],
        );
        assert_eq!(
            captured,
            vec![
                warning("unlocalized_a", "probe-worker"),
                warning("unlocalized_b", "probe-worker"),
            ]
        );
    }

    #[test]
    fn tr_lookup_without_a_bundle_warns_again_for_the_same_key_on_another_thread() {
        let first = warnings_on_a_bundleless_thread("probe-first", &["unlocalized_shared"]);
        let second = warnings_on_a_bundleless_thread("probe-second", &["unlocalized_shared"]);
        assert_eq!(
            (first, second),
            (
                vec![warning("unlocalized_shared", "probe-first")],
                vec![warning("unlocalized_shared", "probe-second")],
            )
        );
    }
}
