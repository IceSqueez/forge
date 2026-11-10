use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::ChatEventKind;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, TriggerCategory, TriggerVariables,
};
use forge_types::{DeclaredVariable, VariantKind};
use time::OffsetDateTime;

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::follow as fields;

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), follower_identity)
            .event_specific_optional(
                DeclaredVariable {
                    name: "followed_at".to_owned(),
                    kind: VariantKind::Datetime,
                    synthesis: None,
                },
                |event| payload_read::timestamp(event, fields::FOLLOWED_AT),
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_FOLLOW,
    category: TriggerCategory::Users,
    label: "Follow",
    summary: "Fires when a viewer follows your channel",
    search_text: "twitch follow follower new",
    icon: "heart",
    kind: event_kinds::CHANNEL_FOLLOW,
    actors: ActorDeclaration::principal(),
    variables,
    chat_event: Some(ChatEventKind::Follow),
    follow_moment,
    ..TwitchTriggerSpec::BASE
};

fn follow_moment(event: &Event) -> Option<OffsetDateTime> {
    event
        .payload
        .get(fields::FOLLOWED_AT)
        .and_then(|raw| raw.as_str())
        .and_then(forge_platform_core::parse_timestamp)
}

fn follower_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::USER),
        fields::USER_ID,
        fields::USER_LOGIN,
        fields::USER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestamp::tests::instant;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

    fn follow_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.follow",
            serde_json::json!({
                "followed_at": "2026-06-13T10:00:00Z",
                "user": { "id": "42", "login": "new_follower", "display_name": "NewFollower" }
            }),
        )
    }

    #[test]
    fn a_follow_publishes_the_canonical_actor_block_and_the_moment_it_happened() {
        let stack = SPEC.build_arg_stack(&follow_event());
        for (name, value) in [
            ("user_id", "42"),
            ("user_name", "NewFollower"),
            ("user_login", "new_follower"),
            ("user_platform", "twitch"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
        assert_eq!(
            stack.get("followed_at"),
            Some(&instant("2026-06-13T10:00:00Z"))
        );
    }
}
