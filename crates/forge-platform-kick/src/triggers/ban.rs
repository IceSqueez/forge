use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::{entity, moderation as fields};

pub(crate) struct BanDescriptor;

impl TriggerKindDescriptor for BanDescriptor {
    fn id(&self) -> &str {
        "kick.moderation.banned"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Users
    }

    fn label(&self) -> &str {
        "User banned"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator bans a user in Kick chat"
    }

    fn search_text(&self) -> &str {
        "kick ban timeout moderator user removed"
    }

    fn icon_name(&self) -> &str {
        "user-x"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
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
            source: Some(EventSource::Kick),
            kind_prefix: Some("kick.moderation.banned".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let banned_user = event.payload.get(fields::BANNED_USER);
        let banned_user_id = banned_user
            .and_then(|u| u.get(entity::ID))
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());
        let banned_username = banned_user
            .and_then(|u| u.get(entity::USERNAME))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let duration_secs = event
            .payload
            .get(fields::DURATION_SECS)
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());

        let reason = event
            .payload
            .get(fields::REASON)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("banned_user_id".to_owned(), Variant::String(banned_user_id))
            .set(
                "banned_username".to_owned(),
                Variant::String(banned_username),
            )
            .set("duration_secs".to_owned(), Variant::String(duration_secs))
            .set("reason".to_owned(), Variant::String(reason))
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "banned_user_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Banned user ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "banned_username".to_owned(),
                    kind: VariantKind::String,
                    label: "Banned username".to_owned(),
                    synthesis: Some(SynthesisHint::Username),
                },
                DeclaredVariable {
                    name: "duration_secs".to_owned(),
                    kind: VariantKind::String,
                    label: "Ban duration (seconds)".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "reason".to_owned(),
                    kind: VariantKind::String,
                    label: "Ban reason".to_owned(),
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

    fn ban_event() -> Event {
        Event::new(
            EventSource::Kick,
            "kick.moderation.banned",
            serde_json::json!({
                "banned_user": { "id": 77, "username": "bad_actor" },
                "moderator": { "id": 2, "username": "mod" },
                "is_permanent": false,
                "duration_secs": 300,
                "reason": null
            }),
        )
    }

    #[test]
    fn build_arg_stack_extracts_ban_fields() {
        let stack = BanDescriptor.build_arg_stack(&ban_event());
        assert_eq!(
            stack.get("banned_user_id"),
            Some(&Variant::String("77".to_owned()))
        );
        assert_eq!(
            stack.get("banned_username"),
            Some(&Variant::String("bad_actor".to_owned()))
        );
        assert_eq!(
            stack.get("duration_secs"),
            Some(&Variant::String("300".to_owned()))
        );
    }
}
