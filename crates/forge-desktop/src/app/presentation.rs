use forge_components::{DisplayFormatChoices, Region, install_process_display_formats};
use forge_runtime::DateNamesHandle;
use forge_storage::Language;
use forge_types::{NamesLanguage, Shared};
use gpui::{App, Global};

use crate::app::i18n::{
    ANONYMOUS_DONOR_KEY, catalog_language, date_names_language, install_process_language,
};

pub(crate) struct LocaleState {
    pub language: Language,
    pub formats: DisplayFormatChoices,
}

impl Global for LocaleState {}

pub(crate) struct LanguageSinks<'a> {
    pub anonymous_donor: &'a Shared<String>,
    pub date_names: &'a DateNamesHandle,
}

fn locale_state(cx: &mut App) -> &mut LocaleState {
    if !cx.has_global::<LocaleState>() {
        seed_presentation(cx);
    }
    cx.global_mut::<LocaleState>()
}

pub(crate) fn seed_presentation(cx: &mut App) {
    let language = Language::default();
    cx.set_global(LocaleState {
        language,
        formats: DisplayFormatChoices::following(Region::for_language(catalog_language(language))),
    });
}

pub(crate) fn install_presentation(
    language: Language,
    formats: DisplayFormatChoices,
    cx: &mut App,
) {
    install_process_language(language);
    install_process_display_formats(formats.resolve());
    cx.set_global(LocaleState { language, formats });
}

pub(crate) fn apply_language(language: Language, sinks: LanguageSinks<'_>, cx: &mut App) {
    install_process_language(language);
    sinks
        .anonymous_donor
        .store(forge_components::tr!(ANONYMOUS_DONOR_KEY));
    sinks.date_names.set_language(date_names_language(language));
    locale_state(cx).language = language;
}

pub(crate) fn apply_display_formats(formats: DisplayFormatChoices, cx: &mut App) {
    install_process_display_formats(formats.resolve());
    locale_state(cx).formats = formats;
}

pub(crate) fn active_names_language(cx: &App) -> NamesLanguage {
    cx.try_global::<LocaleState>()
        .map(|active| date_names_language(active.language))
        .unwrap_or_default()
}
