use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct SupportSuperChatDescriptor;

impl TriggerKindDescriptor for SupportSuperChatDescriptor {
    fn id(&self) -> &str {
        "youtube.support.super_chat"
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
            kind_prefix: Some("youtube.support.super_chat".to_owned()),
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
        let message_text = event
            .payload
            .get("message_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "user_display_name".to_owned(),
                Variant::String(user_display_name),
            )
            .set("amount_micros".to_owned(), Variant::Int(amount_micros))
            .set("currency".to_owned(), Variant::String(currency))
            .set("message_text".to_owned(), Variant::String(message_text))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn super_chat_event() -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.support.super_chat",
            serde_json::json!({
                "user_display_name": "BigFan",
                "amount_micros": 5000000,
                "currency": "USD",
                "message_text": "Great stream!"
            }),
        )
    }

    #[test]
    fn kind_id_matches_canonical() {
        assert_eq!(
            SupportSuperChatDescriptor.id(),
            "youtube.support.super_chat"
        );
    }

    #[test]
    fn is_platform_specific_youtube() {
        assert_eq!(
            SupportSuperChatDescriptor.event_filter().source,
            Some(EventSource::YouTube)
        );
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
