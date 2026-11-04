use forge_types::{MessageCatalog, MessageCode};

pub(crate) const NAMESPACE: &str = "overlay";

pub(crate) const ALERT_LABEL: MessageCode = MessageCode::from_static(NAMESPACE, "kind_alert");
pub(crate) const ALERT_SUMMARY: MessageCode =
    MessageCode::from_static(NAMESPACE, "kind_alert_summary");
pub(crate) const BLANK_LABEL: MessageCode = MessageCode::from_static(NAMESPACE, "kind_blank");
pub(crate) const BLANK_SUMMARY: MessageCode =
    MessageCode::from_static(NAMESPACE, "kind_blank_summary");
pub(crate) const CARD_LABEL: MessageCode = MessageCode::from_static(NAMESPACE, "kind_card");
pub(crate) const CARD_SUMMARY: MessageCode =
    MessageCode::from_static(NAMESPACE, "kind_card_summary");
pub(crate) const CHAT_LABEL: MessageCode = MessageCode::from_static(NAMESPACE, "kind_chat");
pub(crate) const CHAT_SUMMARY: MessageCode =
    MessageCode::from_static(NAMESPACE, "kind_chat_summary");
pub(crate) const FRAME_LABEL: MessageCode = MessageCode::from_static(NAMESPACE, "kind_frame");
pub(crate) const FRAME_SUMMARY: MessageCode =
    MessageCode::from_static(NAMESPACE, "kind_frame_summary");
pub(crate) const GOAL_LABEL: MessageCode = MessageCode::from_static(NAMESPACE, "kind_goal");
pub(crate) const GOAL_SUMMARY: MessageCode =
    MessageCode::from_static(NAMESPACE, "kind_goal_summary");
pub(crate) const LATEST_LABEL: MessageCode = MessageCode::from_static(NAMESPACE, "kind_latest");
pub(crate) const LATEST_SUMMARY: MessageCode =
    MessageCode::from_static(NAMESPACE, "kind_latest_summary");
pub(crate) const TICKER_LABEL: MessageCode = MessageCode::from_static(NAMESPACE, "kind_ticker");
pub(crate) const TICKER_SUMMARY: MessageCode =
    MessageCode::from_static(NAMESPACE, "kind_ticker_summary");

pub(crate) const CHAT_EVENT_FOLLOW: MessageCode =
    MessageCode::from_static(NAMESPACE, "chat_event_follow");
pub(crate) const CHAT_EVENT_SUBSCRIPTION: MessageCode =
    MessageCode::from_static(NAMESPACE, "chat_event_subscription");
pub(crate) const CHAT_EVENT_RESUBSCRIPTION: MessageCode =
    MessageCode::from_static(NAMESPACE, "chat_event_resubscription");
pub(crate) const CHAT_EVENT_GIFT_SUBSCRIPTION: MessageCode =
    MessageCode::from_static(NAMESPACE, "chat_event_gift_subscription");
pub(crate) const CHAT_EVENT_CHEER: MessageCode =
    MessageCode::from_static(NAMESPACE, "chat_event_cheer");
pub(crate) const CHAT_EVENT_RAID: MessageCode =
    MessageCode::from_static(NAMESPACE, "chat_event_raid");
pub(crate) const CHAT_EVENT_DONATION: MessageCode =
    MessageCode::from_static(NAMESPACE, "chat_event_donation");
pub(crate) const CHAT_EVENT_SUPER_CHAT: MessageCode =
    MessageCode::from_static(NAMESPACE, "chat_event_super_chat");
pub(crate) const CHAT_EVENT_SUPER_STICKER: MessageCode =
    MessageCode::from_static(NAMESPACE, "chat_event_super_sticker");
pub(crate) const CHAT_EVENT_MEMBERSHIP: MessageCode =
    MessageCode::from_static(NAMESPACE, "chat_event_membership");
pub(crate) const CHAT_EVENT_MEMBERSHIP_GIFT: MessageCode =
    MessageCode::from_static(NAMESPACE, "chat_event_membership_gift");

macro_rules! catalog {
    ($($icon:ident => $name:literal),* $(,)?) => {
        $(pub(crate) const $icon: MessageCode = MessageCode::from_static(NAMESPACE, $name);)*

        pub const OVERLAY_MESSAGES: MessageCatalog = MessageCatalog {
            namespace: NAMESPACE,
            codes: &[
        ALERT_LABEL,
        ALERT_SUMMARY,
        BLANK_LABEL,
        BLANK_SUMMARY,
        CARD_LABEL,
        CARD_SUMMARY,
        CHAT_LABEL,
        CHAT_SUMMARY,
        FRAME_LABEL,
        FRAME_SUMMARY,
        GOAL_LABEL,
        GOAL_SUMMARY,
        LATEST_LABEL,
        LATEST_SUMMARY,
        TICKER_LABEL,
        TICKER_SUMMARY,
        CHAT_EVENT_FOLLOW,
        CHAT_EVENT_SUBSCRIPTION,
        CHAT_EVENT_RESUBSCRIPTION,
        CHAT_EVENT_GIFT_SUBSCRIPTION,
        CHAT_EVENT_CHEER,
        CHAT_EVENT_RAID,
        CHAT_EVENT_DONATION,
        CHAT_EVENT_SUPER_CHAT,
        CHAT_EVENT_SUPER_STICKER,
        CHAT_EVENT_MEMBERSHIP,
        CHAT_EVENT_MEMBERSHIP_GIFT,
                $($icon,)*
            ],
            en_ftl: include_str!("../locales/en/messages.ftl"),
            uk_ftl: include_str!("../locales/uk/messages.ftl"),
        };
    };
}

catalog! {
    ICON_HEART => "icon_heart",
    ICON_HEART_FILLED => "icon_heart_filled",
    ICON_HEART_HANDSHAKE => "icon_heart_handshake",
    ICON_HEARTS => "icon_hearts",
    ICON_GIFT => "icon_gift",
    ICON_GIFT_CARD => "icon_gift_card",
    ICON_CROWN => "icon_crown",
    ICON_DIAMOND => "icon_diamond",
    ICON_COIN => "icon_coin",
    ICON_COINS => "icon_coins",
    ICON_CASH => "icon_cash",
    ICON_MONEYBAG => "icon_moneybag",
    ICON_PIG_MONEY => "icon_pig_money",
    ICON_RECEIPT => "icon_receipt",
    ICON_WALLET => "icon_wallet",
    ICON_AWARD => "icon_award",
    ICON_MEDAL => "icon_medal",
    ICON_MEDAL_2 => "icon_medal_2",
    ICON_TROPHY => "icon_trophy",
    ICON_ROSETTE => "icon_rosette",
    ICON_CERTIFICATE => "icon_certificate",
    ICON_TICKET => "icon_ticket",
    ICON_THUMB_UP => "icon_thumb_up",
    ICON_STAR => "icon_star",
    ICON_STAR_FILLED => "icon_star_filled",
    ICON_STARS => "icon_stars",
    ICON_SPARKLES => "icon_sparkles",
    ICON_CONFETTI => "icon_confetti",
    ICON_BALLOON => "icon_balloon",
    ICON_CAKE => "icon_cake",
    ICON_CANDLE => "icon_candle",
    ICON_GLASS_CHAMPAGNE => "icon_glass_champagne",
    ICON_GLASS_COCKTAIL => "icon_glass_cocktail",
    ICON_LAUREL_WREATH => "icon_laurel_wreath",
    ICON_COMET => "icon_comet",
    ICON_RAINBOW => "icon_rainbow",
    ICON_CANDY => "icon_candy",
    ICON_LOLLIPOP => "icon_lollipop",
    ICON_MOOD_HAPPY => "icon_mood_happy",
    ICON_MOOD_SMILE => "icon_mood_smile",
    ICON_MOOD_CRAZY_HAPPY => "icon_mood_crazy_happy",
    ICON_MOOD_TONGUE => "icon_mood_tongue",
    ICON_MOOD_HEART => "icon_mood_heart",
    ICON_MOOD_WINK => "icon_mood_wink",
    ICON_MOOD_CRY => "icon_mood_cry",
    ICON_MOOD_SURPRISED => "icon_mood_surprised",
    ICON_MOOD_NERD => "icon_mood_nerd",
    ICON_MOOD_KID => "icon_mood_kid",
    ICON_USER => "icon_user",
    ICON_USER_PLUS => "icon_user_plus",
    ICON_USER_CHECK => "icon_user_check",
    ICON_USER_HEART => "icon_user_heart",
    ICON_USER_STAR => "icon_user_star",
    ICON_USERS => "icon_users",
    ICON_USERS_GROUP => "icon_users_group",
    ICON_FRIENDS => "icon_friends",
    ICON_MESSAGE => "icon_message",
    ICON_MESSAGE_CIRCLE => "icon_message_circle",
    ICON_MESSAGE_2 => "icon_message_2",
    ICON_MESSAGES => "icon_messages",
    ICON_MESSAGE_HEART => "icon_message_heart",
    ICON_MESSAGE_STAR => "icon_message_star",
    ICON_SPEAKERPHONE => "icon_speakerphone",
    ICON_BELL => "icon_bell",
    ICON_BELL_RINGING => "icon_bell_ringing",
    ICON_BELL_HEART => "icon_bell_heart",
    ICON_AT => "icon_at",
    ICON_HASH => "icon_hash",
    ICON_EYE => "icon_eye",
    ICON_FLAG => "icon_flag",
    ICON_PIN => "icon_pin",
    ICON_BOOKMARK => "icon_bookmark",
    ICON_BROADCAST => "icon_broadcast",
    ICON_ANTENNA => "icon_antenna",
    ICON_VIDEO => "icon_video",
    ICON_CAMERA => "icon_camera",
    ICON_MICROPHONE => "icon_microphone",
    ICON_MICROPHONE_2 => "icon_microphone_2",
    ICON_HEADPHONES => "icon_headphones",
    ICON_HEADSET => "icon_headset",
    ICON_MUSIC => "icon_music",
    ICON_PLAYLIST => "icon_playlist",
    ICON_VINYL => "icon_vinyl",
    ICON_RADIO => "icon_radio",
    ICON_DEVICE_TV => "icon_device_tv",
    ICON_DEVICE_GAMEPAD_2 => "icon_device_gamepad_2",
    ICON_DEVICE_GAMEPAD => "icon_device_gamepad",
    ICON_MOVIE => "icon_movie",
    ICON_PHOTO => "icon_photo",
    ICON_PLAYER_PLAY => "icon_player_play",
    ICON_PLAYER_PAUSE => "icon_player_pause",
    ICON_PLAYER_RECORD => "icon_player_record",
    ICON_VOLUME => "icon_volume",
    ICON_SCREEN_SHARE => "icon_screen_share",
    ICON_SWORD => "icon_sword",
    ICON_SWORDS => "icon_swords",
    ICON_SHIELD => "icon_shield",
    ICON_SHIELD_STAR => "icon_shield_star",
    ICON_TARGET => "icon_target",
    ICON_TARGET_ARROW => "icon_target_arrow",
    ICON_DICE => "icon_dice",
    ICON_DICE_5 => "icon_dice_5",
    ICON_PUZZLE => "icon_puzzle",
    ICON_CHESS => "icon_chess",
    ICON_CHESS_KNIGHT => "icon_chess_knight",
    ICON_CARDS => "icon_cards",
    ICON_GHOST => "icon_ghost",
    ICON_SKULL => "icon_skull",
    ICON_ALIEN => "icon_alien",
    ICON_ROBOT => "icon_robot",
    ICON_WAND => "icon_wand",
    ICON_CRYSTAL_BALL => "icon_crystal_ball",
    ICON_FLASK => "icon_flask",
    ICON_POO => "icon_poo",
    ICON_UFO => "icon_ufo",
    ICON_PLANET => "icon_planet",
    ICON_METEOR => "icon_meteor",
    ICON_BOMB => "icon_bomb",
    ICON_FLAME => "icon_flame",
    ICON_BOLT => "icon_bolt",
    ICON_ROCKET => "icon_rocket",
    ICON_FLAG_3 => "icon_flag_3",
    ICON_CHART_BAR => "icon_chart_bar",
    ICON_CHART_LINE => "icon_chart_line",
    ICON_TRENDING_UP => "icon_trending_up",
    ICON_PROGRESS => "icon_progress",
    ICON_HOURGLASS => "icon_hourglass",
    ICON_CLOCK => "icon_clock",
    ICON_ALARM => "icon_alarm",
    ICON_STOPWATCH => "icon_stopwatch",
    ICON_CALENDAR => "icon_calendar",
    ICON_CALENDAR_EVENT => "icon_calendar_event",
    ICON_CHECKLIST => "icon_checklist",
    ICON_CIRCLE_CHECK => "icon_circle_check",
    ICON_CIRCLE_X => "icon_circle_x",
    ICON_CHECK => "icon_check",
    ICON_X => "icon_x",
    ICON_PLUS => "icon_plus",
    ICON_INFO_CIRCLE => "icon_info_circle",
    ICON_ALERT_TRIANGLE => "icon_alert_triangle",
    ICON_LOCK => "icon_lock",
    ICON_KEY => "icon_key",
    ICON_SUN => "icon_sun",
    ICON_MOON => "icon_moon",
    ICON_CLOUD => "icon_cloud",
    ICON_SNOWFLAKE => "icon_snowflake",
    ICON_LEAF => "icon_leaf",
    ICON_FLOWER => "icon_flower",
    ICON_CLOVER => "icon_clover",
    ICON_PAW => "icon_paw",
    ICON_CAT => "icon_cat",
    ICON_DOG => "icon_dog",
    ICON_FISH => "icon_fish",
    ICON_BUTTERFLY => "icon_butterfly",
    ICON_FEATHER => "icon_feather",
    ICON_COFFEE => "icon_coffee",
    ICON_PIZZA => "icon_pizza",
    ICON_COOKIE => "icon_cookie",
    ICON_BEER => "icon_beer",
    ICON_ICE_CREAM => "icon_ice_cream",
    ICON_APPLE => "icon_apple",
    ICON_WORLD => "icon_world",
    ICON_GLOBE => "icon_globe",
    ICON_HOME => "icon_home",
    ICON_MAP_PIN => "icon_map_pin",
    ICON_COMPASS => "icon_compass",
}
