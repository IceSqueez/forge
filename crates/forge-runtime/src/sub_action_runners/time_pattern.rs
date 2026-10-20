use forge_registry::{STEP_CONFIG_KEY, STEP_KIND_KEY, SubActionConfigExt};
use forge_types::{
    DatePattern, LegacyTranslation, NamesLanguage, ParsedPattern, PatternZone, SubActionConfig,
    SubActionStep, Variant, current_names_language, shared_date_pattern, translate_legacy_pattern,
};

pub(crate) const TIME_FORMAT_KIND_ID: &str = "core.time.format";
pub(crate) const TIME_PARSE_KIND_ID: &str = "core.time.parse";
pub(crate) const TIME_NOW_KIND_ID: &str = "core.time.now";
pub(crate) const FORMAT_PATTERN_KEY: &str = "format_string";
pub(crate) const PARSE_PATTERN_KEY: &str = "format";
pub(crate) const CUSTOM_NOW_PATTERN_KEY: &str = "custom_format_string";
pub(crate) const UTC_KEY: &str = "utc";
pub(crate) const DEFAULT_TIME_PATTERN: &str = "yyyy-MM-dd HH:mm:ss";

pub(crate) struct ResolvedTimePattern {
    text: String,
    parsed: ParsedPattern,
    pub(crate) zone: PatternZone,
    pub(crate) language: NamesLanguage,
}

impl ResolvedTimePattern {
    pub(crate) fn pattern(&self) -> Result<&DatePattern, String> {
        self.parsed
            .as_ref()
            .as_ref()
            .map_err(|error| format!("invalid pattern '{}': {error}", self.text))
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }
}

pub(crate) fn resolve_time_pattern(
    config: &SubActionConfig,
    pattern_key: &str,
) -> Result<ResolvedTimePattern, String> {
    let stored = config.str(pattern_key).unwrap_or(DEFAULT_TIME_PATTERN);
    let (text, utc_when_unset, language) = match translate_legacy_pattern(stored) {
        LegacyTranslation::NotLegacy => (stored.to_owned(), false, current_names_language()),
        LegacyTranslation::Translated(pattern) => (pattern, true, NamesLanguage::En),
        LegacyTranslation::Untranslatable { component } => {
            return Err(format!(
                "the old pattern part {component} has no equivalent in date patterns; \
                 rewrite the pattern with letters such as HH:mm dd.MM.yyyy"
            ));
        }
    };
    let zone = if config.bool(UTC_KEY).unwrap_or(utc_when_unset) {
        PatternZone::Utc
    } else {
        PatternZone::Local
    };
    let parsed = shared_date_pattern(&text);
    Ok(ResolvedTimePattern {
        text,
        parsed,
        zone,
        language,
    })
}

pub fn translate_legacy_time_steps(steps: &mut [SubActionStep]) -> bool {
    let mut changed = false;
    for step in steps {
        changed |= translate_legacy_time_config(&step.kind_id, &mut step.config);
        for value in step.config.values_mut() {
            changed |= translate_nested_steps(value);
        }
    }
    changed
}

fn translate_nested_steps(value: &mut Variant) -> bool {
    match value {
        Variant::Array(items) => items.iter_mut().fold(false, |changed, item| {
            translate_nested_steps(item) | changed
        }),
        Variant::Object(map) => {
            let mut changed = false;
            let kind_id = map
                .get(STEP_KIND_KEY)
                .and_then(Variant::as_str)
                .map(str::to_owned);
            if let (Some(kind_id), Some(Variant::Object(config))) =
                (kind_id, map.get_mut(STEP_CONFIG_KEY))
            {
                changed |= translate_legacy_time_config(&kind_id, config);
            }
            for nested in map.values_mut() {
                changed |= translate_nested_steps(nested);
            }
            changed
        }
        _ => false,
    }
}

fn translate_legacy_time_config(kind_id: &str, config: &mut SubActionConfig) -> bool {
    let pattern_key = match kind_id {
        TIME_FORMAT_KIND_ID => FORMAT_PATTERN_KEY,
        TIME_PARSE_KIND_ID => PARSE_PATTERN_KEY,
        TIME_NOW_KIND_ID => CUSTOM_NOW_PATTERN_KEY,
        _ => return false,
    };
    let Some(LegacyTranslation::Translated(pattern)) =
        config.str(pattern_key).map(translate_legacy_pattern)
    else {
        return false;
    };
    config.insert(pattern_key.to_owned(), Variant::String(pattern));
    config
        .entry(UTC_KEY.to_owned())
        .or_insert(Variant::Bool(true));
    true
}
