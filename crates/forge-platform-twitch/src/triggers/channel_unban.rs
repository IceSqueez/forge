use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

pub(crate) struct ChannelUnbanDescriptor;

impl TriggerKindDescriptor for ChannelUnbanDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.unban"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "User unbanned"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer's ban or timeout is lifted"
    }

    fn search_text(&self) -> &str {
        "twitch unban unbanned moderation pardon"
    }

    fn icon_name(&self) -> &str {
        "check"
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
            kind_prefix: Some("channel.unban".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
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

        ArgStack::new()
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("user_name".to_owned(), Variant::String(user_name))
            .set(
                "moderator_login".to_owned(),
                Variant::String(moderator_login),
            )
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "user_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Unbanned user login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "user_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Unbanned user ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "user_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Unbanned user display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    DeclaredVariable {
                        name: "moderator_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unban_event() -> Event {
        let payload = serde_json::json!({
            "user": { "id": "777", "login": "viewer_one", "display_name": "ViewerOne" },
            "moderator": { "login": "mod_jane", "display_name": "ModJane" },
        });
        Event::new(EventSource::Twitch, "channel.unban", payload)
    }

    #[test]
    fn event_filter_targets_channel_unban_topic_from_twitch() {
        let filter = ChannelUnbanDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("channel.unban"));
    }

    #[test]
    fn build_arg_stack_exposes_user_and_moderator_without_ban_specific_vars() {
        let stack = ChannelUnbanDescriptor.build_arg_stack(&unban_event());
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
        assert_eq!(stack.get("reason"), None);
        assert_eq!(stack.get("banned_at"), None);
        assert_eq!(stack.get("ends_at"), None);
    }
}
