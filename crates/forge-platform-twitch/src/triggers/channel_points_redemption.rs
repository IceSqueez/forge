use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ChannelPointsRedemptionDescriptor;

impl TriggerKindDescriptor for ChannelPointsRedemptionDescriptor {
    fn id(&self) -> &str {
        "twitch.channel_points.redemption"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Ungrouped
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
            kind_prefix: Some("channel.channel_points_redemption".to_owned()),
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
                .get("reward")
                .and_then(|r| r.get("id"))
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
                .get("reward")
                .and_then(|r| r.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            return event_reward_title == reward_title;
        }

        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let redemption_id = event
            .payload
            .get("redemption")
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let redemption_status = event
            .payload
            .get("redemption")
            .and_then(|r| r.get("status"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_input = event
            .payload
            .get("redemption")
            .and_then(|r| r.get("user_input"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let redeemed_at = event
            .payload
            .get("redemption")
            .and_then(|r| r.get("redeemed_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = event
            .payload
            .get("user")
            .and_then(|u| u.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_login = event
            .payload
            .get("user")
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_name = event
            .payload
            .get("user")
            .and_then(|u| u.get("display_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let reward_id = event
            .payload
            .get("reward")
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let reward_title = event
            .payload
            .get("reward")
            .and_then(|r| r.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let reward_cost = event
            .payload
            .get("reward")
            .and_then(|r| r.get("cost"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let reward_prompt = event
            .payload
            .get("reward")
            .and_then(|r| r.get("prompt"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("redemption.id".to_owned(), Variant::String(redemption_id))
            .set(
                "redemption.status".to_owned(),
                Variant::String(redemption_status),
            )
            .set("user_input".to_owned(), Variant::String(user_input))
            .set("redeemed_at".to_owned(), Variant::String(redeemed_at))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_name".to_owned(), Variant::String(user_name))
            .set("reward.id".to_owned(), Variant::String(reward_id))
            .set("reward.title".to_owned(), Variant::String(reward_title))
            .set("reward.cost".to_owned(), Variant::Int(reward_cost))
            .set("reward.prompt".to_owned(), Variant::String(reward_prompt))
    }
}
