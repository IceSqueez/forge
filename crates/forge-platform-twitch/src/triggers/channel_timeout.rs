use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::moderation as fields;

pub(crate) struct ChannelTimeoutDescriptor;

impl TriggerKindDescriptor for ChannelTimeoutDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.timeout"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "User timed out"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer is timed out from the channel"
    }

    fn search_text(&self) -> &str {
        "twitch timeout timed out moderation ban temporary"
    }

    fn icon_name(&self) -> &str {
        "clock"
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
            kind_prefix: Some("channel.ban".to_owned()),
        }
    }

    // channel.ban fires for both bans and timeouts; only fire for timeouts (is_permanent == false).
    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        !event
            .payload
            .get(fields::IS_PERMANENT)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user = event.payload.get(fields::USER);
        let moderator = event.payload.get(fields::MODERATOR);

        let user_login = user
            .and_then(|u| u.get(fields::USER_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = user
            .and_then(|u| u.get(fields::USER_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_name = user
            .and_then(|u| u.get(fields::USER_DISPLAY_NAME))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let moderator_login = moderator
            .and_then(|m| m.get(fields::MODERATOR_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let reason = event
            .payload
            .get(fields::REASON)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let banned_at = event
            .payload
            .get(fields::BANNED_AT)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let ends_at = event
            .payload
            .get(fields::ENDS_AT)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("user_name".to_owned(), Variant::String(user_name))
            .set(
                "moderator_login".to_owned(),
                Variant::String(moderator_login),
            )
            .set("reason".to_owned(), Variant::String(reason))
            .set("banned_at".to_owned(), Variant::String(banned_at))
            .set("ends_at".to_owned(), Variant::String(ends_at))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "user_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Timed-out user login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "user_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Timed-out user ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "user_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Timed-out user display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    DeclaredVariable {
                        name: "moderator_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "reason".to_owned(),
                        kind: VariantKind::String,
                        label: "Timeout reason".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "banned_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Timed out at".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "ends_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Timeout ends at".to_owned(),
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

    fn timeout_event() -> Event {
        let payload = serde_json::json!({
            "user": { "id": "777", "login": "viewer_one", "display_name": "ViewerOne" },
            "moderator": { "login": "mod_jane", "display_name": "ModJane" },
            "reason": "cooldown",
            "banned_at": "2026-06-13T10:00:00Z",
            "ends_at": "2026-06-13T10:10:00Z",
            "is_permanent": false,
        });
        Event::new(EventSource::Twitch, "channel.ban", payload)
    }

    #[test]
    fn event_filter_targets_shared_channel_ban_topic_from_twitch() {
        let filter = ChannelTimeoutDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("channel.ban"));
    }

    #[test]
    fn build_arg_stack_exposes_ends_at_alongside_ban_vars() {
        let stack = ChannelTimeoutDescriptor.build_arg_stack(&timeout_event());
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("viewer_one".to_owned()))
        );
        assert_eq!(
            stack.get("moderator_login"),
            Some(&Variant::String("mod_jane".to_owned()))
        );
        assert_eq!(
            stack.get("reason"),
            Some(&Variant::String("cooldown".to_owned()))
        );
        assert_eq!(
            stack.get("banned_at"),
            Some(&Variant::String("2026-06-13T10:00:00Z".to_owned()))
        );
        assert_eq!(
            stack.get("ends_at"),
            Some(&Variant::String("2026-06-13T10:10:00Z".to_owned()))
        );
    }
}
