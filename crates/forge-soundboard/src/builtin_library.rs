use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinSoundEntry {
    pub builtin_id: &'static str,
    pub display_name: &'static str,
    pub category: &'static str,
    pub icon_name: &'static str,
    pub loop_playback: bool,
}

pub const BUILTIN_FILE_EXTENSIONS: &[&str] = &["mp3", "wav", "ogg", "flac"];

pub const BUILTIN_SOUNDS: &[BuiltinSoundEntry] = &[
    BuiltinSoundEntry {
        builtin_id: "vine_boom",
        display_name: "Vine boom",
        category: "memes",
        icon_name: "ripple",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "airhorn",
        display_name: "MLG Airhorn",
        category: "memes",
        icon_name: "speakerphone",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "bruh",
        display_name: "Bruh",
        category: "memes",
        icon_name: "mood-crazy-happy",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "sad_trombone",
        display_name: "Sad trombone",
        category: "memes",
        icon_name: "music",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "badum_tss",
        display_name: "Ba-dum-tss",
        category: "memes",
        icon_name: "hand-click",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "wow",
        display_name: "Wow",
        category: "memes",
        icon_name: "sparkles",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "sad_violin",
        display_name: "Sad violin",
        category: "memes",
        icon_name: "mood-sad",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "crickets",
        display_name: "Crickets",
        category: "memes",
        icon_name: "wave-sine",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "fail_buzzer",
        display_name: "Fail buzzer",
        category: "memes",
        icon_name: "alert-triangle",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "to_be_continued",
        display_name: "To Be Continued",
        category: "memes",
        icon_name: "player-skip-forward",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "emotional_damage",
        display_name: "Emotional damage",
        category: "memes",
        icon_name: "bolt",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "oof",
        display_name: "Oof",
        category: "memes",
        icon_name: "volume",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "sus",
        display_name: "Among Us (sus)",
        category: "memes",
        icon_name: "eye",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "windows_error",
        display_name: "Windows error",
        category: "memes",
        icon_name: "x",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "wasted",
        display_name: "Wasted",
        category: "memes",
        icon_name: "flag",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "discord_ping",
        display_name: "Discord ping",
        category: "memes",
        icon_name: "message-circle",
        loop_playback: false,
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
