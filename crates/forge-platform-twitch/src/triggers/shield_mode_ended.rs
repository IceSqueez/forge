use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::shield as shield_fields;

pub(crate) struct ShieldModeEndedDescriptor;

impl TriggerKindDescriptor for ShieldModeEndedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.shield_mode_ended"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Shield mode ended"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator deactivates shield mode in the channel"
    }

    fn search_text(&self) -> &str {
        "twitch shield mode protection moderation ended deactivated"
    }

    fn icon_name(&self) -> &str {
        "shield-off"
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
            kind_prefix: Some("channel.shield_mode.end".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let moderator = event.payload.get(shield_fields::MODERATOR);

        let moderator_login = moderator
            .and_then(|m| m.get(shield_fields::MODERATOR_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let moderator_id = moderator
            .and_then(|m| m.get(shield_fields::MODERATOR_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let ended_at = event
            .payload
            .get(shield_fields::ENDED_AT)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "moderator_login".to_owned(),
                Variant::String(moderator_login),
            )
            .set("moderator_id".to_owned(), Variant::String(moderator_id))
            .set("ended_at".to_owned(), Variant::String(ended_at))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "moderator_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "moderator_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "ended_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Ended at".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shield_end_event() -> Event {
        let payload = serde_json::json!({
            "moderator": { "id": "42", "login": "mod_jane", "display_name": "ModJane" },
            "ended_at": "2026-06-13T19:00:00Z",
        });
        Event::new(EventSource::Twitch, "channel.shield_mode.end", payload)
    }

    #[test]
    fn event_filter_targets_shield_mode_end_topic_from_twitch() {
        let filter = ShieldModeEndedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.shield_mode.end")
        );
    }

    #[test]
    fn build_arg_stack_maps_moderator_fields_and_ended_at() {
        let stack = ShieldModeEndedDescriptor.build_arg_stack(&shield_end_event());
        assert_eq!(
            stack.get("moderator_login"),
            Some(&Variant::String("mod_jane".to_owned()))
        );
        assert_eq!(
            stack.get("moderator_id"),
            Some(&Variant::String("42".to_owned()))
        );
        assert_eq!(
            stack.get("ended_at"),
            Some(&Variant::String("2026-06-13T19:00:00Z".to_owned()))
        );
    }
}
