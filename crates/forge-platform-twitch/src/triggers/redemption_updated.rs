use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

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

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let redemption = event.payload.get(fields::REDEMPTION);
        let user = event.payload.get(fields::USER);
        let reward = event.payload.get(fields::REWARD);

        let redemption_id = redemption
            .and_then(|r| r.get(fields::REDEMPTION_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let status = redemption
            .and_then(|r| r.get(fields::REDEMPTION_STATUS))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_input = redemption
            .and_then(|r| r.get(fields::USER_INPUT))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
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
        let reward_id = reward
            .and_then(|r| r.get(fields::REWARD_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let reward_title = reward
            .and_then(|r| r.get(fields::REWARD_TITLE))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("redemption.id".to_owned(), Variant::String(redemption_id))
            .set("redemption.status".to_owned(), Variant::String(status))
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("user_input".to_owned(), Variant::String(user_input))
            .set("reward.id".to_owned(), Variant::String(reward_id))
            .set("reward.title".to_owned(), Variant::String(reward_title))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "redemption.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Redemption ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "redemption.status".to_owned(),
                        kind: VariantKind::String,
                        label: "Redemption status".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "user_login".to_owned(),
                        kind: VariantKind::String,
                        label: "User login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "user_id".to_owned(),
                        kind: VariantKind::String,
                        label: "User ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "user_input".to_owned(),
                        kind: VariantKind::String,
                        label: "User input".to_owned(),
                        synthesis: Some(SynthesisHint::Message),
                    },
                    DeclaredVariable {
                        name: "reward.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Reward ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "reward.title".to_owned(),
                        kind: VariantKind::String,
                        label: "Reward title".to_owned(),
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
