#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsSection {
    Dashboard,
    Engines,
    Aliases,
    Filters,
}

impl TtsSection {
    pub const ALL: [TtsSection; 4] = [
        TtsSection::Dashboard,
        TtsSection::Engines,
        TtsSection::Aliases,
        TtsSection::Filters,
    ];

    pub fn key(self) -> &'static str {
        match self {
            TtsSection::Dashboard => "dashboard",
            TtsSection::Engines => "engines",
            TtsSection::Aliases => "aliases",
            TtsSection::Filters => "filters",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|section| section.key() == key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    General,
    Appearance,
    Language,
    Shortcuts,
    Notifications,
    Audio,
    Scripting,
    Storage,
    WebSocket,
    About,
    Diagnostics,
}

impl SettingsSection {
    pub const ALL: [SettingsSection; 11] = [
        SettingsSection::General,
        SettingsSection::Appearance,
        SettingsSection::Language,
        SettingsSection::Shortcuts,
        SettingsSection::Notifications,
        SettingsSection::Audio,
        SettingsSection::Scripting,
        SettingsSection::Storage,
        SettingsSection::WebSocket,
        SettingsSection::About,
        SettingsSection::Diagnostics,
    ];

    pub fn key(self) -> &'static str {
        match self {
            SettingsSection::General => "general",
            SettingsSection::Appearance => "appearance",
            SettingsSection::Language => "language",
            SettingsSection::Shortcuts => "shortcuts",
            SettingsSection::Notifications => "notifications",
            SettingsSection::Audio => "audio",
            SettingsSection::Scripting => "scripting",
            SettingsSection::Storage => "storage",
            SettingsSection::WebSocket => "websocket",
            SettingsSection::About => "about",
            SettingsSection::Diagnostics => "diagnostics",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|section| section.key() == key)
    }
}
