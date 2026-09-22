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

pub(crate) struct SupportSuperStickerDescriptor;

impl TriggerKindDescriptor for SupportSuperStickerDescriptor {
    fn id(&self) -> &str {
        "youtube.chat.super_sticker"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Bits
    }

    fn label(&self) -> &str {
        "Super Sticker"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer sends a Super Sticker in YouTube live chat"
    }

    fn search_text(&self) -> &str {
        "youtube super sticker donation amount currency support"
    }

    fn icon_name(&self) -> &str {
        "star"
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
            kind_prefix: Some("youtube.chat.super_sticker".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(youtube_actor(ActorRole::Principal), author_identity)
                .event_specific(
                    DeclaredVariable {
                        name: "sticker_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Sticker ID".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::STICKER_ID)),
                )
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

    fn super_sticker_event() -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.chat.super_sticker",
            serde_json::json!({
                "author": { "display_name": "StickerFan", "channel_id": "UCsticker" },
                "sticker_id": "sticker_abc_123",
                "amount_micros": 2000000,
                "currency": "EUR"
            }),
        )
    }

    #[test]
    fn always_matches() {
        assert!(
            SupportSuperStickerDescriptor
                .matches_trigger(&TriggerConfig::new(), &super_sticker_event())
        );
    }

    #[test]
    fn build_arg_stack_extracts_sticker_fields() {
        let stack = SupportSuperStickerDescriptor.build_arg_stack(&super_sticker_event());
        assert_eq!(
            stack.get("user_display_name"),
            Some(&Variant::String("StickerFan".to_owned()))
        );
        assert_eq!(
            stack.get("sticker_id"),
            Some(&Variant::String("sticker_abc_123".to_owned()))
        );
        assert_eq!(stack.get("amount_micros"), Some(&Variant::Int(2_000_000)));
        assert_eq!(
            stack.get("currency"),
            Some(&Variant::String("EUR".to_owned()))
        );
    }
}
