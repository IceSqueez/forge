use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, kick_actor};
use crate::payload_fields::reward as fields;

pub(crate) struct RewardRedeemedDescriptor;

impl TriggerKindDescriptor for RewardRedeemedDescriptor {
    fn id(&self) -> &str {
        "kick.channel.reward.redemption.updated"
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
            kind_prefix: Some("kick.channel.reward.redemption.updated".to_owned()),
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
                        name: "redemption_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Redemption ID".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::ID)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "reward_id".to_owned(),
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
                        name: "reward_title".to_owned(),
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
                    DeclaredVariable {
                        name: "username".to_owned(),
                        kind: VariantKind::String,
                        label: "Redeeming username".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                    |event| Variant::String(payload_read::login_of(redeemer_identity(event))),
                )
                .legacy(
                    DeclaredVariable {
                        name: "user_input".to_owned(),
                        kind: VariantKind::String,
                        label: "User input text".to_owned(),
                        synthesis: Some(SynthesisHint::Message),
                    },
                    CanonicalVariable::MessageText,
                    |event| Variant::String(payload_read::text(event, fields::USER_INPUT)),
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
#[allow(clippy::unwrap_used)]
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
        assert_eq!(stack.get("user_input"), stack.get("message_text"));
        assert_eq!(stack.get("username"), stack.get("user_login"));
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
