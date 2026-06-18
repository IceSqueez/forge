use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct RewardRedeemedDescriptor;

impl TriggerKindDescriptor for RewardRedeemedDescriptor {
    fn id(&self) -> &str {
        "kick.channel.reward_redeemed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Reward redeemed"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer redeems a Kick channel reward"
    }

    fn search_text(&self) -> &str {
        "kick reward redeem channel points redemption gift viewer"
    }

    fn icon_name(&self) -> &str {
        "gift"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
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
            source: Some(EventSource::Kick),
            kind_prefix: Some("kick.channel.reward_redeemed".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let redemption_id = event
            .payload
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let reward = event.payload.get("reward");
        let reward_id = reward
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let reward_title = reward
            .and_then(|r| r.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let redeemer = event.payload.get("redeemer");
        let user_id = redeemer
            .and_then(|r| r.get("user_id"))
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());
        let username = redeemer
            .and_then(|r| r.get("username"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let user_input = event
            .payload
            .get("user_input")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("redemption_id".to_owned(), Variant::String(redemption_id))
            .set("reward_id".to_owned(), Variant::String(reward_id))
            .set("reward_title".to_owned(), Variant::String(reward_title))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("username".to_owned(), Variant::String(username))
            .set("user_input".to_owned(), Variant::String(user_input))
    }
}
