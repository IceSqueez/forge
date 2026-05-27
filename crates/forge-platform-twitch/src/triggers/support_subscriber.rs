use forge_events::{Event, EventSource};
use forge_registry::{EventFilter, FormField, TriggerCategory, TriggerKindDescriptor};
use forge_types::{ArgStack, Trigger, TriggerConfig, Variant};

pub(crate) struct SupportSubscriberDescriptor;

impl TriggerKindDescriptor for SupportSubscriberDescriptor {
    fn id(&self) -> &str {
        "twitch.support.subscriber"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Subscriptions
    }

    fn label(&self) -> &str {
        "New subscriber"
    }

    fn summary(&self) -> &str {
        "Fires on a first-time channel subscription"
    }

    fn search_text(&self) -> &str {
        "twitch subscribe subscriber new subscription tier"
    }

    fn icon_name(&self) -> &str {
        "user-plus"
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
            kind_prefix: Some("channel.subscribe".to_owned()),
        }
    }

    fn matches_trigger(&self, _trigger: &Trigger, _event: &Event) -> bool {
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
        let is_gift = event
            .payload
            .get("is_gift")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        ArgStack::new()
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("sub_tier".to_owned(), Variant::String(tier))
            .set("sub_is_gift".to_owned(), Variant::Bool(is_gift))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_types::{ActionId, TriggerId};

    fn make_trigger() -> Trigger {
        Trigger {
            id: TriggerId::new(),
            action_id: ActionId::new(),
            kind_id: "twitch.support.subscriber".to_owned(),
            config: TriggerConfig::new(),
        }
    }

    fn subscribe_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.subscribe",
            serde_json::json!({
                "user": { "id": "111", "login": "newbie", "display_name": "Newbie" },
                "tier": "1000",
                "is_gift": false
            }),
        )
    }

    #[test]
    fn id_is_stable() {
        assert_eq!(
            SupportSubscriberDescriptor.id(),
            "twitch.support.subscriber"
        );
    }

    #[test]
    fn always_matches() {
        let trigger = make_trigger();
        assert!(SupportSubscriberDescriptor.matches_trigger(&trigger, &subscribe_event()));
    }

    #[test]
    fn condition_display_is_any() {
        assert_eq!(
            SupportSubscriberDescriptor.condition_display(&TriggerConfig::new()),
            "any"
        );
    }

    #[test]
    fn build_arg_stack_extracts_sub_fields() {
        let stack = SupportSubscriberDescriptor.build_arg_stack(&subscribe_event());
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("newbie".to_owned()))
        );
        assert_eq!(
            stack.get("sub_tier"),
            Some(&Variant::String("1000".to_owned()))
        );
        assert_eq!(stack.get("sub_is_gift"), Some(&Variant::Bool(false)));
    }
}
