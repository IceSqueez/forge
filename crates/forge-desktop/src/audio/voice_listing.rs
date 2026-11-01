use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use forge_components::{contains_folded, tr, tr_find, ui_names_language};
use forge_speech_types::{LanguageCode, TtsVoice, VoiceGender};
use forge_types::NamesLanguage;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GenderFilter {
    #[default]
    All,
    Male,
    Female,
}

impl GenderFilter {
    pub(crate) const ALL: [Self; 3] = [Self::All, Self::Male, Self::Female];

    pub(crate) fn label(self) -> String {
        match self {
            Self::All => tr!("tts_engines_voices_gender_all"),
            Self::Male => tr!("tts_engines_voices_gender_male"),
            Self::Female => tr!("tts_engines_voices_gender_female"),
        }
    }

    fn accepts(self, gender: &VoiceGender) -> bool {
        match self {
            Self::All => true,
            Self::Male => *gender == VoiceGender::Male,
            Self::Female => *gender == VoiceGender::Female,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct VoiceFilter {
    pub(crate) folded_name: String,
    pub(crate) language: Option<String>,
    pub(crate) gender: GenderFilter,
}

impl VoiceFilter {
    fn accepts(&self, voice: &TtsVoice) -> bool {
        self.gender.accepts(&voice.gender) && contains_folded(&voice.name, &self.folded_name)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct VoiceLanguage {
    pub(crate) key: String,
    pub(crate) name: String,
    pub(crate) count: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct VoiceGroup {
    pub(crate) name: String,
    voice_indices: Vec<usize>,
}

impl VoiceGroup {
    pub(crate) fn count(&self) -> usize {
        self.voice_indices.len()
    }
}

pub(crate) struct VoiceListing {
    catalog: Arc<Vec<TtsVoice>>,
    pub(crate) engine_total: usize,
    pub(crate) languages: Vec<VoiceLanguage>,
    pub(crate) selected_language: Option<String>,
    pub(crate) groups: Vec<VoiceGroup>,
}

impl VoiceListing {
    fn derive(
        catalog: Arc<Vec<TtsVoice>>,
        engine_id: &str,
        filter: &VoiceFilter,
        language_name: impl Fn(&str) -> String,
    ) -> Self {
        let mut by_language: BTreeMap<String, Vec<(usize, &TtsVoice)>> = BTreeMap::new();
        for (index, voice) in catalog.iter().enumerate() {
            if voice.engine_id.0 == engine_id {
                by_language
                    .entry(language_key(&voice.locale))
                    .or_default()
                    .push((index, voice));
            }
        }

        let mut languages: Vec<VoiceLanguage> = by_language
            .iter()
            .map(|(key, voices)| VoiceLanguage {
                name: language_name(key),
                key: key.clone(),
                count: voices.len(),
            })
            .collect();
        languages.sort_by_cached_key(|language| language.name.to_lowercase());

        let selected_language = filter
            .language
            .clone()
            .filter(|key| by_language.contains_key(key));
        let groups = languages
            .iter()
            .filter(|language| {
                selected_language
                    .as_ref()
                    .is_none_or(|selected| *selected == language.key)
            })
            .filter_map(|language| {
                let mut matching: Vec<(usize, &TtsVoice)> = by_language
                    .get(&language.key)?
                    .iter()
                    .copied()
                    .filter(|(_, voice)| filter.accepts(voice))
                    .collect();
                matching.sort_by_cached_key(|(_, voice)| voice.name.to_lowercase());
                (!matching.is_empty()).then(|| VoiceGroup {
                    name: language.name.clone(),
                    voice_indices: matching.into_iter().map(|(index, _)| index).collect(),
                })
            })
            .collect();

        let engine_total = languages.iter().map(|language| language.count).sum();
        Self {
            engine_total,
            languages,
            selected_language,
            groups,
            catalog,
        }
    }

    pub(crate) fn voices_of<'a>(
        &'a self,
        group: &'a VoiceGroup,
    ) -> impl Iterator<Item = &'a TtsVoice> {
        group
            .voice_indices
            .iter()
            .filter_map(|index| self.catalog.get(*index))
    }

    pub(crate) fn selected_language_name(&self) -> Option<&str> {
        let selected = self.selected_language.as_ref()?;
        self.languages
            .iter()
            .find(|language| language.key == *selected)
            .map(|language| language.name.as_str())
    }
}

fn language_key(locale: &str) -> String {
    LanguageCode::from_locale(locale)
        .map_or_else(|| locale.to_owned(), |language| language.to_string())
}

fn language_display_name(key: &str) -> String {
    if key.is_empty() {
        return tr!("tts_voice_language_unset");
    }
    tr_find(&format!("tts_language_{key}"), None).unwrap_or_else(|| key.to_owned())
}

struct CachedListing {
    engine_id: String,
    filter: VoiceFilter,
    names_language: NamesLanguage,
    listing: Rc<VoiceListing>,
}

impl CachedListing {
    fn is_for(
        &self,
        catalog: &Arc<Vec<TtsVoice>>,
        engine_id: &str,
        filter: &VoiceFilter,
        names_language: NamesLanguage,
    ) -> bool {
        Arc::ptr_eq(&self.listing.catalog, catalog)
            && self.engine_id == engine_id
            && self.filter == *filter
            && self.names_language == names_language
    }
}

#[derive(Default)]
pub(crate) struct VoiceListingCache {
    cached: Option<CachedListing>,
}

impl VoiceListingCache {
    pub(crate) fn listing(
        &mut self,
        catalog: &Arc<Vec<TtsVoice>>,
        engine_id: &str,
        filter: &VoiceFilter,
    ) -> Rc<VoiceListing> {
        self.listing_named(
            catalog,
            engine_id,
            filter,
            ui_names_language(),
            language_display_name,
        )
    }

    fn listing_named(
        &mut self,
        catalog: &Arc<Vec<TtsVoice>>,
        engine_id: &str,
        filter: &VoiceFilter,
        names_language: NamesLanguage,
        language_name: impl Fn(&str) -> String,
    ) -> Rc<VoiceListing> {
        if let Some(cached) = &self.cached
            && cached.is_for(catalog, engine_id, filter, names_language)
        {
            return Rc::clone(&cached.listing);
        }
        let listing = Rc::new(VoiceListing::derive(
            Arc::clone(catalog),
            engine_id,
            filter,
            language_name,
        ));
        self.cached = Some(CachedListing {
            engine_id: engine_id.to_owned(),
            filter: filter.clone(),
            names_language,
            listing: Rc::clone(&listing),
        });
        listing
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use forge_speech_types::{EngineId, VoiceId};
    use forge_storage::Language;

    use crate::app::i18n::install_language;

    use super::*;

    const ENGINE: &str = "nsspeech";

    fn voice(engine: &str, name: &str, locale: &str, gender: VoiceGender) -> TtsVoice {
        TtsVoice {
            id: VoiceId(format!("{engine}.{name}")),
            name: name.to_owned(),
            locale: locale.to_owned(),
            gender,
            engine_id: EngineId(engine.to_owned()),
            is_neural: false,
            sample_rate_hint: 22_050,
        }
    }

    fn catalog() -> Arc<Vec<TtsVoice>> {
        Arc::new(vec![
            voice(ENGINE, "Samantha", "en-US", VoiceGender::Female),
            voice(ENGINE, "Daniel", "en-GB", VoiceGender::Male),
            voice(ENGINE, "Леся", "uk-UA", VoiceGender::Female),
            voice(ENGINE, "Fred", "en_US", VoiceGender::Neutral),
            voice(ENGINE, "Ting", "cmn-CN", VoiceGender::Female),
            voice("piper", "Amy", "en-US", VoiceGender::Female),
        ])
    }

    fn name_of(key: &str) -> String {
        match key {
            "en" => "English".to_owned(),
            "uk" => "Ukrainian".to_owned(),
            raw => raw.to_owned(),
        }
    }

    fn shown(listing: &VoiceListing) -> Vec<(String, Vec<String>)> {
        listing
            .groups
            .iter()
            .map(|group| {
                (
                    group.name.clone(),
                    listing
                        .voices_of(group)
                        .map(|voice| voice.name.clone())
                        .collect(),
                )
            })
            .collect()
    }

    fn filtered(query: &str, language: Option<&str>, gender: GenderFilter) -> VoiceFilter {
        VoiceFilter {
            folded_name: query.to_lowercase(),
            language: language.map(str::to_owned),
            gender,
        }
    }

    #[test]
    fn filters_combine_into_the_expected_groups() {
        let unknown = ("cmn-CN", vec!["Ting"]);
        let english = ("English", vec!["Daniel", "Fred", "Samantha"]);
        let ukrainian = ("Ukrainian", vec!["Леся"]);
        for (label, filter, expected) in [
            (
                "no filter shows everything of the engine alphabetically",
                filtered("", None, GenderFilter::All),
                vec![unknown.clone(), english.clone(), ukrainian.clone()],
            ),
            (
                "name match folds latin case",
                filtered("SAMA", None, GenderFilter::All),
                vec![("English", vec!["Samantha"])],
            ),
            (
                "name match folds cyrillic case",
                filtered("ЛЕС", None, GenderFilter::All),
                vec![ukrainian.clone()],
            ),
            (
                "language keeps one group and merges its regions",
                filtered("", Some("en"), GenderFilter::All),
                vec![english.clone()],
            ),
            (
                "male hides neutral",
                filtered("", None, GenderFilter::Male),
                vec![("English", vec!["Daniel"])],
            ),
            (
                "female hides neutral",
                filtered("", None, GenderFilter::Female),
                vec![
                    unknown.clone(),
                    ("English", vec!["Samantha"]),
                    ukrainian.clone(),
                ],
            ),
            (
                "name language and gender combine",
                filtered("a", Some("en"), GenderFilter::Male),
                vec![("English", vec!["Daniel"])],
            ),
            (
                "contradicting filters match nothing",
                filtered("Леся", Some("en"), GenderFilter::All),
                vec![],
            ),
            (
                "a language the engine lacks is dropped",
                filtered("", Some("fr"), GenderFilter::All),
                vec![unknown.clone(), english.clone(), ukrainian.clone()],
            ),
        ] {
            let listing = VoiceListing::derive(catalog(), ENGINE, &filter, name_of);
            let expected: Vec<(String, Vec<String>)> = expected
                .into_iter()
                .map(|(group, names)| {
                    (
                        group.to_owned(),
                        names.into_iter().map(str::to_owned).collect(),
                    )
                })
                .collect();
            assert_eq!(shown(&listing), expected, "{label}");
        }
    }

    #[test]
    fn language_choices_and_total_ignore_the_active_filter() {
        let filter = filtered("Леся", Some("uk"), GenderFilter::Male);

        let listing = VoiceListing::derive(catalog(), ENGINE, &filter, name_of);

        assert_eq!(listing.engine_total, 5);
        let choices: Vec<(&str, &str, usize)> = listing
            .languages
            .iter()
            .map(|l| (l.key.as_str(), l.name.as_str(), l.count))
            .collect();
        assert_eq!(
            choices,
            [
                ("cmn-CN", "cmn-CN", 1),
                ("en", "English", 3),
                ("uk", "Ukrainian", 1),
            ]
        );
        assert_eq!(listing.selected_language_name(), Some("Ukrainian"));
        assert!(listing.groups.is_empty());
    }

    #[test]
    fn a_dropped_language_selection_reads_as_all_languages() {
        let listing = VoiceListing::derive(
            catalog(),
            ENGINE,
            &filtered("", Some("fr"), GenderFilter::All),
            name_of,
        );

        assert_eq!(listing.selected_language, None);
        assert_eq!(listing.selected_language_name(), None);
    }

    #[test]
    fn group_counts_follow_the_filter_while_an_unknown_engine_lists_nothing() {
        let listing = VoiceListing::derive(
            catalog(),
            ENGINE,
            &filtered("", Some("en"), GenderFilter::Female),
            name_of,
        );
        let counts: Vec<usize> = listing.groups.iter().map(VoiceGroup::count).collect();
        assert_eq!(counts, [1]);

        let empty = VoiceListing::derive(catalog(), "absent", &VoiceFilter::default(), name_of);
        assert_eq!(empty.engine_total, 0);
        assert!(empty.languages.is_empty() && empty.groups.is_empty());
    }

    #[test]
    fn language_names_come_from_the_catalog_and_fall_back_to_the_raw_code() {
        for (language, ukrainian) in [(Language::En, "Ukrainian"), (Language::Uk, "Українська")]
        {
            install_language(language);
            for (key, expected) in [
                (language_key("uk-UA"), ukrainian),
                (language_key("uk_UA"), ukrainian),
                (language_key("cmn-CN"), "cmn-CN"),
                (language_key("xx-YY"), "xx"),
            ] {
                assert_eq!(language_display_name(&key), expected, "{language:?}");
            }
            assert!(!language_display_name(&language_key("")).is_empty());
        }
    }

    #[test]
    fn the_listing_is_derived_again_only_when_an_input_changes() {
        let derivations = Cell::new(0);
        let counting = |key: &str| {
            derivations.set(derivations.get() + 1);
            key.to_owned()
        };
        let solo_catalog = Arc::new(vec![voice(ENGINE, "Fred", "en-US", VoiceGender::Male)]);
        let default_filter = VoiceFilter::default();
        let narrowed = filtered("f", None, GenderFilter::All);
        let mut cache = VoiceListingCache::default();

        for (label, engine, filter, names, expected) in [
            (
                "first request",
                ENGINE,
                &default_filter,
                NamesLanguage::En,
                1,
            ),
            ("same inputs", ENGINE, &default_filter, NamesLanguage::En, 1),
            ("filter change", ENGINE, &narrowed, NamesLanguage::En, 2),
            ("engine change", "piper", &narrowed, NamesLanguage::En, 2),
            ("back to engine", ENGINE, &narrowed, NamesLanguage::En, 3),
            ("ui language", ENGINE, &narrowed, NamesLanguage::Uk, 4),
        ] {
            cache.listing_named(&solo_catalog, engine, filter, names, counting);
            assert_eq!(derivations.get(), expected, "{label}");
        }

        let replaced_catalog = Arc::new(solo_catalog.as_ref().clone());
        cache.listing_named(
            &replaced_catalog,
            ENGINE,
            &narrowed,
            NamesLanguage::Uk,
            counting,
        );
        assert_eq!(derivations.get(), 5, "replaced catalog");
    }
}
