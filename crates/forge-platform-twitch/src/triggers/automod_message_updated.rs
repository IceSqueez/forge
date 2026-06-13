use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct AutomodMessageUpdatedDescriptor;

impl TriggerKindDescriptor for AutomodMessageUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.automod.message_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "AutoMod message decision updated"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator approves, denies, or allows a held AutoMod message to expire"
    }

    fn search_text(&self) -> &str {
        "twitch automod message approved denied expired moderator held decision status"
    }

    fn icon_name(&self) -> &str {
        "shield"
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
            label: "Decision status",
            options: &["any", "approved", "denied", "expired"],
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
            kind_prefix: Some("channel.automod.message.update".to_owned()),
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
            .get("automod")
            .and_then(|a| a.get("status"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        event_status.to_lowercase() == filter
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let automod = event.payload.get("automod");
        let user = event.payload.get("user");
        let moderator = event.payload.get("moderator");

        let message_id = automod
            .and_then(|a| a.get("message_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let status = automod
            .and_then(|a| a.get("status"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_login = user
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let message_text = event
            .payload
            .get("message_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let moderator_login = moderator
            .and_then(|m| m.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("automod.message_id".to_owned(), Variant::String(message_id))
            .set("automod.status".to_owned(), Variant::String(status))
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("message_text".to_owned(), Variant::String(message_text))
            .set(
                "moderator_login".to_owned(),
                Variant::String(moderator_login),
            )
    }
}
