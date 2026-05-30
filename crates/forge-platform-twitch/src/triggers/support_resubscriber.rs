use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct SupportResubscriberDescriptor;

impl TriggerKindDescriptor for SupportResubscriberDescriptor {
    fn id(&self) -> &str {
        "twitch.support.resubscriber"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Subscriptions
    }

    fn label(&self) -> &str {
        "Re-subscriber"
    }

    fn summary(&self) -> &str {
        "Fires on subscription renewals"
    }

    fn search_text(&self) -> &str {
        "twitch resub resubscribe renewal subscription months streak"
    }

    fn icon_name(&self) -> &str {
        "repeat"
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
            kind_prefix: Some("channel.subscription.message".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user_login = event
            .payload
            .get("user")
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = event
            .payload
            .get("user")
            .and_then(|u| u.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let tier = event
            .payload
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let cumulative_months = event
            .payload
            .get("cumulative_months")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let streak_months = event
            .payload
            .get("streak_months")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let message = event
            .payload
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("sub_tier".to_owned(), Variant::String(tier))
            .set(
                "sub_cumulative_months".to_owned(),
                Variant::Int(cumulative_months),
            )
            .set("sub_streak_months".to_owned(), Variant::Int(streak_months))
            .set("sub_message".to_owned(), Variant::String(message))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn resub_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.subscription.message",
            serde_json::json!({
                "user": { "id": "222", "login": "loyalfan", "display_name": "LoyalFan" },
                "tier": "1000",
                "cumulative_months": 12,
                "streak_months": 6,
                "message": "Love this channel!",
                "share_streak": true
            }),
        )
    }

    #[test]
    fn id_is_stable() {
        assert_eq!(
            SupportResubscriberDescriptor.id(),
            "twitch.support.resubscriber"
        );
    }

    #[test]
    fn always_matches() {
        assert!(
            SupportResubscriberDescriptor.matches_trigger(&TriggerConfig::new(), &resub_event())
        );
    }

    #[test]
    fn build_arg_stack_extracts_resub_fields() {
        let stack = SupportResubscriberDescriptor.build_arg_stack(&resub_event());
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("loyalfan".to_owned()))
        );
        assert_eq!(stack.get("sub_cumulative_months"), Some(&Variant::Int(12)));
        assert_eq!(stack.get("sub_streak_months"), Some(&Variant::Int(6)));
        assert_eq!(
            stack.get("sub_message"),
            Some(&Variant::String("Love this channel!".to_owned()))
        );
    }
}
