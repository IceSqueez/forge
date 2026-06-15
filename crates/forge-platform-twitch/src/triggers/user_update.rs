use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct UserUpdateDescriptor;

impl TriggerKindDescriptor for UserUpdateDescriptor {
    fn id(&self) -> &str {
        "twitch.user.update"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Users
    }

    fn label(&self) -> &str {
        "User profile updated"
    }

    fn summary(&self) -> &str {
        "Fires when the authenticated user changes their profile description or display name"
    }

    fn search_text(&self) -> &str {
        "twitch user update profile description display name account changed"
    }

    fn icon_name(&self) -> &str {
        "user-pen"
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
        "any profile change".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("user.update".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user = event.payload.get("user");

        let user_id = user
            .and_then(|u| u.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_login = user
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_display_name = user
            .and_then(|u| u.get("display_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_description = user
            .and_then(|u| u.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("user.id".to_owned(), Variant::String(user_id))
            .set("user.login".to_owned(), Variant::String(user_login))
            .set(
                "user.display_name".to_owned(),
                Variant::String(user_display_name),
            )
            .set(
                "user.description".to_owned(),
                Variant::String(user_description),
            )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    fn event_with(payload: serde_json::Value) -> Event {
        Event::new(EventSource::Twitch, "user.update", payload)
    }

    fn expect_string(stack: &ArgStack, key: &str, expected: &str) {
        assert_eq!(
            stack.get(key),
            Some(&Variant::String(expected.to_owned())),
            "arg {key}"
        );
    }

    #[test]
    fn build_arg_stack_surfaces_every_user_field_from_payload() {
        let event = event_with(json!({
            "user": {
                "id": "1337",
                "login": "thestreamer",
                "display_name": "TheStreamer",
                "description": "now playing forge",
            },
        }));

        let stack = UserUpdateDescriptor.build_arg_stack(&event);

        expect_string(&stack, "user.id", "1337");
        expect_string(&stack, "user.login", "thestreamer");
        expect_string(&stack, "user.display_name", "TheStreamer");
        expect_string(&stack, "user.description", "now playing forge");
    }

    #[test]
    fn build_arg_stack_on_empty_payload_yields_blank_fields_without_panic() {
        let stack = UserUpdateDescriptor.build_arg_stack(&event_with(json!({})));

        for key in [
            "user.id",
            "user.login",
            "user.display_name",
            "user.description",
        ] {
            expect_string(&stack, key, "");
        }
    }

    // Why: tokens/PII discipline (CLAUDE.md invariant 7). Twitch's raw
    // user.update field set includes `email`/`email_verified` when the
    // `user:read:email` scope is granted. This trigger must NEVER surface
    // that PII into the ArgStack the user can interpolate into chat/logs.
    // Guards against a future edit that naively copies the raw user object.
    #[test]
    fn build_arg_stack_never_surfaces_email_pii_even_when_present_in_payload() {
        let secret_email = "streamer@example.com";
        let event = event_with(json!({
            "user": {
                "id": "1337",
                "login": "thestreamer",
                "display_name": "TheStreamer",
                "description": "bio",
                "email": secret_email,
                "email_verified": true,
            },
        }));

        let stack = UserUpdateDescriptor.build_arg_stack(&event);

        for (key, value) in stack.snapshot() {
            assert!(
                !key.contains("email"),
                "ArgStack exposed an email-named arg: {key}"
            );
            if let Variant::String(s) = &value {
                assert_ne!(s, secret_email, "ArgStack leaked the email value at {key}");
            }
        }
    }

    #[test]
    fn event_filter_targets_twitch_user_update_kind() {
        let filter = UserUpdateDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("user.update"));
    }
}
