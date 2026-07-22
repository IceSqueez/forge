use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

pub(crate) struct ChannelBanDescriptor;

impl TriggerKindDescriptor for ChannelBanDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.ban"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "User banned (permanent)"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer is permanently banned from the channel"
    }

    fn search_text(&self) -> &str {
        "twitch ban banned permanent moderation"
    }

    fn icon_name(&self) -> &str {
        "ban"
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

    // channel.ban fires for both bans and timeouts; only fire for permanent bans here.
    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event
            .payload
            .get("is_permanent")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user = event.payload.get("user");
        let moderator = event.payload.get("moderator");

        let user_login = user
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = user
            .and_then(|u| u.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_name = user
            .and_then(|u| u.get("display_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let moderator_login = moderator
            .and_then(|m| m.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let reason = event
            .payload
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let banned_at = event
            .payload
            .get("banned_at")
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
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "user_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Banned user login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "user_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Banned user ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "user_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Banned user display name".to_owned(),
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
                        label: "Ban reason".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "banned_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Banned at".to_owned(),
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
    use crate::triggers::channel_timeout::ChannelTimeoutDescriptor;

    fn ban_event(is_permanent: bool) -> Event {
        let payload = serde_json::json!({
            "user": { "id": "777", "login": "viewer_one", "display_name": "ViewerOne" },
            "moderator": { "login": "mod_jane", "display_name": "ModJane" },
            "reason": "spamming",
            "banned_at": "2026-06-13T10:00:00Z",
            "ends_at": "2026-06-13T10:10:00Z",
            "is_permanent": is_permanent,
        });
        Event::new(EventSource::Twitch, "channel.ban", payload)
    }

    #[test]
    fn is_permanent_routes_ban_and_timeout_to_opposite_descriptors() {
        let cfg = TriggerConfig::new();

        let permanent = ban_event(true);
        assert!(
            ChannelBanDescriptor.matches_trigger(&cfg, &permanent),
            "permanent ban must fire the ban descriptor"
        );
        assert!(
            !ChannelTimeoutDescriptor.matches_trigger(&cfg, &permanent),
            "permanent ban must NOT fire the timeout descriptor"
        );

        let timeout = ban_event(false);
        assert!(
            !ChannelBanDescriptor.matches_trigger(&cfg, &timeout),
            "timeout must NOT fire the ban descriptor"
        );
        assert!(
            ChannelTimeoutDescriptor.matches_trigger(&cfg, &timeout),
            "timeout must fire the timeout descriptor"
        );
    }

    #[test]
    fn missing_is_permanent_defaults_to_timeout_not_ban() {
        let cfg = TriggerConfig::new();
        let payload = serde_json::json!({
            "user": { "id": "1", "login": "x", "display_name": "X" },
            "moderator": { "login": "m", "display_name": "M" },
        });
        let event = Event::new(EventSource::Twitch, "channel.ban", payload);
        assert!(!ChannelBanDescriptor.matches_trigger(&cfg, &event));
        assert!(ChannelTimeoutDescriptor.matches_trigger(&cfg, &event));
    }

    #[test]
    fn event_filter_targets_channel_ban_topic_from_twitch() {
        let filter = ChannelBanDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("channel.ban"));
    }

    #[test]
    fn build_arg_stack_exposes_user_moderator_reason_and_banned_at() {
        let stack = ChannelBanDescriptor.build_arg_stack(&ban_event(true));
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("viewer_one".to_owned()))
        );
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("777".to_owned()))
        );
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("ViewerOne".to_owned()))
        );
        assert_eq!(
            stack.get("moderator_login"),
            Some(&Variant::String("mod_jane".to_owned()))
        );
        assert_eq!(
            stack.get("reason"),
            Some(&Variant::String("spamming".to_owned()))
        );
        assert_eq!(
            stack.get("banned_at"),
            Some(&Variant::String("2026-06-13T10:00:00Z".to_owned()))
        );
        assert_eq!(stack.get("ends_at"), None);
    }
}
