pub const HEADLINE: &str = "headline";
pub const SUBLINE: &str = "subline";
pub const ACCENT: &str = "accent";
pub const FONT: &str = "font";
pub const POSITION: &str = "position";
pub const DESIGN_WIDTH: &str = "design_width";
pub const DESIGN_HEIGHT: &str = "design_height";
pub const MARGIN_TOP: &str = "margin_top";
pub const MARGIN_RIGHT: &str = "margin_right";
pub const MARGIN_BOTTOM: &str = "margin_bottom";
pub const MARGIN_LEFT: &str = "margin_left";
pub const TEXT_SIZE: &str = "text_size";
pub const DURATION: &str = "duration";
pub const SOUND: &str = "sound";
pub const SPEECH: &str = "speech";
pub(crate) const SPEECH_KEYS: [&str; 2] = [SPEECH, SPEECH_VOICE];
pub const SPEECH_VOICE: &str = "speech_voice";
pub const SHOW: &str = "show";
pub const ICON: &str = "icon";

pub const AUTHOR: &str = "author";
pub const AUTHOR_COLOR: &str = "author_color";
pub const BADGES: &str = "badges";
pub const MESSAGE: &str = "message";

pub const LABEL: &str = "label";
pub const VALUE: &str = "value";
pub const TARGET: &str = "target";

pub const SLOT: &str = "slot";
pub const PLATFORM: &str = "platform";
pub const PLACEHOLDER: &str = "placeholder";

pub const CLIP_ID: &str = "clip_id";
pub const CLIP_PATH: &str = "clip_path";
pub const REPORT_PATH: &str = "report_path";
pub const CLIP_MEDIA_TYPE: &str = "clip_media_type";
pub const CLIP_DURATION_MS: &str = "clip_duration_ms";
pub const COMMAND: &str = "command";

pub const SOUND_OPTIONS_KEY: &str = forge_registry::OVERLAY_SOUND_OPTIONS_KEY;
pub const ICON_OPTIONS_KEY: &str = forge_registry::OVERLAY_ICON_OPTIONS_KEY;

pub const DEFAULT_ICON: &str = "star-filled";

pub const ACCENT_MAUVE: &str = "mauve";
pub const ACCENT_SKY: &str = "sky";
pub const ACCENT_GREEN: &str = "green";
pub const ACCENT_PEACH: &str = "peach";
pub const ACCENT_YELLOW: &str = "yellow";
pub const ACCENT_RED: &str = "red";
pub const ACCENT_OPTIONS: &[&str] = &[
    ACCENT_MAUVE,
    ACCENT_SKY,
    ACCENT_GREEN,
    ACCENT_PEACH,
    ACCENT_YELLOW,
    ACCENT_RED,
];

pub const FONT_INTER: &str = "Inter";
pub const FONT_JETBRAINS_MONO: &str = "JetBrains Mono";
pub const FONT_RUBIK: &str = "Rubik";
pub const FONT_BEBAS_NEUE: &str = "Bebas Neue";
pub const FONT_OPTIONS: &[&str] = &[FONT_INTER, FONT_JETBRAINS_MONO, FONT_RUBIK, FONT_BEBAS_NEUE];

pub const POSITION_TOP: &str = "top";
pub const POSITION_BOTTOM: &str = "bottom";
pub const POSITION_OPTIONS: &[&str] = &[POSITION_TOP, POSITION_BOTTOM];

pub const DURATION_MIN_SECS: i64 = 1;
pub const DURATION_MAX_SECS: i64 = 15;

pub const DESIGN_SIZE_MIN_PX: i64 = 40;
pub const DESIGN_SIZE_MAX_PX: i64 = 7680;

pub const MARGIN_MIN_PERCENT: i64 = 0;
pub const MARGIN_MAX_PERCENT: i64 = 45;

pub const MARGIN_SIDES: [&str; 4] = [MARGIN_TOP, MARGIN_RIGHT, MARGIN_BOTTOM, MARGIN_LEFT];

pub const TEXT_SIZE_MIN_PX: i64 = 8;
pub const TEXT_SIZE_MAX_PX: i64 = 200;

pub const CLIP_DURATION_MIN_MS: i64 = 0;
pub const CLIP_DURATION_MAX_MS: i64 = 60 * 60 * 1_000;
