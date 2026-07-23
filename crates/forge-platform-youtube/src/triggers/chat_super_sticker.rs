use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::{chat as chat_fields, entity, support as fields};

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

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let author = event.payload.get(chat_fields::AUTHOR);
        let user_display_name = author
            .and_then(|a| a.get(entity::DISPLAY_NAME))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let channel_id = author
            .and_then(|a| a.get(entity::CHANNEL_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let sticker_id = event
            .payload
            .get(fields::STICKER_ID)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let amount_micros = event
            .payload
            .get(fields::AMOUNT_MICROS)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let currency = event
            .payload
            .get(fields::CURRENCY)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "user_display_name".to_owned(),
                Variant::String(user_display_name),
            )
            .set("channel_id".to_owned(), Variant::String(channel_id))
            .set("sticker_id".to_owned(), Variant::String(sticker_id))
            .set("amount_micros".to_owned(), Variant::Int(amount_micros))
            .set("currency".to_owned(), Variant::String(currency))
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "user_display_name".to_owned(),
                    kind: VariantKind::String,
                    label: "Sender display name".to_owned(),
                    synthesis: Some(SynthesisHint::DisplayName),
                },
                DeclaredVariable {
                    name: "channel_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Sender channel ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "sticker_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Sticker ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "amount_micros".to_owned(),
                    kind: VariantKind::Int,
                    label: "Amount in micros".to_owned(),
                    synthesis: Some(SynthesisHint::BoundedInt {
                        min: 2_000_000,
                        max: 500_000_000,
                    }),
                },
                DeclaredVariable {
                    name: "currency".to_owned(),
                    kind: VariantKind::String,
                    label: "Currency code".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
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
