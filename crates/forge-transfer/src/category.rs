use crate::layout::{ARCHIVE_SEPARATOR, INCLUDED_DIRS};
use forge_paths::{ASSETS_DIR_NAME, MEDIA_DIR_NAME, OVERLAYS_DIR_NAME, TTS_DIR_NAME};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupCategory {
    Database,
    Media,
    Voices,
    Overlays,
    Assets,
}

impl BackupCategory {
    pub const ALL: [Self; 5] = [
        Self::Database,
        Self::Media,
        Self::Voices,
        Self::Overlays,
        Self::Assets,
    ];

    pub fn all() -> Vec<Self> {
        Self::ALL.to_vec()
    }

    pub fn directory(self) -> Option<&'static str> {
        match self {
            Self::Database => None,
            Self::Media => Some(MEDIA_DIR_NAME),
            Self::Voices => Some(TTS_DIR_NAME),
            Self::Overlays => Some(OVERLAYS_DIR_NAME),
            Self::Assets => Some(ASSETS_DIR_NAME),
        }
    }
}

pub(crate) fn directories_of(categories: &[BackupCategory]) -> Vec<&'static str> {
    INCLUDED_DIRS
        .into_iter()
        .filter(|dir| {
            categories
                .iter()
                .any(|category| category.directory() == Some(*dir))
        })
        .collect()
}

pub(crate) fn includes_database(categories: &[BackupCategory]) -> bool {
    categories.contains(&BackupCategory::Database)
}

pub(crate) fn admits_content(categories: &[BackupCategory], archive_path: &str) -> bool {
    let mut segments = archive_path.split(ARCHIVE_SEPARATOR).skip(1);
    match (segments.next(), segments.next()) {
        (Some(_), None) => true,
        (Some(top), Some(_)) => directories_of(categories).contains(&top),
        _ => false,
    }
}
