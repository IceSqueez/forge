use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct BanDescriptor;

impl TriggerKindDescriptor for BanDescriptor {
    fn id(&self) -> &str {
        "kick.ban"
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
            kind_prefix: Some("kick.ban".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let banned_user = event.payload.get("user");
        let banned_user_id = banned_user
            .and_then(|u| u.get("id"))
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());
        let banned_username = banned_user
            .and_then(|u| u.get("username"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let duration_secs = event
            .payload
            .get("duration")
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());

        let reason = event
            .payload
            .get("permanent_ban_reason")
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
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn ban_event() -> Event {
        Event::new(
            EventSource::Kick,
            "kick.ban",
            serde_json::json!({
                "user": { "id": 77, "username": "bad_actor" },
                "banned_by": { "id": 2, "username": "mod" },
                "duration": 300,
                "permanent_ban_reason": ""
            }),
        )
    }

    #[test]
    fn kind_id_is_canonical() {
        assert_eq!(BanDescriptor.id(), "kick.ban");
    }

    #[test]
    fn category_is_users() {
        assert_eq!(BanDescriptor.category(), TriggerCategory::Users);
    }

    #[test]
    fn platform_contract_is_kick() {
        assert_eq!(
            BanDescriptor.platform_contract(),
            KindPlatformContract::PlatformSpecific(PlatformId::Kick)
        );
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
