use forge_events::{Event, EventSource};
use forge_registry::{EventFilter, FormField, TriggerCategory, TriggerKindDescriptor};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub(crate) struct ChannelRaidReceivedDescriptor;

impl TriggerKindDescriptor for ChannelRaidReceivedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.raid_received"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Raids
    }

    fn label(&self) -> &str {
        "Raid received"
    }

    fn summary(&self) -> &str {
        "Fires when another streamer raids your channel"
    }

    fn search_text(&self) -> &str {
        "twitch raid incoming host viewers streamer"
    }

    fn icon_name(&self) -> &str {
        "sword"
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
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.raid".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let viewer_count = event
            .payload
            .get("viewer_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let from_login = event
            .payload
            .get("from_broadcaster")
            .and_then(|b| b.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let from_id = event
            .payload
            .get("from_broadcaster")
            .and_then(|b| b.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let from_display_name = event
            .payload
            .get("from_broadcaster")
            .and_then(|b| b.get("display_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("raid_viewer_count".to_owned(), Variant::Int(viewer_count))
            .set("raider_login".to_owned(), Variant::String(from_login))
            .set("raider_id".to_owned(), Variant::String(from_id))
            .set(
                "raider_display_name".to_owned(),
                Variant::String(from_display_name),
            )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_types::{ActionId, Trigger, TriggerId};

    fn make_trigger() -> Trigger {
        Trigger {
            id: TriggerId::new(),
            action_id: ActionId::new(),
            kind_id: "twitch.channel.raid_received".to_owned(),
            config: TriggerConfig::new(),
        }
    }

    fn raid_event(viewers: i64) -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.raid",
            serde_json::json!({
                "viewer_count": viewers,
                "from_broadcaster": {
                    "id": "666",
                    "login": "big_streamer",
                    "display_name": "BigStreamer"
                }
            }),
        )
    }

    #[test]
    fn id_is_stable() {
        assert_eq!(
            ChannelRaidReceivedDescriptor.id(),
            "twitch.channel.raid_received"
        );
    }

    #[test]
    fn always_matches() {
        let trigger = make_trigger();
        assert!(ChannelRaidReceivedDescriptor.matches_trigger(&trigger.config, &raid_event(100)));
    }

    #[test]
    fn condition_display_is_any() {
        assert_eq!(
            ChannelRaidReceivedDescriptor.condition_display(&TriggerConfig::new()),
            "any"
        );
    }

    #[test]
    fn build_arg_stack_extracts_raid_fields() {
        let stack = ChannelRaidReceivedDescriptor.build_arg_stack(&raid_event(250));
        assert_eq!(stack.get("raid_viewer_count"), Some(&Variant::Int(250)));
        assert_eq!(
            stack.get("raider_login"),
            Some(&Variant::String("big_streamer".to_owned()))
        );
        assert_eq!(
            stack.get("raider_display_name"),
            Some(&Variant::String("BigStreamer".to_owned()))
        );
    }
}
