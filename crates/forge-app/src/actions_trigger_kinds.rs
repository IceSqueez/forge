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
    pub fn display_name(&self) -> String {
        match self {
            TriggerCategory::Chat => forge_widgets::tr!("actions_cat_chat_commands"),
            TriggerCategory::Subscriptions => forge_widgets::tr!("actions_cat_subs_bits"),
            TriggerCategory::Bits => forge_widgets::tr!("actions_cat_bits"),
            TriggerCategory::Raids => forge_widgets::tr!("actions_cat_raids"),
            TriggerCategory::Obs => forge_widgets::tr!("actions_cat_obs_events"),
            TriggerCategory::Server => forge_widgets::tr!("actions_cat_server_events"),
            TriggerCategory::Timer => forge_widgets::tr!("actions_cat_timers"),
            TriggerCategory::Ungrouped => forge_widgets::tr!("actions_cat_ungrouped"),
            TriggerCategory::All => forge_widgets::tr!("actions_cat_all"),
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
        "twitch.chat.command" => forge_widgets::tr!("actions_kind_twitch_chat_command"),
        "twitch.chat.message" => forge_widgets::tr!("actions_kind_twitch_chat_message"),
        "twitch.support.subscriber" => forge_widgets::tr!("actions_kind_twitch_subscriber"),
        "twitch.support.resubscriber" => forge_widgets::tr!("actions_kind_twitch_resubscriber"),
        "twitch.support.gift_sub" => forge_widgets::tr!("actions_kind_twitch_gift_sub"),
        "twitch.support.cheer" => forge_widgets::tr!("actions_kind_twitch_cheer"),
        "twitch.channel.raid_received" => forge_widgets::tr!("actions_kind_twitch_raid"),
        "obs.scenes.current_changed" => forge_widgets::tr!("actions_kind_obs_scene_changed"),
        "script.event.custom" => forge_widgets::tr!("actions_kind_server_custom_event"),
        other => other.to_string(),
    }
}

pub fn kind_label(kind_id: &str) -> String {
    match kind_id {
        "twitch.chat.command" => forge_widgets::tr!("actions_kind_twitch_chat_command"),
        "twitch.chat.message" => forge_widgets::tr!("actions_kind_twitch_chat_message"),
        "twitch.support.subscriber" => forge_widgets::tr!("actions_kind_twitch_subscriber"),
        "twitch.support.resubscriber" => forge_widgets::tr!("actions_kind_twitch_resubscriber"),
        "twitch.support.gift_sub" => forge_widgets::tr!("actions_kind_twitch_gift_sub"),
        "twitch.support.cheer" => forge_widgets::tr!("actions_kind_twitch_cheer"),
        "twitch.channel.raid_received" => forge_widgets::tr!("actions_kind_twitch_raid"),
        "obs.scenes.current_changed" => forge_widgets::tr!("actions_kind_obs_scene_changed"),
        "script.event.custom" => forge_widgets::tr!("actions_kind_server_custom_event"),
        _ => forge_widgets::tr!("actions_kind_unknown"),
    }
}

pub fn kind_summary(kind_id: &str) -> String {
    match kind_id {
        "twitch.chat.command" => forge_widgets::tr!("actions_summary_twitch_chat_command"),
        "twitch.chat.message" => forge_widgets::tr!("actions_summary_twitch_chat_message"),
        "twitch.support.subscriber" => forge_widgets::tr!("actions_summary_twitch_subscriber"),
        "twitch.support.resubscriber" => forge_widgets::tr!("actions_summary_twitch_resubscriber"),
        "twitch.support.gift_sub" => forge_widgets::tr!("actions_summary_twitch_gift_sub"),
        "twitch.support.cheer" => forge_widgets::tr!("actions_summary_twitch_cheer"),
        "twitch.channel.raid_received" => forge_widgets::tr!("actions_summary_twitch_raid"),
        "obs.scenes.current_changed" => {
            forge_widgets::tr!("actions_summary_obs_scene_changed")
        }
        "script.event.custom" => {
            forge_widgets::tr!("actions_summary_server_custom_event")
        }
        _ => String::new(),
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
