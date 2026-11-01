use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, TriggerCategory, TriggerVariables,
};
use forge_types::{DeclaredVariable, SynthesisHint, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::automatic_reward as automatic_reward_fields;

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), redeemer_identity)
            .event_specific(
                DeclaredVariable {
                    name: "redemption.id".to_owned(),
                    kind: VariantKind::String,
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
                    synthesis: Some(SynthesisHint::BoundedInt {
                        min: 0,
                        max: super::MAX_SYNTHESIZED_COUNT,
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

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_POINTS_AUTOMATIC_REWARD_REDEEMED,
    category: TriggerCategory::ChannelPoints,
    label: "Automatic reward redeemed",
    summary: "Fires when a viewer redeems a built-in automatic channel point reward",
    search_text: "twitch channel points automatic reward redeemed built-in highlight message",
    icon: "star",
    kind: event_kinds::CHANNEL_CHANNEL_POINTS_AUTOMATIC_REWARD_REDEMPTION_ADD,
    actors: ActorDeclaration::principal(),
    variables,
    ..TwitchTriggerSpec::BASE
};

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
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

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
    fn build_arg_stack_types_reward_cost_as_int_and_maps_identity_fields() {
        let stack = SPEC.build_arg_stack(&automatic_reward_event());
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
        let stack = SPEC.build_arg_stack(&event);
        assert_eq!(stack.get("reward.cost"), Some(&Variant::Int(0)));
    }
}
