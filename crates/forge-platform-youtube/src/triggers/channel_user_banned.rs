use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, youtube_actor};
use crate::payload_fields::ban as fields;

const ANY_BAN: &str = "any";
const PERMANENT_BAN: &str = "permanent";
const TEMPORARY_BAN: &str = "temporary";

pub(crate) struct ChannelUserBannedDescriptor;

impl TriggerKindDescriptor for ChannelUserBannedDescriptor {
    fn id(&self) -> &str {
        "youtube.channel.user_banned"
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
            kind_prefix: Some("youtube.channel.user_banned".to_owned()),
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
            PERMANENT_BAN => {
                event.payload.get(fields::TYPE).and_then(|v| v.as_str()) == Some(PERMANENT_BAN)
            }
            TEMPORARY_BAN => {
                event.payload.get(fields::TYPE).and_then(|v| v.as_str()) == Some(TEMPORARY_BAN)
            }
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
                        name: "ban.type".to_owned(),
                        kind: VariantKind::String,
                        label: "Ban type".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(ban_type(event)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "ban.duration_seconds".to_owned(),
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
                    DeclaredVariable {
                        name: "ban.target.display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Banned user display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                    |event| Variant::String(target_identity(event).display_name),
                )
                .legacy(
                    DeclaredVariable {
                        name: "ban.target.channel_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Banned user channel ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| Variant::String(target_identity(event).id),
                )
                .legacy(
                    DeclaredVariable {
                        name: "ban.moderator.channel_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator channel ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Moderator, ActorSlot::Id),
                    |event| Variant::String(moderator_identity(event).id),
                )
                .legacy(
                    DeclaredVariable {
                        name: "ban.moderator.display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator display name".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Moderator, ActorSlot::Name),
                    |event| Variant::String(moderator_identity(event).display_name),
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
#[allow(clippy::unwrap_used)]
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

    #[test]
    fn build_arg_stack_surfaces_target_moderator_type_and_duration() {
        let event = ban_event(json!({
            "target_user": { "display_name": "Troll", "channel_id": "UCtarget" },
            "moderator": { "channel_id": "UCmod", "display_name": "ModName" },
            "type": "temporary",
            "duration_secs": 300_i64,
        }));

        let stack = ChannelUserBannedDescriptor.build_arg_stack(&event);

        assert_eq!(
            stack.get("ban.target.display_name"),
            Some(&Variant::String("Troll".to_owned()))
        );
        assert_eq!(
            stack.get("ban.target.channel_id"),
            Some(&Variant::String("UCtarget".to_owned()))
        );
        assert_eq!(
            stack.get("ban.moderator.channel_id"),
            Some(&Variant::String("UCmod".to_owned()))
        );
        assert_eq!(
            stack.get("ban.moderator.display_name"),
            Some(&Variant::String("ModName".to_owned()))
        );
        assert_eq!(
            stack.get("ban.type"),
            Some(&Variant::String("temporary".to_owned()))
        );
        assert_eq!(stack.get("ban.duration_seconds"), Some(&Variant::Int(300)));
    }

    #[test]
    fn build_arg_stack_surfaces_permanent_type_with_zero_duration() {
        let event = ban_event(json!({
            "type": "permanent",
            "duration_secs": 0_i64,
        }));

        let stack = ChannelUserBannedDescriptor.build_arg_stack(&event);

        assert_eq!(
            stack.get("ban.type"),
            Some(&Variant::String("permanent".to_owned()))
        );
        assert_eq!(stack.get("ban.duration_seconds"), Some(&Variant::Int(0)));
    }

    #[test]
    fn build_arg_stack_on_empty_payload_uses_safe_defaults() {
        let event = ban_event(json!({}));

        let stack = ChannelUserBannedDescriptor.build_arg_stack(&event);

        assert_eq!(
            stack.get("ban.target.display_name"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("ban.type"),
            Some(&Variant::String("permanent".to_owned()))
        );
        assert_eq!(stack.get("ban.duration_seconds"), Some(&Variant::Int(0)));
    }

    #[test]
    fn matches_trigger_permanent_filter_accepts_permanent_rejects_temporary() {
        let cfg = filter_config("permanent");
        let permanent = ban_event(json!({ "type": "permanent" }));
        let temporary = ban_event(json!({ "type": "temporary" }));

        assert!(ChannelUserBannedDescriptor.matches_trigger(&cfg, &permanent));
        assert!(!ChannelUserBannedDescriptor.matches_trigger(&cfg, &temporary));
    }

    #[test]
    fn matches_trigger_temporary_filter_accepts_temporary_rejects_permanent() {
        let cfg = filter_config("temporary");
        let permanent = ban_event(json!({ "type": "permanent" }));
        let temporary = ban_event(json!({ "type": "temporary" }));

        assert!(ChannelUserBannedDescriptor.matches_trigger(&cfg, &temporary));
        assert!(!ChannelUserBannedDescriptor.matches_trigger(&cfg, &permanent));
    }

    #[test]
    fn matches_trigger_any_filter_accepts_both_ban_types() {
        let cfg = filter_config("any");
        let permanent = ban_event(json!({ "type": "permanent" }));
        let temporary = ban_event(json!({ "type": "temporary" }));

        assert!(ChannelUserBannedDescriptor.matches_trigger(&cfg, &permanent));
        assert!(ChannelUserBannedDescriptor.matches_trigger(&cfg, &temporary));
    }

    #[test]
    fn matches_trigger_missing_filter_defaults_to_any() {
        let cfg = TriggerConfig::new();
        let temporary = ban_event(json!({ "type": "temporary" }));

        assert!(ChannelUserBannedDescriptor.matches_trigger(&cfg, &temporary));
    }

    #[test]
    fn matches_trigger_specific_filter_rejects_payload_with_missing_ban_type() {
        let cfg = filter_config("permanent");
        let event = ban_event(json!({}));

        assert!(!ChannelUserBannedDescriptor.matches_trigger(&cfg, &event));
    }
}
