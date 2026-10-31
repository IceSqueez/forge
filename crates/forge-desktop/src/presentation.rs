use forge_components::DisplayFormatChoices;
use forge_storage::Language;
use gpui::Global;

pub(crate) struct ActiveLanguage(pub Language);

impl Global for ActiveLanguage {}

pub(crate) struct ActiveDisplayFormats(pub DisplayFormatChoices);

impl Global for ActiveDisplayFormats {}
