use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalVariable, EventFilter,
    FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, youtube_actor};
use crate::vocabulary::fields::{chat as chat_fields, support as fields};
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct SupportSuperChatDescriptor;

impl TriggerKindDescriptor for SupportSuperChatDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CHAT_SUPER_CHAT
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Bits
    }

    fn label(&self) -> &str {
        "Super Chat"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer sends a Super Chat in YouTube live chat"
    }

    fn search_text(&self) -> &str {
        "youtube super chat donation money amount currency support"
    }

    fn icon_name(&self) -> &str {
        "currency-dollar"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::YouTube)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::YouTube),
            kind_prefix: Some(event_kinds::CHAT_SUPER_CHAT.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(youtube_actor(ActorRole::Principal), author_identity)
                .message_text(|event| payload_read::text(event, chat_fields::MESSAGE_TEXT))
                .event_specific(
                    DeclaredVariable {
                        name: variables::support::AMOUNT_MICROS.to_owned(),
                        kind: VariantKind::Int,
                        label: "Amount in micros".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt {
                            min: 2_000_000,
                            max: 500_000_000,
                        }),
                    },
                    |event| Variant::Int(payload_read::number(event, fields::AMOUNT_MICROS)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: variables::support::CURRENCY.to_owned(),
                        kind: VariantKind::String,
                        label: "Currency code".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::CURRENCY)),
                )
                .legacy(
                    variables::sender::DISPLAY_NAME,
                    "Sender display name",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                )
                .legacy(
                    variables::sender::CHANNEL_ID,
                    "Sender channel ID",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn author_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(chat_fields::AUTHOR))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn super_chat_event() -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.chat.super_chat",
            serde_json::json!({
                "author": { "display_name": "BigFan", "channel_id": "UCbigfan" },
                "amount_micros": 5000000,
                "currency": "USD",
                "message_text": "Great stream!"
            }),
        )
    }

    #[test]
    fn a_super_chat_publishes_its_sender_its_note_and_the_money_it_carries() {
        let stack = SupportSuperChatDescriptor.build_arg_stack(&super_chat_event());
        for (name, value) in [
            ("user_id", "UCbigfan"),
            ("user_name", "BigFan"),
            ("message_text", "Great stream!"),
            ("currency", "USD"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
        assert_eq!(stack.get("amount_micros"), Some(&Variant::Int(5_000_000)));
    }
}
