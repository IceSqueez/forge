use forge_events::{Event, EventSource};
use forge_types::{Command, Trigger, TriggerKind};
use serde_json::json;

pub fn synthesize_test_event(trigger: &Trigger, commands: &[Command]) -> Event {
    match &trigger.kind {
        TriggerKind::TwitchChatCommand => {
            let cmd_name = commands
                .iter()
                .find(|c| c.action_id == trigger.action_id)
                .map(|c| c.name.as_str())
                .unwrap_or("!test");
            Event::new(
                EventSource::Twitch,
                "chat.message",
                json!({
                    "message": cmd_name,
                    "user_login": "test_user",
                    "channel": "test_channel"
                }),
            )
        }
        TriggerKind::TwitchChatAnyMessage => Event::new(
            EventSource::Twitch,
            "chat.message",
            json!({
                "message": "test message",
                "user_login": "test_user",
                "channel": "test_channel"
            }),
        ),
        TriggerKind::TwitchSubscribe => Event::new(
            EventSource::Twitch,
            "sub.received",
            json!({
                "user_login": "test_user",
                "tier": "1000"
            }),
        ),
        TriggerKind::TwitchResubscribe => Event::new(
            EventSource::Twitch,
            "resub.received",
            json!({
                "user_login": "test_user",
                "tier": "1000",
                "months": 3
            }),
        ),
        TriggerKind::TwitchGiftSub => Event::new(
            EventSource::Twitch,
            "giftsub.received",
            json!({
                "gifter_login": "test_gifter",
                "recipient_login": "test_recipient",
                "tier": "1000"
            }),
        ),
        TriggerKind::TwitchCheer => Event::new(
            EventSource::Twitch,
            "cheer.received",
            json!({
                "user_login": "test_user",
                "bits": 100
            }),
        ),
        TriggerKind::TwitchRaid => Event::new(
            EventSource::Twitch,
            "raid.received",
            json!({
                "from_broadcaster_login": "test_raider",
                "viewers": 10
            }),
        ),
        TriggerKind::ObsSceneChanged { scene } => {
            let scene_name = scene.as_deref().unwrap_or("TestScene").to_owned();
            Event::new(
                EventSource::Obs,
                "scene.changed",
                json!({ "scene": scene_name }),
            )
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_types::{ActionId, Command, CommandId, CommandPermission, TriggerId};
    use std::collections::BTreeMap;

    fn make_trigger(action_id: ActionId, kind: TriggerKind) -> Trigger {
        Trigger {
            id: TriggerId::new(),
            action_id,
            kind,
            config: BTreeMap::new(),
        }
    }

    fn make_command(action_id: ActionId, name: &str) -> Command {
        Command {
            id: CommandId::new(),
            action_id,
            name: name.to_string(),
            cooldown_secs: 0,
            permission: CommandPermission::Everyone,
        }
    }

    #[test]
    fn chat_command_trigger_yields_twitch_chat_message() {
        let trigger = make_trigger(ActionId::new(), TriggerKind::TwitchChatCommand);
        let event = synthesize_test_event(&trigger, &[]);
        assert_eq!(event.kind, "chat.message");
        assert_eq!(event.source, EventSource::Twitch);
    }

    #[test]
    fn chat_command_uses_registered_command_name() {
        let action_id = ActionId::new();
        let trigger = make_trigger(action_id, TriggerKind::TwitchChatCommand);
        let cmd = make_command(action_id, "!quote");
        let event = synthesize_test_event(&trigger, &[cmd]);
        assert_eq!(event.payload["message"].as_str().unwrap(), "!quote");
    }

    #[test]
    fn chat_command_falls_back_to_test_when_no_commands() {
        let trigger = make_trigger(ActionId::new(), TriggerKind::TwitchChatCommand);
        let event = synthesize_test_event(&trigger, &[]);
        assert_eq!(event.payload["message"].as_str().unwrap(), "!test");
    }

    #[test]
    fn any_message_trigger_yields_twitch_chat_message() {
        let trigger = make_trigger(ActionId::new(), TriggerKind::TwitchChatAnyMessage);
        let event = synthesize_test_event(&trigger, &[]);
        assert_eq!(event.kind, "chat.message");
        assert_eq!(event.source, EventSource::Twitch);
    }

    #[test]
    fn obs_scene_changed_yields_scene_changed_with_obs_source() {
        let trigger = make_trigger(
            ActionId::new(),
            TriggerKind::ObsSceneChanged {
                scene: Some("Gaming".to_string()),
            },
        );
        let event = synthesize_test_event(&trigger, &[]);
        assert_eq!(event.kind, "scene.changed");
        assert_eq!(event.source, EventSource::Obs);
        assert_eq!(event.payload["scene"].as_str().unwrap(), "Gaming");
    }

    #[test]
    fn obs_scene_changed_none_uses_test_scene() {
        let trigger = make_trigger(
            ActionId::new(),
            TriggerKind::ObsSceneChanged { scene: None },
        );
        let event = synthesize_test_event(&trigger, &[]);
        assert_eq!(event.payload["scene"].as_str().unwrap(), "TestScene");
    }

    #[test]
    fn subscribe_trigger_yields_sub_received() {
        let trigger = make_trigger(ActionId::new(), TriggerKind::TwitchSubscribe);
        let event = synthesize_test_event(&trigger, &[]);
        assert_eq!(event.kind, "sub.received");
        assert_eq!(event.source, EventSource::Twitch);
    }

    #[test]
    fn raid_trigger_yields_raid_received() {
        let trigger = make_trigger(ActionId::new(), TriggerKind::TwitchRaid);
        let event = synthesize_test_event(&trigger, &[]);
        assert_eq!(event.kind, "raid.received");
        assert_eq!(event.source, EventSource::Twitch);
    }
}
