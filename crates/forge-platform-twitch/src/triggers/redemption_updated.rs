use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

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
            kind_prefix: Some("channel.channel_points_redemption_update".to_owned()),
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
            .get("redemption")
            .and_then(|r| r.get("status"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        event_status == filter
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let redemption = event.payload.get("redemption");
        let user = event.payload.get("user");
        let reward = event.payload.get("reward");

        let redemption_id = redemption
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let status = redemption
            .and_then(|r| r.get("status"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_input = redemption
            .and_then(|r| r.get("user_input"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
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
        let reward_id = reward
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let reward_title = reward
            .and_then(|r| r.get("title"))
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
            Some("channel.channel_points_redemption_update")
        );
    }

    #[test]
    fn matches_trigger_gates_fulfilled_event_by_status_filter() {
        // Event status is "fulfilled". Each row exercises the status_filter gate:
        // "any" fires unconditionally; a concrete filter fires only on an exact
        // status match. The default config (no override) behaves like "any".
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
