use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::automatic_reward as automatic_reward_fields;

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
            kind_prefix: Some(
                "twitch.channel.channel_points_automatic_reward_redemption.add".to_owned(),
            ),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), redeemer_identity)
                .event_specific(
                    DeclaredVariable {
                        name: "redemption.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Redemption ID".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            automatic_reward_fields::REDEMPTION,
                            automatic_reward_fields::REDEMPTION_ID,
                        ))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: "reward.type".to_owned(),
                        kind: VariantKind::String,
                        label: "Reward type".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            automatic_reward_fields::REWARD,
                            automatic_reward_fields::REWARD_TYPE,
                        ))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: "reward.cost".to_owned(),
                        kind: VariantKind::Int,
                        label: "Reward cost".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt {
                            min: 0,
                            max: 1000000,
                        }),
                    },
                    |event| {
                        Variant::Int(payload_read::nested_number(
                            event,
                            automatic_reward_fields::REWARD,
                            automatic_reward_fields::REWARD_COST,
                        ))
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn redeemer_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(automatic_reward_fields::USER),
        automatic_reward_fields::USER_ID,
        automatic_reward_fields::USER_LOGIN,
        automatic_reward_fields::USER_DISPLAY_NAME,
    )
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
            Some("twitch.channel.channel_points_automatic_reward_redemption.add")
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
