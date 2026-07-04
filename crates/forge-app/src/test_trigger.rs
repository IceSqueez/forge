use forge_events::{Event, EventSource};
use forge_types::{TriggerConfig, TriggerInstance, Variant};
use serde_json::json;

/// Builds a synthetic event that the trigger evaluator will judge with the same
/// `matches_trigger` predicate a live event faces. `config` is the instance's
/// effective config (descriptor defaults merged with overrides), so the synthetic
/// chat message carries the trigger's configured phrase instead of a fixed literal.
pub fn synthesize_test_event(instance: &TriggerInstance, config: &TriggerConfig) -> Event {
    match instance.kind_id.as_str() {
        "twitch.chat.command" => {
            let phrase = match config.get("phrase") {
                Some(Variant::String(s)) if !s.is_empty() => s.as_str(),
                _ => "!command",
            };
            Event::new(
                EventSource::Twitch,
                "chat.message",
                json!({
                    "message": format!("{phrase} test"),
                    "user_login": "test_user",
                    "channel": "test_channel"
                }),
            )
        }
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
            let scene_name = match config.get("scene") {
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
            let event_name = match config.get("name") {
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
            platform_scope: Default::default(),
        }
    }

    #[test]
    fn chat_command_message_uses_configured_phrase() {
        // Regression guard: the synthesized chat message must carry the trigger's
        // configured command phrase so matches_trigger judges it as it would a live
        // event. A revert to the old fixed "!test" literal fails the custom-phrase row.
        let instance = make_instance("twitch.chat.command");
        for (phrase_cfg, expected) in [
            (None, "!command test"),         // phrase unset -> default
            (Some(""), "!command test"),     // empty phrase falls back to default
            (Some("!quote"), "!quote test"), // custom phrase honored verbatim
        ] {
            let mut config = TriggerConfig::new();
            if let Some(p) = phrase_cfg {
                config.insert("phrase".to_owned(), Variant::String(p.to_owned()));
            }
            let event = synthesize_test_event(&instance, &config);
            assert_eq!(
                event.payload["message"].as_str().unwrap(),
                expected,
                "phrase cfg {phrase_cfg:?}"
            );
        }
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
            platform_scope: Default::default(),
        };
        let event = synthesize_test_event(&instance, &instance.overrides);
        assert_eq!(event.kind, "scene.changed");
        assert_eq!(event.source, EventSource::Obs);
        assert_eq!(event.payload["scene"].as_str().unwrap(), "Gaming");
    }

    #[test]
    fn obs_scene_changed_none_uses_test_scene() {
        let instance = make_instance("obs.scenes.current_changed");
        let event = synthesize_test_event(&instance, &instance.overrides);
        assert_eq!(event.payload["scene"].as_str().unwrap(), "TestScene");
    }
}
