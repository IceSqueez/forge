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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use forge_registry::CASE_CHAIN_KEY;

    use super::*;

    const LEGACY: &str = "[day].[month].[year] [hour]:[minute]";
    const TRANSLATED: &str = "dd.MM.yyyy HH:mm";

    fn s(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    fn config(entries: &[(&str, Variant)]) -> SubActionConfig {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    fn step(kind_id: &str, config: SubActionConfig) -> SubActionStep {
        SubActionStep {
            kind_id: kind_id.to_owned(),
            config,
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }
    }

    fn step_object(kind_id: &str, config: SubActionConfig) -> Variant {
        Variant::Object(BTreeMap::from([
            (STEP_KIND_KEY.to_owned(), s(kind_id)),
            (STEP_CONFIG_KEY.to_owned(), Variant::Object(config)),
        ]))
    }

    fn translated_config(pattern_key: &str) -> SubActionConfig {
        config(&[(pattern_key, s(TRANSLATED)), (UTC_KEY, Variant::Bool(true))])
    }

    #[test]
    fn top_level_time_steps_get_the_translated_pattern_and_utc_on() {
        let mut steps = vec![
            step(
                TIME_FORMAT_KIND_ID,
                config(&[(FORMAT_PATTERN_KEY, s(LEGACY))]),
            ),
            step(
                TIME_PARSE_KIND_ID,
                config(&[(PARSE_PATTERN_KEY, s(LEGACY))]),
            ),
            step(
                TIME_NOW_KIND_ID,
                config(&[(CUSTOM_NOW_PATTERN_KEY, s(LEGACY))]),
            ),
        ];
        assert!(translate_legacy_time_steps(&mut steps));
        assert_eq!(steps[0].config, translated_config(FORMAT_PATTERN_KEY));
        assert_eq!(steps[1].config, translated_config(PARSE_PATTERN_KEY));
        assert_eq!(steps[2].config, translated_config(CUSTOM_NOW_PATTERN_KEY));
    }

    #[test]
    fn steps_nested_in_sub_chains_and_case_lists_are_translated() {
        let legacy_format = || config(&[(FORMAT_PATTERN_KEY, s(LEGACY))]);
        let inner_chain = Variant::Array(vec![step_object(TIME_FORMAT_KIND_ID, legacy_format())]);
        let mut steps = vec![
            step(
                "core.logic.if",
                config(&[(
                    "then",
                    Variant::Array(vec![step_object(TIME_FORMAT_KIND_ID, legacy_format())]),
                )]),
            ),
            step(
                "core.logic.switch",
                config(&[(
                    "cases",
                    Variant::Array(vec![Variant::Object(BTreeMap::from([(
                        CASE_CHAIN_KEY.to_owned(),
                        Variant::Array(vec![step_object(
                            "core.logic.if",
                            config(&[("then", inner_chain)]),
                        )]),
                    )]))]),
                )]),
            ),
        ];
        assert!(translate_legacy_time_steps(&mut steps));
        let expected = Variant::Object(translated_config(FORMAT_PATTERN_KEY));
        let then_step = &steps[0].config["then"].as_array().unwrap()[0];
        assert_eq!(then_step.as_object().unwrap()[STEP_CONFIG_KEY], expected);
        let case_step = &steps[1].config["cases"].as_array().unwrap()[0]
            .as_object()
            .unwrap()[CASE_CHAIN_KEY]
            .as_array()
            .unwrap()[0];
        let deepest = &case_step.as_object().unwrap()[STEP_CONFIG_KEY]
            .as_object()
            .unwrap()["then"]
            .as_array()
            .unwrap()[0];
        assert_eq!(deepest.as_object().unwrap()[STEP_CONFIG_KEY], expected);
    }

    #[test]
    fn an_existing_utc_false_is_kept() {
        let mut steps = vec![step(
            TIME_FORMAT_KIND_ID,
            config(&[
                (FORMAT_PATTERN_KEY, s(LEGACY)),
                (UTC_KEY, Variant::Bool(false)),
            ]),
        )];
        assert!(translate_legacy_time_steps(&mut steps));
        assert_eq!(
            steps[0].config,
            config(&[
                (FORMAT_PATTERN_KEY, s(TRANSLATED)),
                (UTC_KEY, Variant::Bool(false)),
            ])
        );
    }

    #[test]
    fn untranslatable_ldml_and_non_time_steps_are_left_alone_and_report_no_change() {
        let original = vec![
            step(
                TIME_FORMAT_KIND_ID,
                config(&[(FORMAT_PATTERN_KEY, s("[unix_timestamp]"))]),
            ),
            step(
                TIME_PARSE_KIND_ID,
                config(&[(PARSE_PATTERN_KEY, s("dd.MM.yyyy"))]),
            ),
            step(TIME_NOW_KIND_ID, config(&[])),
            step("core.string.format", config(&[("format", s(LEGACY))])),
            step(
                "core.logic.if",
                config(&[(
                    "then",
                    Variant::Array(vec![step_object(
                        "core.string.format",
                        config(&[(FORMAT_PATTERN_KEY, s(LEGACY))]),
                    )]),
                )]),
            ),
        ];
        let mut steps = original.clone();
        assert!(!translate_legacy_time_steps(&mut steps));
        assert_eq!(steps, original);
    }

    #[test]
    fn translating_twice_changes_nothing_the_second_time() {
        let mut steps = vec![step(
            TIME_PARSE_KIND_ID,
            config(&[(PARSE_PATTERN_KEY, s(LEGACY))]),
        )];
        assert!(translate_legacy_time_steps(&mut steps));
        let once = steps.clone();
        assert!(!translate_legacy_time_steps(&mut steps));
        assert_eq!(steps, once);
    }
}
