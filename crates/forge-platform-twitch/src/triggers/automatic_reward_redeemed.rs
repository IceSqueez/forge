use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct AutomaticRewardRedeemedDescriptor;

impl TriggerKindDescriptor for AutomaticRewardRedeemedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel_points.automatic_reward_redeemed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Automatic reward redeemed"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer redeems a built-in automatic channel point reward"
    }

    fn search_text(&self) -> &str {
        "twitch channel points automatic reward redeemed built-in highlight message"
    }

    fn icon_name(&self) -> &str {
        "star"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.channel_points_automatic_reward_redemption".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let redemption = event.payload.get("redemption");
        let user = event.payload.get("user");
        let reward = event.payload.get("reward");

        let redemption_id = redemption
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_login = user
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = user
            .and_then(|u| u.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let reward_type = reward
            .and_then(|r| r.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let reward_cost = reward
            .and_then(|r| r.get("cost"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        ArgStack::new()
            .set("redemption.id".to_owned(), Variant::String(redemption_id))
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("reward.type".to_owned(), Variant::String(reward_type))
            .set("reward.cost".to_owned(), Variant::Int(reward_cost))
    }
}
