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

#[cfg(test)]
mod tests {
    use super::*;

    fn automatic_reward_event() -> Event {
        let payload = serde_json::json!({
            "redemption": { "id": "redeem-1" },
            "user": { "id": "42", "login": "viewer_one" },
            "reward": { "type": "send_highlighted_message", "cost": 300 },
        });
        Event::new(
            EventSource::Twitch,
            "channel.channel_points_automatic_reward_redemption",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_automatic_reward_topic_from_twitch() {
        let filter = AutomaticRewardRedeemedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.channel_points_automatic_reward_redemption")
        );
    }

    #[test]
    fn build_arg_stack_types_reward_cost_as_int_and_maps_identity_fields() {
        let stack = AutomaticRewardRedeemedDescriptor.build_arg_stack(&automatic_reward_event());
        assert_eq!(stack.get("reward.cost"), Some(&Variant::Int(300)));
        assert_eq!(
            stack.get("reward.type"),
            Some(&Variant::String("send_highlighted_message".to_owned()))
        );
        assert_eq!(
            stack.get("redemption.id"),
            Some(&Variant::String("redeem-1".to_owned()))
        );
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("viewer_one".to_owned()))
        );
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("42".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_defaults_missing_cost_to_zero() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.channel_points_automatic_reward_redemption",
            serde_json::json!({}),
        );
        let stack = AutomaticRewardRedeemedDescriptor.build_arg_stack(&event);
        assert_eq!(stack.get("reward.cost"), Some(&Variant::Int(0)));
    }
}
