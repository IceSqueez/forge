use std::path::{Path, PathBuf};

/// One slot in the bundled builtin sound library. The audio files themselves are
/// NOT shipped in the repo - the maintainer supplies them under
/// `<data_dir>/soundboard/builtin/<builtin_id>.<ext>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinSoundEntry {
    pub builtin_id: &'static str,
    pub display_name: &'static str,
    pub category: &'static str,
    pub icon_name: &'static str,
    pub suggested_hotkey: &'static str,
    pub loop_playback: bool,
}

/// File extensions probed, in order, when resolving a builtin's audio file.
pub const BUILTIN_FILE_EXTENSIONS: &[&str] = &["mp3", "wav", "ogg", "flac"];

pub const BUILTIN_SOUNDS: &[BuiltinSoundEntry] = &[
    BuiltinSoundEntry {
        builtin_id: "airhorn",
        display_name: "Airhorn",
        category: "memes",
        icon_name: "speakerphone",
        suggested_hotkey: "1",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "sad_trombone",
        display_name: "Sad trombone",
        category: "memes",
        icon_name: "music",
        suggested_hotkey: "2",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "bruh",
        display_name: "Bruh",
        category: "memes",
        icon_name: "mood-crazy-happy",
        suggested_hotkey: "3",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "wow",
        display_name: "Wow",
        category: "memes",
        icon_name: "sparkles",
        suggested_hotkey: "4",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "new_follow",
        display_name: "New follow",
        category: "alerts",
        icon_name: "user-plus",
        suggested_hotkey: "5",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "new_sub",
        display_name: "New sub",
        category: "alerts",
        icon_name: "star",
        suggested_hotkey: "6",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "raid_siren",
        display_name: "Raid siren",
        category: "alerts",
        icon_name: "flag",
        suggested_hotkey: "7",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "intro_sting",
        display_name: "Intro sting",
        category: "music",
        icon_name: "wave-sine",
        suggested_hotkey: "8",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "brb_loop",
        display_name: "BRB loop",
        category: "music",
        icon_name: "repeat",
        suggested_hotkey: "9",
        loop_playback: true,
    },
    BuiltinSoundEntry {
        builtin_id: "outro",
        display_name: "Outro",
        category: "music",
        icon_name: "wave-saw-tool",
        suggested_hotkey: "0",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "applause",
        display_name: "Applause",
        category: "voice",
        icon_name: "hand-click",
        suggested_hotkey: "Q",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "drumroll",
        display_name: "Drumroll",
        category: "voice",
        icon_name: "ripple",
        suggested_hotkey: "W",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "sad_violin",
        display_name: "Sad violin",
        category: "memes",
        icon_name: "mood-sad",
        suggested_hotkey: "E",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "boo",
        display_name: "Boo",
        category: "voice",
        icon_name: "ghost",
        suggested_hotkey: "R",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "ding",
        display_name: "Ding",
        category: "alerts",
        icon_name: "bell",
        suggested_hotkey: "T",
        loop_playback: false,
    },
    BuiltinSoundEntry {
        builtin_id: "tada",
        display_name: "Tada",
        category: "alerts",
        icon_name: "confetti",
        suggested_hotkey: "Y",
        loop_playback: false,
    },
];

/// Resolves a builtin's audio file on disk, trying each supported extension in
/// order. `None` if the maintainer has not supplied a file for this slot yet.
pub fn resolve_builtin_path(data_dir: &Path, builtin_id: &str) -> Option<PathBuf> {
    let dir = data_dir.join("soundboard").join("builtin");
    BUILTIN_FILE_EXTENSIONS.iter().find_map(|ext| {
        let candidate = dir.join(format!("{builtin_id}.{ext}"));
        candidate.is_file().then_some(candidate)
    })
}

/// Every catalog entry paired with whether its audio file is present on disk.
pub fn builtin_availability(data_dir: &Path) -> Vec<(BuiltinSoundEntry, bool)> {
    BUILTIN_SOUNDS
        .iter()
        .map(|entry| {
            let present = resolve_builtin_path(data_dir, entry.builtin_id).is_some();
            (*entry, present)
        })
        .collect()
}
