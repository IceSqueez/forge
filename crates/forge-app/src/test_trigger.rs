use forge_events::{Event, EventSource};
use forge_types::{TriggerInstance, Variant};
use serde_json::json;

pub fn synthesize_test_event(instance: &TriggerInstance) -> Event {
    match instance.kind_id.as_str() {
        "twitch.chat.command" => Event::new(
            EventSource::Twitch,
            "chat.message",
            json!({
                "message": "!test",
                "user_login": "test_user",
                "channel": "test_channel"
            }),
        ),
        "twitch.chat.message" => Event::new(
            EventSource::Twitch,
            "chat.message",
            json!({
                "message": "test message",
                "user_login": "test_user",
                "channel": "test_channel"
            }),
        ),
        "twitch.support.subscriber" => Event::new(
            EventSource::Twitch,
            "sub.received",
            json!({
                "user_login": "test_user",
                "tier": "1000"
            }),
        ),
        "twitch.support.resubscriber" => Event::new(
            EventSource::Twitch,
            "resub.received",
            json!({
                "user_login": "test_user",
                "tier": "1000",
                "months": 3
            }),
        ),
        "twitch.support.gift_sub" => Event::new(
            EventSource::Twitch,
            "giftsub.received",
            json!({
                "gifter_login": "test_gifter",
                "recipient_login": "test_recipient",
                "tier": "1000"
            }),
        ),
        "twitch.support.cheer" => Event::new(
            EventSource::Twitch,
            "cheer.received",
            json!({
                "user_login": "test_user",
                "bits": 100
            }),
        ),
        "twitch.channel.raid_received" => Event::new(
            EventSource::Twitch,
            "raid.received",
            json!({
                "from_broadcaster_login": "test_raider",
                "viewers": 10
            }),
        ),
        "obs.scenes.current_changed" => {
            let scene_name = match instance.overrides.get("scene") {
                Some(Variant::String(s)) => s.clone(),
                _ => "TestScene".to_owned(),
            };
            Event::new(
                EventSource::Obs,
                "scene.changed",
                json!({ "scene": scene_name }),
            )
        }
        "script.event.custom" => {
            let event_name = match instance.overrides.get("name") {
                Some(Variant::String(s)) if !s.is_empty() => s.as_str(),
                _ => "test",
            };
            Event::new(
                EventSource::Server,
                format!("custom.{event_name}"),
                json!({}),
            )
        }
        _ => Event::new(EventSource::Core, "test.trigger", json!({})),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_types::TriggerInstanceId;
    use std::collections::BTreeMap;

    fn make_instance(kind_id: &str) -> TriggerInstance {
        TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: kind_id.to_owned(),
            name: kind_id.to_owned(),
            overrides: BTreeMap::new(),
            enabled: true,
            user_defined: false,
        }
    }

    #[test]
    fn chat_command_trigger_yields_twitch_chat_message() {
        let instance = make_instance("twitch.chat.command");
        let event = synthesize_test_event(&instance);
        assert_eq!(event.kind, "chat.message");
        assert_eq!(event.source, EventSource::Twitch);
    }

    #[test]
    fn chat_command_uses_test_message() {
        let instance = make_instance("twitch.chat.command");
        let event = synthesize_test_event(&instance);
        assert_eq!(event.payload["message"].as_str().unwrap(), "!test");
    }

    #[test]
    fn any_message_trigger_yields_twitch_chat_message() {
        let instance = make_instance("twitch.chat.message");
        let event = synthesize_test_event(&instance);
        assert_eq!(event.kind, "chat.message");
        assert_eq!(event.source, EventSource::Twitch);
    }

    #[test]
    fn obs_scene_changed_yields_scene_changed_with_obs_source() {
        let instance = TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: "obs.scenes.current_changed".to_owned(),
            name: "obs.scenes.current_changed".to_owned(),
            overrides: BTreeMap::from([("scene".to_owned(), Variant::String("Gaming".to_owned()))]),
            enabled: true,
            user_defined: false,
        };
        let event = synthesize_test_event(&instance);
        assert_eq!(event.kind, "scene.changed");
        assert_eq!(event.source, EventSource::Obs);
        assert_eq!(event.payload["scene"].as_str().unwrap(), "Gaming");
    }

    #[test]
    fn obs_scene_changed_none_uses_test_scene() {
        let instance = make_instance("obs.scenes.current_changed");
        let event = synthesize_test_event(&instance);
        assert_eq!(event.payload["scene"].as_str().unwrap(), "TestScene");
    }

    #[test]
    fn subscribe_trigger_yields_sub_received() {
        let instance = make_instance("twitch.support.subscriber");
        let event = synthesize_test_event(&instance);
        assert_eq!(event.kind, "sub.received");
        assert_eq!(event.source, EventSource::Twitch);
    }

    #[test]
    fn raid_trigger_yields_raid_received() {
        let instance = make_instance("twitch.channel.raid_received");
        let event = synthesize_test_event(&instance);
        assert_eq!(event.kind, "raid.received");
        assert_eq!(event.source, EventSource::Twitch);
    }
}
