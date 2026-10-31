use std::str::FromStr;

pub use forge_events::{ActingSender, SENT_AS_FIELD, SenderRole};
use serde::{Deserialize, Serialize};

use crate::error::PlatformError;

pub const SENDER_FIELD: &str = "sender";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SenderPreference {
    #[default]
    Default,
    Streamer,
    #[serde(rename = "bot")]
    BotOnly,
}

impl SenderPreference {
    pub const ALL: [SenderPreference; 3] = [Self::Default, Self::Streamer, Self::BotOnly];

    pub const WIRE_NAMES: &'static [&'static str] = &[
        Self::ALL[0].as_str(),
        Self::ALL[1].as_str(),
        Self::ALL[2].as_str(),
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Streamer => "streamer",
            Self::BotOnly => "bot",
        }
    }
}

impl FromStr for SenderPreference {
    type Err = PlatformError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|sender| sender.as_str() == raw)
            .ok_or_else(|| PlatformError::UnknownSender {
                raw: raw.to_owned(),
            })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parse_rejects_every_non_canonical_wire_name() {
        for raw in [
            "Bot",
            "STREAMER",
            "",
            " bot",
            "bot ",
            "bot_only",
            "broadcaster",
        ] {
            assert!(
                matches!(
                    raw.parse::<SenderPreference>(),
                    Err(PlatformError::UnknownSender { raw: named }) if named == raw
                ),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn unknown_sender_text_names_the_value_and_every_valid_sender() {
        let reason = "robot".parse::<SenderPreference>().unwrap_err().to_string();

        assert!(reason.contains("\"robot\""), "{reason}");
        for valid in ["default", "streamer", "bot"] {
            assert!(reason.contains(valid), "{reason} misses {valid}");
        }
    }

    #[test]
    fn every_sender_serializes_parses_and_lists_under_the_same_wire_name() {
        let listed: Vec<&str> = SenderPreference::ALL
            .iter()
            .map(|sender| sender.as_str())
            .collect();
        assert_eq!(listed, SenderPreference::WIRE_NAMES);
        for sender in SenderPreference::ALL {
            assert_eq!(
                serde_json::to_value(sender).unwrap(),
                json!(sender.as_str())
            );
            assert_eq!(sender.as_str().parse::<SenderPreference>().unwrap(), sender);
        }
    }

    #[test]
    fn acting_sender_is_recorded_under_sent_as_with_its_role_and_login() {
        for (role, wire) in [(SenderRole::Bot, "bot"), (SenderRole::Streamer, "streamer")] {
            let mut payload = json!({ "channel": "twitch", "message": "hi" });

            ActingSender {
                role,
                login: "forgebot".to_owned(),
            }
            .insert_into(&mut payload);

            assert_eq!(
                payload,
                json!({
                    "channel": "twitch",
                    "message": "hi",
                    SENT_AS_FIELD: { "role": wire, "login": "forgebot" },
                })
            );
        }
    }

    #[test]
    fn acting_sender_leaves_a_non_object_payload_untouched() {
        let mut payload = json!("plain");

        ActingSender {
            role: SenderRole::Bot,
            login: "forgebot".to_owned(),
        }
        .insert_into(&mut payload);

        assert_eq!(payload, json!("plain"));
    }
}
