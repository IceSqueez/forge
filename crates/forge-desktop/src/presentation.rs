use forge_components::DisplayFormatChoices;
use forge_storage::Language;
use gpui::Global;

pub struct ActiveLanguage(pub Language);

impl Global for ActiveLanguage {}

pub struct ActiveDisplayFormats(pub DisplayFormatChoices);

impl Global for ActiveDisplayFormats {}
