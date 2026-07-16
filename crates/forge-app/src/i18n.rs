use std::rc::Rc;
use std::sync::Arc;

use fluent::{FluentBundle, FluentResource};
use fluent_langneg::{NegotiationStrategy, negotiate_languages};
use forge_storage::{Language, SettingsRepo};
use unic_langid::LanguageIdentifier;

const EN_MAIN_FTL: &str = include_str!("../locales/en/main.ftl");
const UK_MAIN_FTL: &str = include_str!("../locales/uk/main.ftl");

fn build_bundle(lang: Language) -> Rc<FluentBundle<FluentResource>> {
    let tag = match lang {
        Language::En => "en-US",
        Language::Uk => "uk-UA",
    };
    // invariant: hard-coded BCP-47 tags always parse without error
    let lang_id: LanguageIdentifier = tag.parse().unwrap_or_else(|_| {
        tracing::warn!("BCP-47 parse failed for {tag}; falling back to base tag");
        let base = &tag[..2];
        base.parse().unwrap_or_default()
    });
    let ftl_source = match lang {
        Language::En => EN_MAIN_FTL,
        Language::Uk => UK_MAIN_FTL,
    };

    let mut bundle = FluentBundle::new(vec![lang_id]);
    let resource = match FluentResource::try_new(ftl_source.to_owned()) {
        Ok(r) => r,
        Err((r, errors)) => {
            tracing::warn!(?errors, "FTL parse errors in {:?} bundle", lang);
            r
        }
    };
    if let Err(errors) = bundle.add_resource(resource) {
        tracing::warn!(?errors, "FTL resource add errors for {:?}", lang);
    }
    Rc::new(bundle)
}

/// Detects the OS locale on first run and maps it to a supported `Language`.
///
/// Returns `None` if the OS locale does not negotiate to Ukrainian - in that case the caller
/// should persist and use `Language::En`.
fn negotiate_os_locale() -> Option<Language> {
    let os_locale = sys_locale::get_locale()?;
    let requested: Vec<LanguageIdentifier> = os_locale
        .parse::<LanguageIdentifier>()
        .map(|l| vec![l])
        .unwrap_or_default();
    if requested.is_empty() {
        return None;
    }
    // invariant: "en" and "uk" are valid BCP-47 base tags
    let en: LanguageIdentifier = "en".parse().unwrap_or_default();
    let uk: LanguageIdentifier = "uk".parse().unwrap_or_default();
    let available = vec![en.clone(), uk];
    let negotiated = negotiate_languages(
        &requested,
        &available,
        Some(&en),
        NegotiationStrategy::Filtering,
    );
    match negotiated.first().map(|l| l.to_string()).as_deref() {
        Some("uk") => Some(Language::Uk),
        _ => None,
    }
}

/// Returns `(language_to_use, language_to_persist)`. The second element is `Some` only on first
/// run (no persisted setting yet) - caller must follow up with `SettingsRepo::set_language` to
/// pin the negotiated OS locale.
pub async fn resolve_startup_language(
    settings: Arc<dyn SettingsRepo>,
) -> (Language, Option<Language>) {
    match settings
        .get_string(forge_storage::reserved_keys::LANGUAGE)
        .await
    {
        Ok(Some(s)) => {
            let lang = s.parse::<Language>().unwrap_or(Language::En);
            (lang, None)
        }
        Ok(None) => {
            let detected = negotiate_os_locale().unwrap_or(Language::En);
            (detected, Some(detected))
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to read language setting; defaulting to en");
            (Language::En, None)
        }
    }
}

/// Builds the `FluentBundle` for `lang` and installs it into the `forge-widgets` thread-local.
///
/// Must be called on the main/render thread - the thread-local is per-thread and iced's view
/// loop runs on the main thread.
pub fn install_language(lang: Language) {
    let bundle = build_bundle(lang);
    forge_widgets::install_bundle(bundle);
    let locale_id: &'static str = match lang {
        Language::En => "en",
        Language::Uk => "uk",
    };
    forge_widgets::set_locale_id(locale_id);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::collections::BTreeSet;

    use forge_widgets::{ArgsBuilder, tr_lookup};

    use super::*;

    /// Message ids start at column 0 as `id =`; continuation/plural-branch lines
    /// are indented, comments start with `#`, attributes with `.`.
    fn message_ids(ftl: &str) -> BTreeSet<&str> {
        ftl.lines()
            .filter(|line| line.starts_with(|c: char| c.is_ascii_alphabetic()))
            .filter_map(|line| line.split_once('='))
            .map(|(id, _)| id.trim())
            .filter(|id| {
                id.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            })
            .collect()
    }

    #[test]
    fn en_and_uk_locale_key_sets_are_identical() {
        let en = message_ids(EN_MAIN_FTL);
        let uk = message_ids(UK_MAIN_FTL);
        let missing_in_uk: Vec<_> = en.difference(&uk).collect();
        let missing_in_en: Vec<_> = uk.difference(&en).collect();
        assert!(
            missing_in_uk.is_empty() && missing_in_en.is_empty(),
            "locale key drift - missing in uk: {missing_in_uk:?}; missing in en: {missing_in_en:?}"
        );
    }

    #[test]
    fn locale_resources_parse_and_install_without_errors() {
        for (lang, source) in [(Language::En, EN_MAIN_FTL), (Language::Uk, UK_MAIN_FTL)] {
            let resource = fluent::FluentResource::try_new(source.to_owned())
                .unwrap_or_else(|(_, errors)| panic!("{lang:?} FTL has syntax errors: {errors:?}"));
            let mut bundle = fluent::FluentBundle::new(vec![
                "en".parse::<unic_langid::LanguageIdentifier>().unwrap(),
            ]);
            // add_resource reports duplicate-id overrides that parsing alone misses.
            bundle
                .add_resource(resource)
                .unwrap_or_else(|errors| panic!("{lang:?} FTL has duplicate ids: {errors:?}"));
        }
    }

    #[test]
    fn missing_key_falls_back_to_raw_key_without_panic() {
        install_language(Language::En);
        assert_eq!(
            tr_lookup("definitely_not_a_real_key", None),
            "definitely_not_a_real_key"
        );
    }

    #[test]
    fn installing_a_different_language_switches_translations_on_this_thread() {
        install_language(Language::En);
        assert_eq!(tr_lookup("common_cancel", None), "Cancel");
        install_language(Language::Uk);
        assert_eq!(tr_lookup("common_cancel", None), "Скасувати");
    }

    #[test]
    fn uk_relative_time_plural_messages_format_for_every_plural_category() {
        install_language(Language::Uk);
        // 1 → one, 2 → few, 5 → many, 21 → one, 100 → many (CLDR uk rules); a
        // syntax error in any plural branch would surface as a raw-key fallback
        // or a missing count.
        for key in [
            "fmt_relative_seconds",
            "fmt_relative_minutes",
            "fmt_relative_hours",
            "fmt_relative_days",
        ] {
            for count in [1_i64, 2, 5, 21, 100] {
                let args = ArgsBuilder::new().set("count", count).build();
                let formatted = tr_lookup(key, Some(&args));
                assert_ne!(formatted, key, "{key} fell back to raw key");
                assert!(
                    formatted.contains(&count.to_string()),
                    "{key} with count {count} lost the count: {formatted:?}"
                );
                assert!(
                    formatted.contains("тому"),
                    "{key} with count {count} lost the phrase: {formatted:?}"
                );
            }
        }
    }

    #[test]
    fn fmt_feed_time_pattern_resolves_literal_tokens_not_fluent_references() {
        // Regression: commit eb19280 fixed fmt_feed_time_pattern from {HH}:{MM}:{SS}.{mmm}
        // (which Fluent interprets as message references) to %HH%:%MM%:%SS%.%mmm% (literal
        // tokens for forge-widgets::fmt_feed_time to substitute). This test ensures the bundle
        // returns the literal pattern with no Fluent placeable interpretation.
        for lang in [Language::En, Language::Uk] {
            install_language(lang);
            let pattern = tr_lookup("fmt_feed_time_pattern", None);
            assert_eq!(
                pattern, "%HH%:%MM%:%SS%.%mmm%",
                "{lang:?} pattern was not literal: {pattern:?}"
            );
            // If {HH} form is reintroduced, Fluent will still try to resolve the references.
            // Ensure no braces leaked into the resolved pattern.
            assert!(
                !pattern.contains('{'),
                "{lang:?} pattern contains '{{': {pattern:?}"
            );
            assert!(
                !pattern.contains('}'),
                "{lang:?} pattern contains '}}': {pattern:?}"
            );
        }
    }
}
