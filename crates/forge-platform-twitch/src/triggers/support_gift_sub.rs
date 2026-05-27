use forge_events::{Event, EventSource};
use forge_registry::{EventFilter, FormField, TriggerCategory, TriggerKindDescriptor};
use forge_types::{ArgStack, Trigger, TriggerConfig, Variant};

pub(crate) struct SupportGiftSubDescriptor;

impl TriggerKindDescriptor for SupportGiftSubDescriptor {
    fn id(&self) -> &str {
        "twitch.support.gift_sub"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Subscriptions
    }

    fn label(&self) -> &str {
        "Gift subscription"
    }

    fn summary(&self) -> &str {
        "Fires when a subscription is gifted to another user"
    }

    fn search_text(&self) -> &str {
        "twitch gift sub gifted subscription recipient"
    }

    fn icon_name(&self) -> &str {
        "gift"
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
            kind_prefix: Some("channel.subscription.gift".to_owned()),
        }
    }

    fn matches_trigger(&self, _trigger: &Trigger, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let gifter_login = event
            .payload
            .get("gifter")
            .and_then(|g| g.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let gifter_id = event
            .payload
            .get("gifter")
            .and_then(|g| g.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let is_anonymous = event
            .payload
            .get("is_anonymous")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let recipient_login = event
            .payload
            .get("recipient")
            .and_then(|r| r.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let recipient_id = event
            .payload
            .get("recipient")
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let tier = event
            .payload
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("gifter_login".to_owned(), Variant::String(gifter_login))
            .set("gifter_id".to_owned(), Variant::String(gifter_id))
            .set("gifter_is_anonymous".to_owned(), Variant::Bool(is_anonymous))
            .set(
                "recipient_login".to_owned(),
                Variant::String(recipient_login),
            )
            .set("recipient_id".to_owned(), Variant::String(recipient_id))
            .set("sub_tier".to_owned(), Variant::String(tier))
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
            kind_id: "twitch.support.gift_sub".to_owned(),
            config: TriggerConfig::new(),
        }
    }

    fn gift_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.subscription.gift",
            serde_json::json!({
                "tier": "1000",
                "is_anonymous": false,
                "gifter": { "id": "333", "login": "generous_viewer", "display_name": "GenerousViewer", "total": 5 },
                "recipient": { "id": "444", "login": "lucky_one", "display_name": "LuckyOne" }
            }),
        )
    }

    #[test]
    fn id_is_stable() {
        assert_eq!(SupportGiftSubDescriptor.id(), "twitch.support.gift_sub");
    }

    #[test]
    fn always_matches() {
        let trigger = make_trigger();
        assert!(SupportGiftSubDescriptor.matches_trigger(&trigger, &gift_event()));
    }

    #[test]
    fn build_arg_stack_extracts_gift_fields() {
        let stack = SupportGiftSubDescriptor.build_arg_stack(&gift_event());
        assert_eq!(
            stack.get("gifter_login"),
            Some(&Variant::String("generous_viewer".to_owned()))
        );
        assert_eq!(
            stack.get("gifter_is_anonymous"),
            Some(&Variant::Bool(false))
        );
        assert_eq!(
            stack.get("recipient_login"),
            Some(&Variant::String("lucky_one".to_owned()))
        );
        assert_eq!(
            stack.get("sub_tier"),
            Some(&Variant::String("1000".to_owned()))
        );
    }
}
