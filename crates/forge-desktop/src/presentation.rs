use forge_components::DisplayFormatChoices;
use forge_storage::Language;
use forge_types::NamesLanguage;
use gpui::{App, Global};

pub(crate) struct ActiveLanguage(pub Language);

impl Global for ActiveLanguage {}

pub(crate) fn active_names_language(cx: &App) -> NamesLanguage {
    cx.try_global::<ActiveLanguage>()
        .map(|active| crate::i18n::date_names_language(active.0))
        .unwrap_or_default()
}

pub(crate) struct ActiveDisplayFormats(pub DisplayFormatChoices);

impl Global for ActiveDisplayFormats {}
