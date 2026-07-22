use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::warning as warning_fields;

pub(crate) struct WarningAcknowledgedDescriptor;

impl TriggerKindDescriptor for WarningAcknowledgedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.warning_acknowledged"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Warning acknowledged"
    }

    fn summary(&self) -> &str {
        "Fires when a user acknowledges a warning issued by a moderator"
    }

    fn search_text(&self) -> &str {
        "twitch warning acknowledged user moderation"
    }

    fn icon_name(&self) -> &str {
        "bell-check"
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
            kind_prefix: Some("channel.warning.acknowledge".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user = event.payload.get(warning_fields::USER);

        let user_login = user
            .and_then(|v| v.get(warning_fields::USER_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = user
            .and_then(|v| v.get(warning_fields::USER_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_name = user
            .and_then(|v| v.get(warning_fields::USER_DISPLAY_NAME))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("user_name".to_owned(), Variant::String(user_name))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "user_login".to_owned(),
                        kind: VariantKind::String,
                        label: "User login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "user_id".to_owned(),
                        kind: VariantKind::String,
                        label: "User ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "user_name".to_owned(),
                        kind: VariantKind::String,
                        label: "User display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warning_acknowledged_event() -> Event {
        let payload = serde_json::json!({
            "user": { "id": "654", "login": "warned_user", "display_name": "WarnedUser" },
        });
        Event::new(EventSource::Twitch, "channel.warning.acknowledge", payload)
    }

    #[test]
    fn event_filter_targets_warning_acknowledge_topic_from_twitch() {
        let filter = WarningAcknowledgedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.warning.acknowledge")
        );
    }

    #[test]
    fn build_arg_stack_maps_user_fields_from_nested_payload() {
        let stack = WarningAcknowledgedDescriptor.build_arg_stack(&warning_acknowledged_event());
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("warned_user".to_owned()))
        );
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("654".to_owned()))
        );
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("WarnedUser".to_owned()))
        );
    }
}
