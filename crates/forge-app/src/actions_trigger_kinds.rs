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

pub fn category_of(kind_id: &str) -> TriggerCategory {
    match kind_id {
        "twitch.chat.command" | "twitch.chat.message" => TriggerCategory::Chat,
        "twitch.support.subscriber" | "twitch.support.resubscriber" | "twitch.support.gift_sub" => {
            TriggerCategory::Subscriptions
        }
        "twitch.support.cheer" => TriggerCategory::Bits,
        "twitch.channel.raid_received" => TriggerCategory::Raids,
        "obs.scenes.current_changed" => TriggerCategory::Obs,
        "script.event.custom" => TriggerCategory::Server,
        _ => TriggerCategory::Ungrouped,
    }
}

pub fn trigger_label_of(kind_id: &str) -> String {
    match kind_id {
        "twitch.chat.command" => "Twitch \u{00b7} chat command",
        "twitch.chat.message" => "Twitch \u{00b7} any chat message",
        "twitch.support.subscriber" => "Twitch \u{00b7} new subscriber",
        "twitch.support.resubscriber" => "Twitch \u{00b7} re-subscribe",
        "twitch.support.gift_sub" => "Twitch \u{00b7} gift subs",
        "twitch.support.cheer" => "Twitch \u{00b7} bits cheered",
        "twitch.channel.raid_received" => "Twitch \u{00b7} raid received",
        "obs.scenes.current_changed" => "OBS \u{00b7} scene changed",
        "script.event.custom" => "Server \u{00b7} custom event",
        other => other,
    }
    .to_string()
}

pub fn kind_label(kind_id: &str) -> &'static str {
    match kind_id {
        "twitch.chat.command" => "Twitch \u{00b7} Chat command",
        "twitch.chat.message" => "Twitch \u{00b7} Any chat message",
        "twitch.support.subscriber" => "Twitch \u{00b7} New subscriber",
        "twitch.support.resubscriber" => "Twitch \u{00b7} Re-subscribe",
        "twitch.support.gift_sub" => "Twitch \u{00b7} Gift subs",
        "twitch.support.cheer" => "Twitch \u{00b7} Bits cheered",
        "twitch.channel.raid_received" => "Twitch \u{00b7} Raid received",
        "obs.scenes.current_changed" => "OBS \u{00b7} Scene changed",
        "script.event.custom" => "Server \u{00b7} Custom event",
        _ => "Unknown trigger",
    }
}

pub fn kind_summary(kind_id: &str) -> &'static str {
    match kind_id {
        "twitch.chat.command" => "User types !command in chat",
        "twitch.chat.message" => "Every chat message fires this",
        "twitch.support.subscriber" => "Fires when someone subscribes",
        "twitch.support.resubscriber" => "Existing sub renews",
        "twitch.support.gift_sub" => "Someone gifts subs to channel",
        "twitch.support.cheer" => "Viewer sends bits",
        "twitch.channel.raid_received" => "Another stream raids you",
        "obs.scenes.current_changed" => "Fires when OBS switches the active scene",
        "script.event.custom" => "Fires when triggerCodeEvent is called via the WebSocket API",
        _ => "",
    }
}

pub fn kind_search_text(kind_id: &str) -> &'static str {
    match kind_id {
        "twitch.chat.command" => "twitch chat command !command",
        "twitch.chat.message" => "twitch chat any message all",
        "twitch.support.subscriber" => "twitch subscribe subscriber sub new",
        "twitch.support.resubscriber" => "twitch resubscribe resub renew",
        "twitch.support.gift_sub" => "twitch gift sub giftsub gifted",
        "twitch.support.cheer" => "twitch cheer bits cheered donate",
        "twitch.channel.raid_received" => "twitch raid incoming raided",
        "obs.scenes.current_changed" => "obs scene changed obsscenechanged",
        "script.event.custom" => "server code event custom overlay api trigger",
        _ => "",
    }
}

pub fn all_trigger_kind_ids() -> &'static [&'static str] {
    &[
        "twitch.chat.command",
        "twitch.chat.message",
        "twitch.support.subscriber",
        "twitch.support.resubscriber",
        "twitch.support.gift_sub",
        "twitch.support.cheer",
        "twitch.channel.raid_received",
        "obs.scenes.current_changed",
        "script.event.custom",
    ]
}
