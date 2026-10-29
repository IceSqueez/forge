use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalVariable, EventFilter,
    FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, youtube_actor};
use crate::vocabulary::fields::ban as fields;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const ANY_BAN: &str = "any";
const PERMANENT_BAN: &str = "permanent";
const TEMPORARY_BAN: &str = "temporary";

pub(crate) struct ChannelUserBannedDescriptor;

impl TriggerKindDescriptor for ChannelUserBannedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CHANNEL_USER_BANNED
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Users
    }

    fn label(&self) -> &str {
        "User banned"
    }

    fn summary(&self) -> &str {
        "Fires when a user is banned (permanent or temporary) from YouTube live chat"
    }

    fn search_text(&self) -> &str {
        "youtube channel user banned timeout temporary permanent moderation"
    }

    fn icon_name(&self) -> &str {
        "user-x"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::YouTube)
    }

    fn default_config(&self) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert(
            "ban_type_filter".to_owned(),
            Variant::String(ANY_BAN.to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Select {
            key: "ban_type_filter",
            label: "Ban type",
            options: &[ANY_BAN, PERMANENT_BAN, TEMPORARY_BAN],
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let filter = config
            .get("ban_type_filter")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or(ANY_BAN);

        match filter {
            PERMANENT_BAN => "permanent ban only".to_owned(),
            TEMPORARY_BAN => "temporary timeout only".to_owned(),
            _ => "any ban type".to_owned(),
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::YouTube),
            kind_prefix: Some(event_kinds::CHANNEL_USER_BANNED.to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let filter = config
            .get("ban_type_filter")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or(ANY_BAN);

        match filter {
            PERMANENT_BAN => ban_type(event) == PERMANENT_BAN,
            TEMPORARY_BAN => ban_type(event) == TEMPORARY_BAN,
            _ => true,
        }
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(youtube_actor(ActorRole::Principal), target_identity)
                .actor(youtube_actor(ActorRole::Moderator), moderator_identity)
                .event_specific(
                    DeclaredVariable {
                        name: variables::ban::TYPE.to_owned(),
                        kind: VariantKind::String,
                        label: "Ban type".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(ban_type(event)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: variables::ban::DURATION_SECONDS.to_owned(),
                        kind: VariantKind::Int,
                        label: "Ban duration in seconds".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt {
                            min: 0,
                            max: 86_400,
                        }),
                    },
                    |event| Variant::Int(payload_read::number(event, fields::DURATION_SECS)),
                )
                .legacy(
                    variables::ban::TARGET_DISPLAY_NAME,
                    "Banned user display name",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                )
                .legacy(
                    variables::ban::TARGET_CHANNEL_ID,
                    "Banned user channel ID",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                )
                .legacy(
                    variables::ban::MODERATOR_CHANNEL_ID,
                    "Moderator channel ID",
                    CanonicalVariable::actor(ActorRole::Moderator, ActorSlot::Id),
                )
                .legacy(
                    variables::ban::MODERATOR_DISPLAY_NAME,
                    "Moderator display name",
                    CanonicalVariable::actor(ActorRole::Moderator, ActorSlot::Name),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actors(&[ActorRole::Moderator])
    }
}

fn target_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::TARGET_USER))
}

fn moderator_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::MODERATOR))
}

fn ban_type(event: &Event) -> String {
    let declared = payload_read::text(event, fields::TYPE);
    if declared.is_empty() {
        PERMANENT_BAN.to_owned()
    } else {
        declared
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ban_event(payload: serde_json::Value) -> Event {
        Event::new(EventSource::YouTube, "youtube.channel.user_banned", payload)
    }

    fn filter_config(ban_type: &str) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert(
            "ban_type_filter".to_owned(),
            Variant::String(ban_type.to_owned()),
        );
        cfg
    }

    fn a_five_minute_timeout() -> Event {
        ban_event(json!({
            "target_user": { "display_name": "Troll", "channel_id": "UCtarget" },
            "moderator": { "channel_id": "UCmod", "display_name": "ModName" },
            "type": "temporary",
            "duration_secs": 300_i64,
        }))
    }

    #[test]
    fn a_ban_publishes_the_banned_user_as_the_principal_and_the_moderator_in_its_own_block() {
        let stack = ChannelUserBannedDescriptor.build_arg_stack(&a_five_minute_timeout());
        for (name, value) in [
            ("user_id", "UCtarget"),
            ("user_name", "Troll"),
            ("moderator_id", "UCmod"),
            ("moderator_name", "ModName"),
            ("ban.type", "temporary"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
        assert_eq!(stack.get("ban.duration_seconds"), Some(&Variant::Int(300)));
    }

    #[test]
    fn a_ban_the_wire_attributes_to_nobody_reads_as_permanent_by_an_empty_moderator() {
        let stack = ChannelUserBannedDescriptor.build_arg_stack(&ban_event(json!({})));
        assert_eq!(
            stack.get("ban.type"),
            Some(&Variant::String("permanent".to_owned()))
        );
        assert_eq!(stack.get("ban.duration_seconds"), Some(&Variant::Int(0)));
        for name in ["user_id", "user_name", "moderator_id", "moderator_name"] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(String::new())),
                "'{name}'"
            );
        }
    }

    #[test]
    fn a_ban_type_filter_admits_only_the_type_it_names() {
        for (filter, wire_type, expected) in [
            (Some("permanent"), Some("permanent"), true),
            (Some("permanent"), Some("temporary"), false),
            (Some("permanent"), None, true),
            (Some("temporary"), Some("temporary"), true),
            (Some("temporary"), Some("permanent"), false),
            (Some("temporary"), None, false),
            (Some("any"), Some("permanent"), true),
            (Some("any"), Some("temporary"), true),
            (Some("any"), None, true),
            (None, Some("temporary"), true),
            (None, None, true),
        ] {
            let cfg = filter.map_or_else(TriggerConfig::new, filter_config);
            let event = wire_type.map_or_else(
                || ban_event(json!({})),
                |wire| ban_event(json!({ "type": wire })),
            );
            assert_eq!(
                ChannelUserBannedDescriptor.matches_trigger(&cfg, &event),
                expected,
                "filter {filter:?} wire type {wire_type:?}"
            );
        }
    }
}
