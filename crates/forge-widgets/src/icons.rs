pub use iced_fonts::{BOOTSTRAP_FONT, BOOTSTRAP_FONT_BYTES};

pub const ICON_HOME: char = '\u{F425}';
pub const ICON_CLOCK: char = '\u{F293}';
pub const ICON_LIGHTNING: char = '\u{F46F}';
pub const ICON_TERMINAL: char = '\u{F5C3}';
pub const ICON_BROADCAST: char = '\u{F1D6}';
pub const ICON_GRID: char = '\u{F3FC}';
pub const ICON_SPEAKER: char = '\u{F57E}';
pub const ICON_MUSIC_NOTE: char = '\u{F4A0}';
pub const ICON_FILE_CODE: char = '\u{F352}';
pub const ICON_SERVER: char = '\u{F52C}';
pub const ICON_JOURNAL: char = '\u{F446}';
pub const ICON_GEAR: char = '\u{F3E5}';
pub const ICON_PEOPLE: char = '\u{F4D0}';
pub const ICON_CHAT: char = '\u{F268}';
pub const ICON_GLOBE: char = '\u{F3EE}';
pub const ICON_ACTIVITY: char = '\u{F66B}';
pub const ICON_HASH: char = '\u{F40A}';
pub const ICON_CHEVRON_DOWN: char = '\u{F282}';
pub const ICON_CHEVRON_UP: char = '\u{F286}';
pub const ICON_CHEVRON_RIGHT: char = '\u{F285}';
pub const ICON_DOWNLOAD: char = '\u{F30A}';
pub const ICON_PLUS: char = '\u{F4FE}';
pub const ICON_REPLAY: char = '\u{F116}';
pub const ICON_EYE: char = '\u{F341}';
pub const ICON_EYE_SLASH: char = '\u{F344}';
pub const ICON_COPY: char = '\u{F28C}';
pub const ICON_REFRESH: char = '\u{F130}';
pub const ICON_ALERT_TRIANGLE: char = '\u{F33B}';
pub const ICON_LOCK: char = '\u{F470}';
pub const ICON_X: char = '\u{F62C}';
pub const ICON_CHECK_CIRCLE: char = '\u{F26D}';
pub const ICON_INFO_CIRCLE: char = '\u{F431}';
pub const ICON_KEYBOARD: char = '\u{F459}';
pub const ICON_FOLDER: char = '\u{F3D7}';
pub const ICON_FOLDER_OPEN: char = '\u{F3D8}';
pub const ICON_FILE_IMAGE: char = '\u{F39B}';
pub const ICON_EXTERNAL_LINK: char = '\u{F1C5}';
pub const ICON_PAUSE: char = '\u{F4C4}';
pub const ICON_PLAY: char = '\u{F4F4}';
pub const ICON_ERASER: char = '\u{F331}';
pub const ICON_DOTS_VERTICAL: char = '\u{F5D3}';
pub const ICON_CIRCLE_DASHED: char = '\u{F2E6}';
pub const ICON_LOADER: char = '\u{F116}';

/// Maps an icon name (Tabler-style kebab-case) to its bundled Bootstrap-font
/// codepoint. Returns `ICON_INFO_CIRCLE` for unknown names so missing icons
/// render as a recognizable placeholder rather than as several broken
/// replacement glyphs.
pub fn bootstrap_icon_for(name: &str) -> char {
    match name {
        "home" => ICON_HOME,
        "clock" => ICON_CLOCK,
        "bolt" | "lightning" => ICON_LIGHTNING,
        "terminal" => ICON_TERMINAL,
        "broadcast" => ICON_BROADCAST,
        "grid" | "apps" | "layout-grid" => ICON_GRID,
        "speaker" | "volume" => ICON_SPEAKER,
        "music" => ICON_MUSIC_NOTE,
        "file-code" => ICON_FILE_CODE,
        "server" => ICON_SERVER,
        "journal" | "logs" => ICON_JOURNAL,
        "gear" | "settings" => ICON_GEAR,
        "people" | "users" => ICON_PEOPLE,
        "chat" | "message-circle" => ICON_CHAT,
        "globe" => ICON_GLOBE,
        "activity" => ICON_ACTIVITY,
        "hash" | "variable" => ICON_HASH,
        "chevron-down" => ICON_CHEVRON_DOWN,
        "chevron-up" => ICON_CHEVRON_UP,
        "chevron-right" => ICON_CHEVRON_RIGHT,
        "download" => ICON_DOWNLOAD,
        "plus" => ICON_PLUS,
        "replay" => ICON_REPLAY,
        "eye" => ICON_EYE,
        "eye-slash" | "eye-off" => ICON_EYE_SLASH,
        "copy" => ICON_COPY,
        "refresh" | "arrows-shuffle" | "loader" | "loader-2" => ICON_REFRESH,
        "alert-triangle" | "warning" => ICON_ALERT_TRIANGLE,
        "lock" | "key" => ICON_LOCK,
        "x" | "close" => ICON_X,
        "check" | "check-circle" | "shield-check" => ICON_CHECK_CIRCLE,
        "info-circle" | "info" | "list" => ICON_INFO_CIRCLE,
        "keyboard" => ICON_KEYBOARD,
        "folder" => ICON_FOLDER,
        "folder-open" => ICON_FOLDER_OPEN,
        "file-image" | "image" | "camera" => ICON_FILE_IMAGE,
        "external-link" | "external" => ICON_EXTERNAL_LINK,
        "pause" | "player-pause" => ICON_PAUSE,
        "play" | "player-play" => ICON_PLAY,
        "eraser" => ICON_ERASER,
        "dots-vertical" => ICON_DOTS_VERTICAL,
        "circle-dashed" => ICON_CIRCLE_DASHED,
        "device-desktop" | "layout" | "stack-2" | "record" => ICON_BROADCAST,
        "brand-twitch" | "twitch" | "brand-obs" | "obs" => ICON_BROADCAST,
        "rss" => ICON_ACTIVITY,
        "send" => ICON_CHEVRON_RIGHT,
        "flag" => ICON_ALERT_TRIANGLE,
        "edit" | "pencil" => ICON_INFO_CIRCLE,
        _ => ICON_INFO_CIRCLE,
    }
}
