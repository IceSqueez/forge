use std::sync::Arc;

use fluent_langneg::{NegotiationStrategy, negotiate_languages};
use forge_components::{CatalogLanguage, DisplayFormatChoices, Region};
use forge_storage::settings::core_settings::{LANGUAGE, REGION};
use forge_storage::{
    Language, SettingsRepo, read_stored_setting, stored_display_formats, write_setting,
};
use forge_types::NamesLanguage;
use unic_langid::LanguageIdentifier;

const EN_MAIN_FTL: &str = include_str!("../locales/en/main.ftl");
const UK_MAIN_FTL: &str = include_str!("../locales/uk/main.ftl");

pub(crate) fn app_catalogs(lang: Language) -> Vec<&'static str> {
    let desktop = match lang {
        Language::En => EN_MAIN_FTL,
        Language::Uk => UK_MAIN_FTL,
    };
    std::iter::once(desktop)
        .chain(crate::message_catalogs::fragments(lang))
        .collect()
}

fn negotiate_os_locale() -> Option<Language> {
    let os_locale = sys_locale::get_locale()?;
    let requested: Vec<LanguageIdentifier> = os_locale
        .parse::<LanguageIdentifier>()
        .map(|l| vec![l])
        .unwrap_or_default();
    if requested.is_empty() {
        return None;
    }
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

pub(crate) async fn resolve_startup_language(
    settings: Arc<dyn SettingsRepo>,
) -> (Language, Option<Language>) {
    match read_stored_setting(settings.as_ref(), &LANGUAGE).await {
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

pub(crate) fn catalog_language(lang: Language) -> CatalogLanguage {
    match lang {
        Language::En => CatalogLanguage::En,
        Language::Uk => CatalogLanguage::Uk,
    }
}

pub(crate) fn startup_region(os_locale: Option<&str>, lang: Language) -> Region {
    os_locale
        .and_then(Region::from_locale_tag)
        .unwrap_or_else(|| Region::for_language(catalog_language(lang)))
}

pub(crate) async fn resolve_startup_display_formats(
    settings: &dyn SettingsRepo,
    lang: Language,
) -> DisplayFormatChoices {
    let language_region = Region::for_language(catalog_language(lang));
    let stored = match stored_display_formats(settings).await {
        Ok(stored) => stored,
        Err(e) => {
            tracing::warn!(error = %e, "failed to read display format settings; following the language region");
            return DisplayFormatChoices::following(language_region);
        }
    };
    let region = match stored.region.as_deref() {
        Some(code) => Region::from_code(code).unwrap_or(language_region),
        None => {
            let detected = startup_region(sys_locale::get_locale().as_deref(), lang);
            if let Err(e) =
                write_setting(settings, &REGION, &Some(detected.code().to_owned())).await
            {
                tracing::warn!(error = %e, "failed to persist the detected region");
            }
            detected
        }
    };
    DisplayFormatChoices::from_stored(
        Some(region.code()),
        stored.date_format.as_deref(),
        stored.time_format.as_deref(),
        stored.first_day_of_week.as_deref(),
        language_region,
    )
}

pub(crate) const ANONYMOUS_DONOR_KEY: &str = "donation_anonymous_donor";

pub(crate) fn date_names_language(lang: Language) -> NamesLanguage {
    catalog_language(lang).names_language()
}

pub(crate) fn message_in(lang: Language, key: &str) -> String {
    forge_components::resolve_in_language(catalog_language(lang), &app_catalogs(lang), key)
}

pub(crate) fn install_os_default() {
    install_process_language(negotiate_os_locale().unwrap_or(Language::En));
}

pub(crate) fn install_process_language(lang: Language) {
    forge_components::install_process_language(catalog_language(lang), &app_catalogs(lang));
}

#[cfg(test)]
pub(crate) fn install_language(lang: Language) {
    forge_components::install_thread_language(catalog_language(lang), &app_catalogs(lang));
}

#[cfg(test)]
mod tests {
    use forge_components::{ArgsBuilder, tr_lookup};
    use forge_components::{
        ClockStyle, DateFormatChoice, FirstWeekday, FormatChoice, FormatOption,
    };
    use forge_storage::settings::core_settings::{DATE_FORMAT, TIME_FORMAT};
    use forge_storage::test_support::{MemorySettings, StoredSettingRow};
    use forge_storage::{RowClasses, SettingDisclosure, SettingPortability};

    use super::*;
    use crate::test_support::test_backend;

    const MARKER: &str = "MARKER-7";

    #[test]
    fn date_names_language_matches_the_ui_language_code() {
        for lang in [Language::En, Language::Uk] {
            assert_eq!(date_names_language(lang).code(), lang.to_string());
        }
    }

    #[test]
    fn strings_added_for_toggles_queues_voice_gate_and_server_resolve_in_both_locales() {
        for lang in [Language::En, Language::Uk] {
            install_language(lang);
            for (key, argument) in [
                ("tts_filters_output_strip_twitch_emotes", None),
                ("tts_filters_output_strip_twitch_emotes_meta", None),
                ("tts_filters_output_strip_reward_emotes", None),
                ("tts_filters_output_strip_reward_emotes_meta", None),
                ("settings_voice_gate_retry", None),
                ("soundboard_persist_failed", None),
                ("queues_not_live_badge", None),
                ("queues_not_live_tooltip", None),
                ("queues_apply_live_done", None),
                ("queues_apply_live_missing", None),
                ("queues_apply_live_not_found", None),
                ("queues_apply_live_failed", Some("error")),
                ("server_unavailable_banner", Some("reason")),
            ] {
                let resolved = match argument {
                    Some(name) => {
                        tr_lookup(key, Some(&ArgsBuilder::new().set(name, MARKER).build()))
                    }
                    None => tr_lookup(key, None),
                };

                assert_ne!(resolved, key, "{lang:?} {key} is missing");
                assert!(
                    argument.is_none() || resolved.contains(MARKER),
                    "{lang:?} {key} dropped its {argument:?} argument: {resolved}"
                );
            }
        }
    }

    #[test]
    fn startup_region_prefers_a_known_os_locale_region_over_the_ui_language() {
        for (os_locale, lang, expected) in [
            (Some("de_DE.UTF-8"), Language::Uk, Region::Germany),
            (Some("en-GB"), Language::En, Region::UnitedKingdom),
            (Some("en_NZ.UTF-8"), Language::Uk, Region::Ukraine),
            (Some("C"), Language::En, Region::UnitedStates),
            (Some("uk"), Language::Uk, Region::Ukraine),
            (None, Language::En, Region::UnitedStates),
            (None, Language::Uk, Region::Ukraine),
        ] {
            assert_eq!(
                startup_region(os_locale, lang),
                expected,
                "{os_locale:?} {lang:?}"
            );
        }
    }

    #[tokio::test]
    async fn stored_display_choices_are_read_back_without_rewriting_the_region() {
        let (settings, mut writes) = test_backend();
        settings.seed_raw(&REGION, "gb");
        settings.seed_raw(&DATE_FORMAT, "ymd");
        settings.seed_raw(&TIME_FORMAT, "bogus");

        let choices = resolve_startup_display_formats(settings.as_ref(), Language::Uk).await;

        assert_eq!(
            (choices, writes.try_recv().ok()),
            (
                DisplayFormatChoices {
                    region: Region::UnitedKingdom,
                    date_format: FormatChoice::Fixed(DateFormatChoice::YearMonthDay),
                    clock: FormatChoice::RegionDefault,
                    first_day: FormatChoice::RegionDefault,
                },
                None,
            )
        );
    }

    #[tokio::test]
    async fn an_unknown_stored_region_follows_the_ui_language_region() {
        let (settings, _writes) = test_backend();
        settings.seed_raw(&REGION, "ZZ");

        let choices = resolve_startup_display_formats(settings.as_ref(), Language::Uk).await;

        assert_eq!(choices, DisplayFormatChoices::following(Region::Ukraine));
    }

    #[tokio::test]
    async fn a_first_run_region_is_stored_once_and_read_back_on_the_next_start() {
        let (settings, mut writes) = test_backend();

        let first = resolve_startup_display_formats(settings.as_ref(), Language::En).await;
        let stored = writes.try_recv().ok();
        let second = resolve_startup_display_formats(settings.as_ref(), Language::Uk).await;

        assert_eq!(
            (stored, second, writes.try_recv().ok()),
            (
                Some((REGION.name().to_owned(), first.region.code().to_owned())),
                first,
                None,
            )
        );
    }

    #[tokio::test]
    async fn the_first_run_region_is_stamped_portable_and_verbatim() {
        let settings = MemorySettings::new();

        let choices = resolve_startup_display_formats(&settings, Language::En).await;

        assert_eq!(
            settings.rows(),
            vec![StoredSettingRow {
                key: REGION.name().to_owned(),
                value: choices.region.code().to_owned(),
                classes: RowClasses {
                    portability: SettingPortability::Portable,
                    disclosure: SettingDisclosure::Verbatim,
                },
            }]
        );
    }

    #[tokio::test]
    async fn unreadable_settings_follow_the_ui_language_region() {
        let settings = MemorySettings::new();
        settings.fail_reads(true);
        settings.fail_writes(true);

        let choices = resolve_startup_display_formats(&settings, Language::Uk).await;

        assert_eq!(choices, DisplayFormatChoices::following(Region::Ukraine));
    }

    #[test]
    fn every_region_clock_and_weekday_name_resolves_in_both_locales() {
        let keys = Region::ALL
            .map(Region::name_key)
            .into_iter()
            .chain(ClockStyle::ALL.iter().map(|clock| clock.label_key()))
            .chain(FirstWeekday::ALL.iter().map(|day| day.label_key()));
        for key in keys {
            for lang in [Language::En, Language::Uk] {
                assert_ne!(message_in(lang, key), key, "{lang:?} {key}");
            }
        }
    }

    #[test]
    fn step_count_copy_picks_the_cldr_plural_form_for_each_count() {
        let counts: [(i64, &str, &str); 6] = [
            (0, "0 steps", "0 кроків"),
            (1, "1 step", "1 крок"),
            (2, "2 steps", "2 кроки"),
            (5, "5 steps", "5 кроків"),
            (11, "11 steps", "11 кроків"),
            (21, "21 steps", "21 крок"),
        ];
        for (key, prefix) in [
            ("action_editor_sub_count", ""),
            ("action_editor_sub_actions_count", ""),
            ("action_editor_picker_sub_count", "· "),
            ("event_feed_step_count", ""),
        ] {
            for lang in [Language::En, Language::Uk] {
                install_language(lang);
                for (count, en, uk) in counts {
                    let expected = match lang {
                        Language::En => en,
                        Language::Uk => uk,
                    };
                    let args = ArgsBuilder::new().set("count", count).build();
                    let resolved = tr_lookup(key, Some(&args))
                        .replace(crate::test_support::FORMAT_ISOLATES, "");
                    assert_eq!(
                        resolved,
                        format!("{prefix}{expected}"),
                        "{lang:?} {key} count {count}"
                    );
                }
            }
        }
    }
}
