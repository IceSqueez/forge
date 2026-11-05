pub const HEADLINE: &str = "headline";
pub const SUBLINE: &str = "subline";
pub const ACCENT: &str = "accent";
pub const MAIN_TEXT_COLOR: &str = "main_text_color";
pub const TEXT_COLOR: &str = "text_color";
pub const BORDER_COLOR: &str = "border_color";
pub const ACCENT_SPLIT_KEYS: [&str; 2] = [TEXT_COLOR, BORDER_COLOR];
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
pub const BACKGROUND_COLOR: &str = "background_color";
pub const BACKGROUND_OPACITY: &str = "background_opacity";
pub const IMAGE: &str = "image";
pub const IMAGE_POSITION: &str = "image_position";
pub const LABEL_SIDE: &str = "label_side";
pub const LABEL_OFFSET_X: &str = "label_offset_x";
pub const LABEL_OFFSET_Y: &str = "label_offset_y";
pub const IMAGE_HEIGHT: &str = "image_height";
pub const CARD_MIN_WIDTH: &str = "card_min_width";
pub const CARD_MIN_HEIGHT: &str = "card_min_height";
pub const CARD_MAX_WIDTH: &str = "card_max_width";
pub const CARD_MAX_HEIGHT: &str = "card_max_height";
pub const TEXT_OVERFLOW: &str = "text_overflow";

pub const AUTHOR: &str = "author";
pub const AUTHOR_COLOR: &str = "author_color";
pub const BADGES: &str = "badges";
pub const BADGE_IMAGES: &str = "badge_images";
pub const MESSAGE: &str = "message";
pub const MESSAGE_ID: &str = "message_id";
pub const USER_ID: &str = "user_id";
pub const EVENT: &str = "event";
pub const AVATAR: &str = "avatar";
pub const TWITCH_ICON: &str = "twitch_icon";
pub const YOUTUBE_ICON: &str = "youtube_icon";
pub const KICK_ICON: &str = "kick_icon";
pub const PLATFORM_ICON_KEYS: [&str; 3] = [TWITCH_ICON, YOUTUBE_ICON, KICK_ICON];
pub const ROW_WIDTH: &str = "row_width";
pub const SHOW_AVATARS: &str = "show_avatars";
pub const MESSAGE_PARTS: &str = "message_parts";
pub const THIRD_PARTY_EMOTES: &str = "third_party_emotes";
pub const ROW_LIMIT: &str = "row_limit";
pub const HIDE_ROWS: &str = "hide_rows";
pub const HIDE_ROWS_AFTER_SECS: &str = "hide_rows_after_secs";
pub const HIDE_COMMANDS: &str = "hide_commands";
pub const COMMAND_PREFIX: &str = "command_prefix";
pub const HIDDEN_ACCOUNTS: &str = "hidden_accounts";

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
pub const ACCENT_MAUVE_HEX: &str = "#cba6f7";
pub const ACCENT_SKY_HEX: &str = "#89dceb";
pub const ACCENT_GREEN_HEX: &str = "#a6e3a1";
pub const ACCENT_PEACH_HEX: &str = "#fab387";
pub const ACCENT_YELLOW_HEX: &str = "#f9e2af";
pub const ACCENT_RED_HEX: &str = "#f38ba8";
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

pub const IMAGE_POSITION_ABOVE: &str = "above";
pub const IMAGE_POSITION_BELOW: &str = "below";
pub const IMAGE_POSITION_LEFT: &str = "left";
pub const IMAGE_POSITION_RIGHT: &str = "right";
pub const IMAGE_POSITION_OPTIONS: &[&str] = &[
    IMAGE_POSITION_ABOVE,
    IMAGE_POSITION_BELOW,
    IMAGE_POSITION_LEFT,
    IMAGE_POSITION_RIGHT,
];

pub const LABEL_SIDE_LEFT: &str = "left";
pub const LABEL_SIDE_CENTER: &str = "center";
pub const LABEL_SIDE_RIGHT: &str = "right";
pub const LABEL_SIDE_OPTIONS: &[&str] = &[LABEL_SIDE_LEFT, LABEL_SIDE_CENTER, LABEL_SIDE_RIGHT];

pub const LABEL_OFFSET_MIN_PX: i64 = -4000;
pub const LABEL_OFFSET_MAX_PX: i64 = 4000;

pub const DEFAULT_MAIN_TEXT_COLOR: &str = "#ffffff";
pub const MAIN_TEXT_COLOR_OPTIONS: &[&str] = &[
    DEFAULT_MAIN_TEXT_COLOR,
    ACCENT_MAUVE_HEX,
    ACCENT_SKY_HEX,
    ACCENT_GREEN_HEX,
    ACCENT_PEACH_HEX,
    ACCENT_YELLOW_HEX,
    ACCENT_RED_HEX,
];

pub const DEFAULT_BACKGROUND_COLOR: &str = "#14141c";
pub const BACKGROUND_COLOR_OPTIONS: &[&str] = &[
    DEFAULT_BACKGROUND_COLOR,
    ACCENT_MAUVE_HEX,
    ACCENT_SKY_HEX,
    ACCENT_GREEN_HEX,
    ACCENT_PEACH_HEX,
    ACCENT_YELLOW_HEX,
    ACCENT_RED_HEX,
];
pub const BACKGROUND_OPACITY_MIN_PERCENT: i64 = 0;
pub const BACKGROUND_OPACITY_MAX_PERCENT: i64 = 100;
pub const DEFAULT_BACKGROUND_OPACITY_PERCENT: i64 = 86;
pub const DEFAULT_CHAT_BACKGROUND_OPACITY_PERCENT: i64 = 78;

pub const IMAGE_HEIGHT_MIN_PX: i64 = 16;
pub const IMAGE_HEIGHT_MAX_PX: i64 = 480;
pub const DEFAULT_IMAGE_HEIGHT_PX: i64 = 120;

pub const CARD_SIZE_UNBOUNDED_PX: i64 = 0;
pub const CARD_SIZE_MAX_PX: i64 = DESIGN_SIZE_MAX_PX;

pub const TEXT_OVERFLOW_WRAP: &str = "wrap";
pub const TEXT_OVERFLOW_ELLIPSIS: &str = "ellipsis";
pub const TEXT_OVERFLOW_OPTIONS: &[&str] = &[TEXT_OVERFLOW_WRAP, TEXT_OVERFLOW_ELLIPSIS];

pub const DURATION_MIN_SECS: i64 = 1;
pub const DURATION_MAX_SECS: i64 = 15;

pub const ROW_LIMIT_MIN: i64 = 1;
pub const ROW_LIMIT_MAX: i64 = 100;
pub const DEFAULT_ROW_LIMIT: i64 = 15;

pub const HIDE_ROWS_AFTER_MIN_SECS: i64 = 1;
pub const HIDE_ROWS_AFTER_MAX_SECS: i64 = 300;
pub const DEFAULT_HIDE_ROWS_AFTER_SECS: i64 = 30;

pub const DEFAULT_COMMAND_PREFIX: &str = "!";

pub const ROW_WIDTH_FULL: &str = "full";
pub const ROW_WIDTH_FIT: &str = "fit";
pub const ROW_WIDTH_OPTIONS: &[&str] = &[ROW_WIDTH_FULL, ROW_WIDTH_FIT];

pub const DESIGN_SIZE_MIN_PX: i64 = 40;
pub const DESIGN_SIZE_MAX_PX: i64 = 7680;

pub const MARGIN_MIN_PERCENT: i64 = 0;
pub const MARGIN_MAX_PERCENT: i64 = 45;

pub const MARGIN_SIDES: [&str; 4] = [MARGIN_TOP, MARGIN_RIGHT, MARGIN_BOTTOM, MARGIN_LEFT];

pub const TEXT_SIZE_MIN_PX: i64 = 8;
pub const TEXT_SIZE_MAX_PX: i64 = 200;

pub const CLIP_DURATION_MIN_MS: i64 = 0;
pub const CLIP_DURATION_MAX_MS: i64 = 60 * 60 * 1_000;
