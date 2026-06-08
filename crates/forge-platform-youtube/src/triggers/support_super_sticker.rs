use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct SupportSuperStickerDescriptor;

impl TriggerKindDescriptor for SupportSuperStickerDescriptor {
    fn id(&self) -> &str {
        "youtube.support.super_sticker"
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
            kind_prefix: Some("youtube.support.super_sticker".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user_display_name = event
            .payload
            .get("user_display_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let sticker_id = event
            .payload
            .get("sticker_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let amount_micros = event
            .payload
            .get("amount_micros")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let currency = event
            .payload
            .get("currency")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "user_display_name".to_owned(),
                Variant::String(user_display_name),
            )
            .set("sticker_id".to_owned(), Variant::String(sticker_id))
            .set("amount_micros".to_owned(), Variant::Int(amount_micros))
            .set("currency".to_owned(), Variant::String(currency))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn super_sticker_event() -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.support.super_sticker",
            serde_json::json!({
                "user_display_name": "StickerFan",
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
