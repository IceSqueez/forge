use forge_storage::Language;
use gpui::Global;

pub struct ActiveLanguage(pub Language);

impl Global for ActiveLanguage {}
