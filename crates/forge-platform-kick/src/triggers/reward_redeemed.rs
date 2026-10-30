use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalVariable, EventFilter,
    FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read::{self, kick_actor};
use crate::vocabulary::fields::reward as fields;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct RewardRedeemedDescriptor;

impl TriggerKindDescriptor for RewardRedeemedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CHANNEL_REWARD_REDEMPTION_UPDATED
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

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Kick),
            kind_prefix: Some(event_kinds::CHANNEL_REWARD_REDEMPTION_UPDATED.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(kick_actor(ActorRole::Principal), redeemer_identity)
                .message_text(|event| payload_read::text(event, fields::USER_INPUT))
                .event_specific(
                    DeclaredVariable {
                        name: variables::reward::REDEMPTION_ID.to_owned(),
                        kind: VariantKind::String,
                        label: "Redemption ID".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::ID)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: variables::reward::REWARD_ID.to_owned(),
                        kind: VariantKind::String,
                        label: "Reward ID".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event.payload.get(fields::REWARD),
                            fields::ID,
                        ))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: variables::reward::REWARD_TITLE.to_owned(),
                        kind: VariantKind::String,
                        label: "Reward title".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event.payload.get(fields::REWARD),
                            fields::REWARD_TITLE,
                        ))
                    },
                )
                .legacy(
                    variables::legacy::USERNAME,
                    "Redeeming username",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                )
                .legacy(
                    variables::legacy::USER_INPUT,
                    "User input text",
                    CanonicalVariable::MessageText,
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn redeemer_identity(event: &Event) -> ActorIdentity {
    payload_read::identity_keyed(
        event.payload.get(fields::REDEEMER),
        fields::REDEEMER_USER_ID,
        fields::REDEEMER_USERNAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    fn redemption_event(payload: serde_json::Value) -> Event {
        Event::new(
            EventSource::Kick,
            "kick.channel.reward.redemption.updated",
            payload,
        )
    }

    fn a_redemption_with_a_note() -> Event {
        redemption_event(json!({
            "id": "rdm-1",
            "reward": { "id": "rwd-2", "title": "Hydrate" },
            "redeemer": { "user_id": 123, "username": "v" },
            "user_input": "drink up"
        }))
    }

    #[test]
    fn the_redeemer_is_read_through_the_official_api_key_names() {
        let stack = RewardRedeemedDescriptor.build_arg_stack(&a_redemption_with_a_note());
        for (name, value) in [
            ("user_id", "123"),
            ("user_login", "v"),
            ("redemption_id", "rdm-1"),
            ("reward_id", "rwd-2"),
            ("reward_title", "Hydrate"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
    }

    #[test]
    fn the_user_input_is_published_as_the_canonical_message_text() {
        let stack = RewardRedeemedDescriptor.build_arg_stack(&a_redemption_with_a_note());
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("drink up".to_owned()))
        );
    }

    #[test]
    fn a_redemption_the_wire_attributes_to_nobody_names_an_empty_redeemer() {
        let stack = RewardRedeemedDescriptor.build_arg_stack(&redemption_event(json!({
            "id": "rdm-9",
            "reward": { "id": "rwd-9", "title": "Anonymous reward" },
            "user_input": ""
        })));
        for name in ["user_id", "user_login", "user_name", "message_text"] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(String::new())),
                "'{name}'"
            );
        }
        assert_eq!(
            stack.get("reward_id"),
            Some(&Variant::String("rwd-9".to_owned()))
        );
    }
}
