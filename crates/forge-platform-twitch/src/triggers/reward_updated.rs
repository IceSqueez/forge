use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct RewardUpdatedDescriptor;

impl TriggerKindDescriptor for RewardUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel_points.reward_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Channel point reward updated"
    }

    fn summary(&self) -> &str {
        "Fires when an existing channel point reward is modified on the broadcaster's channel"
    }

    fn search_text(&self) -> &str {
        "twitch channel points reward updated changed modified"
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
            kind_prefix: Some("channel.channel_points_custom_reward.update".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let reward = event.payload.get("reward");

        let reward_id = reward
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let title = reward
            .and_then(|r| r.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let cost = reward
            .and_then(|r| r.get("cost"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let prompt = reward
            .and_then(|r| r.get("prompt"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let is_enabled = reward
            .and_then(|r| r.get("is_enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        ArgStack::new()
            .set("reward.id".to_owned(), Variant::String(reward_id))
            .set("reward.title".to_owned(), Variant::String(title))
            .set("reward.cost".to_owned(), Variant::Int(cost))
            .set("reward.prompt".to_owned(), Variant::String(prompt))
            .set("reward.is_enabled".to_owned(), Variant::Bool(is_enabled))
    }
}
