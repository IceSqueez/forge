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

pub(crate) struct ChannelPointsRedemptionDescriptor;

impl TriggerKindDescriptor for ChannelPointsRedemptionDescriptor {
    fn id(&self) -> &str {
        "twitch.channel_points.redemption"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Channel Point Redemption"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer redeems a channel point reward"
    }

    fn search_text(&self) -> &str {
        "twitch channel points reward redemption redeem"
    }

    fn icon_name(&self) -> &str {
        "star"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert("reward_id".to_owned(), Variant::String(String::new()));
        cfg.insert("reward_title".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "reward_id",
                label: "Reward ID (leave blank to match any reward)",
                placeholder: "",
            },
            FormField::Text {
                key: "reward_title",
                label: "Reward title (used only when Reward ID is blank; leave blank to match any)",
                placeholder: "",
            },
        ]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let reward_id = config
            .get("reward_id")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        if !reward_id.is_empty() {
            return format!("reward id = {}", reward_id);
        }
        let reward_title = config
            .get("reward_title")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        if !reward_title.is_empty() {
            return format!("reward = {}", reward_title);
        }
        "any reward".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some(
                "twitch.channel.channel_points_custom_reward_redemption.add".to_owned(),
            ),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let reward_id = config
            .get("reward_id")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        if !reward_id.is_empty() {
            let event_reward_id = event
                .payload
                .get(fields::REWARD)
                .and_then(|r| r.get(fields::REWARD_ID))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            return event_reward_id == reward_id;
        }

        let reward_title = config
            .get("reward_title")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        if !reward_title.is_empty() {
            let event_reward_title = event
                .payload
                .get(fields::REWARD)
                .and_then(|r| r.get(fields::REWARD_TITLE))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            return event_reward_title == reward_title;
        }

        true
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
                        name: "redeemed_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Redeemed at".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            fields::REDEMPTION,
                            fields::REDEEMED_AT,
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
                .event_specific(
                    DeclaredVariable {
                        name: "reward.cost".to_owned(),
                        kind: VariantKind::Int,
                        label: "Reward cost".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt {
                            min: 0,
                            max: 1000000,
                        }),
                    },
                    |event| {
                        Variant::Int(payload_read::nested_number(
                            event,
                            fields::REWARD,
                            fields::REWARD_COST,
                        ))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: "reward.prompt".to_owned(),
                        kind: VariantKind::String,
                        label: "Reward prompt".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            fields::REWARD,
                            fields::REWARD_PROMPT,
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

    fn redemption_config(reward_id: &str, reward_title: &str) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert(
            "reward_id".to_owned(),
            Variant::String(reward_id.to_owned()),
        );
        cfg.insert(
            "reward_title".to_owned(),
            Variant::String(reward_title.to_owned()),
        );
        cfg
    }

    fn redemption_event() -> Event {
        let payload = serde_json::json!({
            "redemption": {
                "id": "redemption-42",
                "status": "unfulfilled",
                "user_input": "play my song",
                "redeemed_at": "2026-06-13T10:00:00Z",
            },
            "user": {
                "id": "777",
                "login": "viewer_one",
                "display_name": "ViewerOne",
            },
            "reward": {
                "id": "r1",
                "title": "Hydrate",
                "cost": 500,
                "prompt": "Make the streamer drink water",
            },
        });
        Event::new(
            EventSource::Twitch,
            "channel.channel_points_redemption",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_redemption_kind_from_twitch() {
        let filter = ChannelPointsRedemptionDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.channel_points_custom_reward_redemption.add")
        );
    }

    #[test]
    fn matches_trigger_applies_reward_id_then_title_precedence() {
        let cases = [
            ("both blank matches any", "", "", true),
            ("reward_id hit", "r1", "", true),
            ("reward_id miss", "rX", "", false),
            ("reward_title hit when id blank", "", "Hydrate", true),
            ("reward_title miss when id blank", "", "Other", false),
            ("reward_id precedence over title", "r1", "Other", true),
        ];
        for (name, reward_id, reward_title, expected) in cases {
            let cfg = redemption_config(reward_id, reward_title);
            assert_eq!(
                ChannelPointsRedemptionDescriptor.matches_trigger(&cfg, &redemption_event()),
                expected,
                "case: {name}"
            );
        }
    }

    #[test]
    fn build_arg_stack_exposes_all_redemption_user_and_reward_vars() {
        let stack = ChannelPointsRedemptionDescriptor.build_arg_stack(&redemption_event());
        assert_eq!(
            stack.get("redemption.id"),
            Some(&Variant::String("redemption-42".to_owned()))
        );
        assert_eq!(
            stack.get("redemption.status"),
            Some(&Variant::String("unfulfilled".to_owned()))
        );
        assert_eq!(
            stack.get("user_input"),
            Some(&Variant::String("play my song".to_owned()))
        );
        assert_eq!(
            stack.get("redeemed_at"),
            Some(&Variant::String("2026-06-13T10:00:00Z".to_owned()))
        );
        assert_eq!(
            stack.get("user_id"),
            Some(&Variant::String("777".to_owned()))
        );
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("viewer_one".to_owned()))
        );
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("ViewerOne".to_owned()))
        );
        assert_eq!(
            stack.get("reward.id"),
            Some(&Variant::String("r1".to_owned()))
        );
        assert_eq!(
            stack.get("reward.title"),
            Some(&Variant::String("Hydrate".to_owned()))
        );
        assert_eq!(stack.get("reward.cost"), Some(&Variant::Int(500)));
        assert_eq!(
            stack.get("reward.prompt"),
            Some(&Variant::String("Make the streamer drink water".to_owned()))
        );
    }
}
