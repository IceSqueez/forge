use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig,
    Variant, VariantKind,
};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::channel_points as fields;

pub(crate) struct RedemptionUpdatedDescriptor;

impl TriggerKindDescriptor for RedemptionUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel_points.redemption_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Channel point redemption updated"
    }

    fn summary(&self) -> &str {
        "Fires when a channel point redemption is fulfilled or canceled"
    }

    fn search_text(&self) -> &str {
        "twitch channel points redemption fulfilled canceled updated status"
    }

    fn icon_name(&self) -> &str {
        "star"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert(
            "status_filter".to_owned(),
            Variant::String("any".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Select {
            key: "status_filter",
            label: "Redemption status",
            options: &["any", "fulfilled", "canceled"],
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let status = config
            .get("status_filter")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or("any");
        if status == "any" {
            "any status".to_owned()
        } else {
            format!("status = {}", status)
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some(
                "twitch.channel.channel_points_custom_reward_redemption.update".to_owned(),
            ),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let filter = config
            .get("status_filter")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or("any");

        if filter == "any" {
            return true;
        }

        let event_status = event
            .payload
            .get(fields::REDEMPTION)
            .and_then(|r| r.get(fields::REDEMPTION_STATUS))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        event_status == filter
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), redeemer_identity)
                .message_text(redemption_user_input)
                .event_specific(
                    DeclaredVariable {
                        name: "redemption.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Redemption ID".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            fields::REDEMPTION,
                            fields::REDEMPTION_ID,
                        ))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: "redemption.status".to_owned(),
                        kind: VariantKind::String,
                        label: "Redemption status".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            fields::REDEMPTION,
                            fields::REDEMPTION_STATUS,
                        ))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: "reward.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Reward ID".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            fields::REWARD,
                            fields::REWARD_ID,
                        ))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: "reward.title".to_owned(),
                        kind: VariantKind::String,
                        label: "Reward title".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            fields::REWARD,
                            fields::REWARD_TITLE,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "user_input".to_owned(),
                        kind: VariantKind::String,
                        label: "User input".to_owned(),
                        synthesis: Some(SynthesisHint::Message),
                    },
                    CanonicalVariable::MessageText,
                    |event| Variant::String(redemption_user_input(event)),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn redeemer_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::USER),
        fields::USER_ID,
        fields::USER_LOGIN,
        fields::USER_DISPLAY_NAME,
    )
}

fn redemption_user_input(event: &Event) -> String {
    payload_read::nested_text(event, fields::REDEMPTION, fields::USER_INPUT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_filter(filter: &str) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert(
            "status_filter".to_owned(),
            Variant::String(filter.to_owned()),
        );
        cfg
    }

    fn fulfilled_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.channel_points_redemption_update",
            serde_json::json!({
                "redemption": {
                    "id": "redemption-42",
                    "status": "fulfilled",
                    "user_input": "play my song",
                },
                "user": {
                    "id": "777",
                    "login": "viewer_one",
                    "display_name": "ViewerOne",
                },
                "reward": {
                    "id": "r1",
                    "title": "Song Request",
                    "cost": 500,
                },
            }),
        )
    }

    #[test]
    fn event_filter_targets_redemption_update_kind_from_twitch() {
        let filter = RedemptionUpdatedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.channel_points_custom_reward_redemption.update")
        );
    }

    #[test]
    fn matches_trigger_gates_fulfilled_event_by_status_filter() {
        let event = fulfilled_event();
        let cases = [
            ("any fires", config_with_filter("any"), true),
            (
                "matching status fires",
                config_with_filter("fulfilled"),
                true,
            ),
            (
                "non-matching status suppressed",
                config_with_filter("canceled"),
                false,
            ),
            (
                "default config fires (any)",
                RedemptionUpdatedDescriptor.default_config(),
                true,
            ),
        ];
        for (name, cfg, expected) in cases {
            assert_eq!(
                RedemptionUpdatedDescriptor.matches_trigger(&cfg, &event),
                expected,
                "case: {name}"
            );
        }
    }

    #[test]
    fn build_arg_stack_exposes_redemption_user_and_reward_vars() {
        let stack = RedemptionUpdatedDescriptor.build_arg_stack(&fulfilled_event());
        assert_eq!(
            stack.get("redemption.id"),
            Some(&Variant::String("redemption-42".to_owned()))
        );
        assert_eq!(
            stack.get("redemption.status"),
            Some(&Variant::String("fulfilled".to_owned()))
        );
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("viewer_one".to_owned()))
        );
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("777".to_owned()))
        );
        assert_eq!(
            stack.get("user_input"),
            Some(&Variant::String("play my song".to_owned()))
        );
        assert_eq!(
            stack.get("reward.id"),
            Some(&Variant::String("r1".to_owned()))
        );
        assert_eq!(
            stack.get("reward.title"),
            Some(&Variant::String("Song Request".to_owned()))
        );
    }
}
