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
