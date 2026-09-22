use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, youtube_actor};
use crate::payload_fields::{chat as chat_fields, support as fields};

pub(crate) struct SupportSuperChatDescriptor;

impl TriggerKindDescriptor for SupportSuperChatDescriptor {
    fn id(&self) -> &str {
        "youtube.chat.super_chat"
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

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::YouTube),
            kind_prefix: Some("youtube.chat.super_chat".to_owned()),
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
                        name: "amount_micros".to_owned(),
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
                        name: "currency".to_owned(),
                        kind: VariantKind::String,
                        label: "Currency code".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::CURRENCY)),
                )
                .legacy(
                    DeclaredVariable {
                        name: "user_display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                    |event| Variant::String(author_identity(event).display_name),
                )
                .legacy(
                    DeclaredVariable {
                        name: "channel_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender channel ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| Variant::String(author_identity(event).id),
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
#[allow(clippy::unwrap_used)]
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
    fn always_matches() {
        assert!(
            SupportSuperChatDescriptor.matches_trigger(&TriggerConfig::new(), &super_chat_event())
        );
    }

    #[test]
    fn build_arg_stack_extracts_super_chat_fields() {
        let stack = SupportSuperChatDescriptor.build_arg_stack(&super_chat_event());
        assert_eq!(
            stack.get("user_display_name"),
            Some(&Variant::String("BigFan".to_owned()))
        );
        assert_eq!(stack.get("amount_micros"), Some(&Variant::Int(5_000_000)));
        assert_eq!(
            stack.get("currency"),
            Some(&Variant::String("USD".to_owned()))
        );
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("Great stream!".to_owned()))
        );
    }
}
