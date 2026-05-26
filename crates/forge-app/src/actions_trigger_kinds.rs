use forge_types::TriggerKind;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TriggerCategory {
    Chat,
    Subscriptions,
    Bits,
    Raids,
    Obs,
    Server,
    Timer,
    Ungrouped,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActionsFilter {
    #[default]
    All,
    Chat,
    Timers,
    Points,
}

impl TriggerCategory {
    pub fn display_name(&self) -> &'static str {
        match self {
            TriggerCategory::Chat => "CHAT COMMANDS",
            TriggerCategory::Subscriptions => "SUBS & BITS",
            TriggerCategory::Bits => "BITS",
            TriggerCategory::Raids => "RAIDS",
            TriggerCategory::Obs => "OBS EVENTS",
            TriggerCategory::Server => "SERVER EVENTS",
            TriggerCategory::Timer => "TIMERS",
            TriggerCategory::Ungrouped => "UNGROUPED",
            TriggerCategory::All => "ALL",
        }
    }
}

pub fn category_of(kind: &TriggerKind) -> TriggerCategory {
    match kind {
        TriggerKind::TwitchChatCommand | TriggerKind::TwitchChatAnyMessage => TriggerCategory::Chat,
        TriggerKind::TwitchSubscribe
        | TriggerKind::TwitchResubscribe
        | TriggerKind::TwitchGiftSub => TriggerCategory::Subscriptions,
        TriggerKind::TwitchCheer => TriggerCategory::Bits,
        TriggerKind::TwitchRaid => TriggerCategory::Raids,
        TriggerKind::ObsSceneChanged { .. } => TriggerCategory::Obs,
        TriggerKind::CodeEvent { .. } => TriggerCategory::Server,
    }
}

pub fn trigger_label_of(kind: &TriggerKind) -> String {
    match kind {
        TriggerKind::TwitchChatCommand => "Twitch \u{00b7} chat command".to_string(),
        TriggerKind::TwitchChatAnyMessage => "Twitch \u{00b7} any chat message".to_string(),
        TriggerKind::TwitchSubscribe => "Twitch \u{00b7} new subscriber".to_string(),
        TriggerKind::TwitchResubscribe => "Twitch \u{00b7} re-subscribe".to_string(),
        TriggerKind::TwitchGiftSub => "Twitch \u{00b7} gift subs".to_string(),
        TriggerKind::TwitchCheer => "Twitch \u{00b7} bits cheered".to_string(),
        TriggerKind::TwitchRaid => "Twitch \u{00b7} raid received".to_string(),
        TriggerKind::ObsSceneChanged { .. } => "OBS \u{00b7} scene changed".to_string(),
        TriggerKind::CodeEvent { .. } => "Server \u{00b7} custom event".to_string(),
    }
}

pub fn kind_label(kind: &TriggerKind) -> &'static str {
    match kind {
        TriggerKind::TwitchChatCommand => "Twitch \u{00b7} Chat command",
        TriggerKind::TwitchChatAnyMessage => "Twitch \u{00b7} Any chat message",
        TriggerKind::TwitchSubscribe => "Twitch \u{00b7} New subscriber",
        TriggerKind::TwitchResubscribe => "Twitch \u{00b7} Re-subscribe",
        TriggerKind::TwitchGiftSub => "Twitch \u{00b7} Gift subs",
        TriggerKind::TwitchCheer => "Twitch \u{00b7} Bits cheered",
        TriggerKind::TwitchRaid => "Twitch \u{00b7} Raid received",
        TriggerKind::ObsSceneChanged { .. } => "OBS \u{00b7} Scene changed",
        TriggerKind::CodeEvent { .. } => "Server \u{00b7} Custom event",
    }
}

pub fn kind_summary(kind: &TriggerKind) -> &'static str {
    match kind {
        TriggerKind::TwitchChatCommand => "User types !command in chat",
        TriggerKind::TwitchChatAnyMessage => "Every chat message fires this",
        TriggerKind::TwitchSubscribe => "Fires when someone subscribes",
        TriggerKind::TwitchResubscribe => "Existing sub renews",
        TriggerKind::TwitchGiftSub => "Someone gifts subs to channel",
        TriggerKind::TwitchCheer => "Viewer sends bits",
        TriggerKind::TwitchRaid => "Another stream raids you",
        TriggerKind::ObsSceneChanged { .. } => "Fires when OBS switches the active scene",
        TriggerKind::CodeEvent { .. } => {
            "Fires when triggerCodeEvent is called via the WebSocket API"
        }
    }
}

pub fn kind_search_text(kind: &TriggerKind) -> &'static str {
    match kind {
        TriggerKind::TwitchChatCommand => "twitch chat command !command",
        TriggerKind::TwitchChatAnyMessage => "twitch chat any message all",
        TriggerKind::TwitchSubscribe => "twitch subscribe subscriber sub new",
        TriggerKind::TwitchResubscribe => "twitch resubscribe resub renew",
        TriggerKind::TwitchGiftSub => "twitch gift sub giftsub gifted",
        TriggerKind::TwitchCheer => "twitch cheer bits cheered donate",
        TriggerKind::TwitchRaid => "twitch raid incoming raided",
        TriggerKind::ObsSceneChanged { .. } => "obs scene changed obsscenechanged",
        TriggerKind::CodeEvent { .. } => "server code event custom overlay api trigger",
    }
}

pub fn all_trigger_kinds() -> [TriggerKind; 9] {
    [
        TriggerKind::TwitchChatCommand,
        TriggerKind::TwitchChatAnyMessage,
        TriggerKind::TwitchSubscribe,
        TriggerKind::TwitchResubscribe,
        TriggerKind::TwitchGiftSub,
        TriggerKind::TwitchCheer,
        TriggerKind::TwitchRaid,
        TriggerKind::ObsSceneChanged { scene: None },
        TriggerKind::CodeEvent {
            name: String::new(),
        },
    ]
}
