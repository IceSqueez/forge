use forge_types::SubActionStep;
use serde::{Deserialize, Serialize};

use crate::EmulatorError;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    #[serde(default)]
    pub twitch: Option<TwitchAccount>,
    #[serde(default)]
    pub chat_commands: Vec<ChatCommand>,
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

impl Fixture {
    pub fn chat_command_mvp() -> Self {
        Self {
            twitch: Some(TwitchAccount::default()),
            chat_commands: vec![ChatCommand {
                phrase: "!ping".to_owned(),
                action_name: "Ping".to_owned(),
                steps: Vec::new(),
            }],
        }
    }

    pub fn validate(&self) -> Result<(), EmulatorError> {
        for command in &self.chat_commands {
            // An empty phrase is stored without complaint but never matches a message.
            if command.phrase.is_empty() {
                return Err(EmulatorError::InvalidFixture {
                    reason: format!(
                        "chat command for action `{}` has an empty phrase",
                        command.action_name
                    ),
                });
            }
        }
        Ok(())
    }
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

    #[test]
    fn unknown_fixture_fields_are_rejected_rather_than_ignored() {
        for json in [
            r#"{"twich": {}}"#,
            r#"{"twitch": {"client_secret": "x"}}"#,
            r#"{"chat_commands": [{"phrase": "!a", "action_name": "A", "cooldown": 5}]}"#,
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
