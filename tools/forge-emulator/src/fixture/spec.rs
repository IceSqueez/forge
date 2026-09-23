use forge_storage::OverlayConfig;
use forge_types::{SubActionStep, TriggerConfig};
use serde::{Deserialize, Serialize};

use crate::EmulatorError;

/// The step kind that addresses an overlay, and the config key naming its target.
pub const OVERLAY_SEND_KIND: &str = "overlay.send";
pub const OVERLAY_TARGET_KEY: &str = "overlay_id";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    #[serde(default)]
    pub twitch: Option<TwitchAccount>,
    #[serde(default)]
    pub overlays: Vec<OverlayFixture>,
    #[serde(default)]
    pub chat_commands: Vec<ChatCommand>,
    #[serde(default)]
    pub event_triggers: Vec<EventTrigger>,
}

/// Defaults are fake values that are safe to publish in a scenario report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TwitchAccount {
    pub client_id: String,
    pub user_id: String,
    pub login: String,
    pub access_token: String,
}

impl Default for TwitchAccount {
    fn default() -> Self {
        Self {
            client_id: "emulatorclientid".to_owned(),
            user_id: "100000001".to_owned(),
            login: "forge_emulator".to_owned(),
            access_token: "emulator-twitch-access-token".to_owned(),
        }
    }
}

/// A Twitch chat-command trigger wired to its own action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatCommand {
    pub phrase: String,
    pub action_name: String,
    #[serde(default)]
    pub steps: Vec<SubActionStep>,
}

/// A trigger instance of any kind wired to its own action; `trigger_kind` is a trigger descriptor
/// id, not an event kind, and `config` fills that descriptor's own fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventTrigger {
    pub trigger_kind: String,
    pub action_name: String,
    #[serde(default)]
    pub config: TriggerConfig,
    #[serde(default)]
    pub steps: Vec<SubActionStep>,
}

pub struct FixtureAction<'a> {
    pub location: String,
    pub name: &'a str,
    pub steps: &'a [SubActionStep],
}

/// An overlay the fixture creates through forge's own repository, which mints its identity slug
/// and its page credential. Scenarios and `overlay.send` steps name it by `display_name`; the
/// seeder rewrites those names to the minted identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlayFixture {
    pub display_name: String,
    pub kind_id: String,
    #[serde(default)]
    pub config: OverlayConfig,
}

impl Fixture {
    pub fn chat_command_mvp() -> Self {
        Self {
            twitch: Some(TwitchAccount::default()),
            overlays: Vec::new(),
            chat_commands: vec![ChatCommand {
                phrase: "!ping".to_owned(),
                action_name: "Ping".to_owned(),
                steps: Vec::new(),
            }],
            event_triggers: Vec::new(),
        }
    }

    /// Seeding order: chat commands first, then event triggers.
    pub fn actions(&self) -> Vec<FixtureAction<'_>> {
        let commands = self
            .chat_commands
            .iter()
            .enumerate()
            .map(|(index, command)| FixtureAction {
                location: format!("chat_commands[{index}]"),
                name: &command.action_name,
                steps: &command.steps,
            });
        let triggers = self
            .event_triggers
            .iter()
            .enumerate()
            .map(|(index, trigger)| FixtureAction {
                location: format!("event_triggers[{index}]"),
                name: &trigger.action_name,
                steps: &trigger.steps,
            });
        commands.chain(triggers).collect()
    }

    pub fn declares_overlay(&self, display_name: &str) -> bool {
        self.overlays
            .iter()
            .any(|overlay| overlay.display_name == display_name)
    }

    pub fn validate(&self) -> Result<(), EmulatorError> {
        let invalid = |reason: String| EmulatorError::InvalidFixture { reason };
        for (index, overlay) in self.overlays.iter().enumerate() {
            if overlay.display_name.trim().is_empty() {
                return Err(invalid(format!(
                    "overlays[{index}] has a blank display name"
                )));
            }
            if overlay.kind_id.trim().is_empty() {
                return Err(invalid(format!(
                    "overlay `{}` names no overlay type",
                    overlay.display_name
                )));
            }
            if self.overlays[..index]
                .iter()
                .any(|earlier| earlier.display_name == overlay.display_name)
            {
                return Err(invalid(format!(
                    "two overlays are both named `{}`, so a step could not tell them apart",
                    overlay.display_name
                )));
            }
        }
        for command in &self.chat_commands {
            // An empty phrase is stored without complaint but never matches a message.
            if command.phrase.is_empty() {
                return Err(invalid(format!(
                    "chat command for action `{}` has an empty phrase",
                    command.action_name
                )));
            }
        }
        for trigger in &self.event_triggers {
            if trigger.trigger_kind.trim().is_empty() {
                return Err(invalid(format!(
                    "event trigger for action `{}` names no trigger kind",
                    trigger.action_name
                )));
            }
        }
        for action in self.actions() {
            for target in overlay_targets(action.steps) {
                if !self.declares_overlay(target) {
                    return Err(invalid(format!(
                        "action `{}` sends to overlay `{target}`, which the fixture does not declare",
                        action.name
                    )));
                }
            }
        }
        Ok(())
    }
}

/// The overlay each `overlay.send` step addresses, skipping steps that name none.
pub fn overlay_targets(steps: &[SubActionStep]) -> impl Iterator<Item = &str> {
    steps
        .iter()
        .filter(|step| step.kind_id == OVERLAY_SEND_KIND)
        .filter_map(|step| step.config.get(OVERLAY_TARGET_KEY))
        .filter_map(forge_types::Variant::as_str)
        .filter(|target| !target.trim().is_empty())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn empty_command_phrase_is_refused_naming_its_action() {
        let mut fixture = Fixture::chat_command_mvp();
        fixture.chat_commands.push(ChatCommand {
            phrase: String::new(),
            action_name: "Silent".to_owned(),
            steps: Vec::new(),
        });
        let refusal = fixture.validate();
        assert!(
            matches!(&refusal, Err(EmulatorError::InvalidFixture { reason }) if reason.contains("`Silent`")),
            "got {refusal:?}"
        );
    }

    #[test]
    fn single_character_phrase_is_accepted() {
        let mut fixture = Fixture::default();
        fixture.chat_commands.push(ChatCommand {
            phrase: "!".to_owned(),
            action_name: "Bang".to_owned(),
            steps: Vec::new(),
        });
        assert!(fixture.validate().is_ok());
    }

    fn overlay(display_name: &str) -> OverlayFixture {
        OverlayFixture {
            display_name: display_name.to_owned(),
            kind_id: "overlay.alert".to_owned(),
            config: OverlayConfig::new(),
        }
    }

    fn event_trigger_sending_to(target: &str) -> EventTrigger {
        EventTrigger {
            trigger_kind: "twitch.support.subscriber".to_owned(),
            action_name: "Announce".to_owned(),
            config: TriggerConfig::new(),
            steps: sends_to(target).steps,
        }
    }

    fn sends_to(target: &str) -> ChatCommand {
        ChatCommand {
            phrase: "!alert".to_owned(),
            action_name: "Raise".to_owned(),
            steps: vec![SubActionStep {
                kind_id: OVERLAY_SEND_KIND.to_owned(),
                config: [(
                    OVERLAY_TARGET_KEY.to_owned(),
                    forge_types::Variant::String(target.to_owned()),
                )]
                .into_iter()
                .collect(),
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            }],
        }
    }

    #[test]
    fn overlay_declarations_that_no_step_could_address_are_refused() {
        for (fixture, expected, label) in [
            (
                Fixture {
                    overlays: vec![overlay("   ")],
                    ..Fixture::default()
                },
                "blank display name",
                "an overlay with nothing to call it by",
            ),
            (
                Fixture {
                    overlays: vec![OverlayFixture {
                        kind_id: String::new(),
                        ..overlay("Alert Box")
                    }],
                    ..Fixture::default()
                },
                "names no overlay type",
                "an overlay with no type to render it",
            ),
            (
                Fixture {
                    overlays: vec![overlay("Alert Box"), overlay("Alert Box")],
                    ..Fixture::default()
                },
                "both named",
                "two overlays sharing one name",
            ),
            (
                Fixture {
                    overlays: vec![overlay("Alert Box")],
                    chat_commands: vec![sends_to("Goal Bar")],
                    ..Fixture::default()
                },
                "does not declare",
                "a chat command step addressing an overlay nothing seeds",
            ),
            (
                Fixture {
                    overlays: vec![overlay("Alert Box")],
                    event_triggers: vec![event_trigger_sending_to("Goal Bar")],
                    ..Fixture::default()
                },
                "does not declare",
                "an event trigger step addressing an overlay nothing seeds",
            ),
            (
                Fixture {
                    event_triggers: vec![EventTrigger {
                        trigger_kind: "  ".to_owned(),
                        ..event_trigger_sending_to("Alert Box")
                    }],
                    ..Fixture::default()
                },
                "names no trigger kind",
                "an event trigger with no descriptor to fire it",
            ),
        ] {
            let refusal = fixture.validate();
            assert!(
                matches!(&refusal, Err(EmulatorError::InvalidFixture { reason }) if reason.contains(expected)),
                "{label}: got {refusal:?}"
            );
        }
    }

    #[test]
    fn a_step_addressing_a_declared_overlay_is_accepted() {
        let fixture = Fixture {
            overlays: vec![overlay("Alert Box")],
            chat_commands: vec![sends_to("Alert Box")],
            ..Fixture::default()
        };

        assert!(fixture.validate().is_ok());
    }

    #[test]
    fn a_step_that_names_no_overlay_yet_is_left_for_forge_to_refuse_at_run_time() {
        let fixture = Fixture {
            chat_commands: vec![sends_to("   ")],
            ..Fixture::default()
        };

        assert!(fixture.validate().is_ok());
    }

    #[test]
    fn unknown_fixture_fields_are_rejected_rather_than_ignored() {
        for json in [
            r#"{"twich": {}}"#,
            r#"{"twitch": {"client_secret": "x"}}"#,
            r#"{"chat_commands": [{"phrase": "!a", "action_name": "A", "cooldown": 5}]}"#,
            r#"{"overlays": [{"display_name": "A", "kind_id": "overlay.alert", "enabled": true}]}"#,
            r#"{"event_triggers": [{"trigger_kind": "twitch.support.subscriber", "action_name": "A", "cooldown": 5}]}"#,
            r#"{"event_triggers": [{"kind_id": "twitch.support.subscriber", "action_name": "A"}]}"#,
        ] {
            assert!(
                serde_json::from_str::<Fixture>(json).is_err(),
                "expected rejection for {json}"
            );
        }
    }

    #[test]
    fn partial_twitch_account_fills_the_remaining_fields_with_fakes() {
        let fixture: Fixture = serde_json::from_str(r#"{"twitch": {"login": "alice"}}"#).unwrap();
        let account = fixture.twitch.unwrap();
        assert_eq!(account.login, "alice");
        assert_eq!(account.client_id, TwitchAccount::default().client_id);
    }
}
