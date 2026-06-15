use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct SupportMemberMilestoneDescriptor;

impl TriggerKindDescriptor for SupportMemberMilestoneDescriptor {
    fn id(&self) -> &str {
        "youtube.channel.member_milestone"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Subscriptions
    }

    fn label(&self) -> &str {
        "Member milestone"
    }

    fn summary(&self) -> &str {
        "Fires when a YouTube channel member reaches a membership milestone"
    }

    fn search_text(&self) -> &str {
        "youtube member milestone anniversary months subscription streak"
    }

    fn icon_name(&self) -> &str {
        "award"
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
            kind_prefix: Some("youtube.channel.member_milestone".to_owned()),
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
        let member_month = event
            .payload
            .get("member_month")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
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
            .set("member_month".to_owned(), Variant::Int(member_month))
            .set("message_text".to_owned(), Variant::String(message_text))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn milestone_event() -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.channel.member_milestone",
            serde_json::json!({
                "user_display_name": "LongTimeFan",
                "member_month": 12,
                "message_text": "One year!"
            }),
        )
    }

    #[test]
    fn always_matches() {
        assert!(
            SupportMemberMilestoneDescriptor
                .matches_trigger(&TriggerConfig::new(), &milestone_event())
        );
    }

    #[test]
    fn build_arg_stack_extracts_milestone_fields() {
        let stack = SupportMemberMilestoneDescriptor.build_arg_stack(&milestone_event());
        assert_eq!(
            stack.get("user_display_name"),
            Some(&Variant::String("LongTimeFan".to_owned()))
        );
        assert_eq!(stack.get("member_month"), Some(&Variant::Int(12)));
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("One year!".to_owned()))
        );
    }
}
