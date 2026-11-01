use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundCategory {
    Memes,
    Alerts,
    Music,
    Voice,
}

impl SoundCategory {
    pub const ALL: [Self; 4] = [Self::Memes, Self::Alerts, Self::Music, Self::Voice];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Memes => "memes",
            Self::Alerts => "alerts",
            Self::Music => "music",
            Self::Voice => "voice",
        }
    }

    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.as_str() == token)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinSoundEntry {
    pub builtin_id: &'static str,
    pub display_name: &'static str,
    pub category: SoundCategory,
    pub icon_name: &'static str,
}

pub(crate) const BUILTIN_FILE_EXTENSIONS: &[&str] = &["mp3", "wav", "ogg", "flac"];

pub const BUILTIN_SOUNDS: &[BuiltinSoundEntry] = &[
    BuiltinSoundEntry {
        builtin_id: "vine_boom",
        display_name: "Vine boom",
        category: SoundCategory::Memes,
        icon_name: "ripple",
    },
    BuiltinSoundEntry {
        builtin_id: "airhorn",
        display_name: "MLG Airhorn",
        category: SoundCategory::Memes,
        icon_name: "speakerphone",
    },
    BuiltinSoundEntry {
        builtin_id: "bruh",
        display_name: "Bruh",
        category: SoundCategory::Memes,
        icon_name: "mood-crazy-happy",
    },
    BuiltinSoundEntry {
        builtin_id: "sad_trombone",
        display_name: "Sad trombone",
        category: SoundCategory::Memes,
        icon_name: "music",
    },
    BuiltinSoundEntry {
        builtin_id: "badum_tss",
        display_name: "Ba-dum-tss",
        category: SoundCategory::Memes,
        icon_name: "hand-click",
    },
    BuiltinSoundEntry {
        builtin_id: "wow",
        display_name: "Wow",
        category: SoundCategory::Memes,
        icon_name: "sparkles",
    },
    BuiltinSoundEntry {
        builtin_id: "sad_violin",
        display_name: "Sad violin",
        category: SoundCategory::Memes,
        icon_name: "mood-sad",
    },
    BuiltinSoundEntry {
        builtin_id: "crickets",
        display_name: "Crickets",
        category: SoundCategory::Memes,
        icon_name: "wave-sine",
    },
    BuiltinSoundEntry {
        builtin_id: "fail_buzzer",
        display_name: "Fail buzzer",
        category: SoundCategory::Memes,
        icon_name: "alert-triangle",
    },
    BuiltinSoundEntry {
        builtin_id: "to_be_continued",
        display_name: "To Be Continued",
        category: SoundCategory::Memes,
        icon_name: "player-skip-forward",
    },
    BuiltinSoundEntry {
        builtin_id: "emotional_damage",
        display_name: "Emotional damage",
        category: SoundCategory::Memes,
        icon_name: "bolt",
    },
    BuiltinSoundEntry {
        builtin_id: "oof",
        display_name: "Oof",
        category: SoundCategory::Memes,
        icon_name: "volume",
    },
    BuiltinSoundEntry {
        builtin_id: "sus",
        display_name: "Among Us (sus)",
        category: SoundCategory::Memes,
        icon_name: "eye",
    },
    BuiltinSoundEntry {
        builtin_id: "windows_error",
        display_name: "Windows error",
        category: SoundCategory::Memes,
        icon_name: "x",
    },
    BuiltinSoundEntry {
        builtin_id: "wasted",
        display_name: "Wasted",
        category: SoundCategory::Memes,
        icon_name: "flag",
    },
    BuiltinSoundEntry {
        builtin_id: "discord_ping",
        display_name: "Discord ping",
        category: SoundCategory::Memes,
        icon_name: "message-circle",
    },
];

pub fn resolve_builtin_path(data_dir: &Path, builtin_id: &str) -> Option<PathBuf> {
    let dir = data_dir.join("soundboard").join("builtin");
    BUILTIN_FILE_EXTENSIONS.iter().find_map(|ext| {
        let candidate = dir.join(format!("{builtin_id}.{ext}"));
        candidate.is_file().then_some(candidate)
    })
}

pub fn builtin_availability(data_dir: &Path) -> Vec<(BuiltinSoundEntry, bool)> {
    BUILTIN_SOUNDS
        .iter()
        .map(|entry| {
            let present = resolve_builtin_path(data_dir, entry.builtin_id).is_some();
            (*entry, present)
        })
        .collect()
}
