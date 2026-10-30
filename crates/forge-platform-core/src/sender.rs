pub use forge_events::{ActingSender, SENT_AS_FIELD, SenderRole};
use serde::{Deserialize, Serialize};

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

    pub const WIRE_NAMES: &'static [&'static str] = &["default", "streamer", "bot"];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Streamer => "streamer",
            Self::BotOnly => "bot",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|sender| sender.as_str() == raw)
    }

    pub fn from_payload(payload: &serde_json::Value) -> Result<Self, String> {
        match payload.get(SENDER_FIELD) {
            None | Some(serde_json::Value::Null) => Ok(Self::Default),
            Some(serde_json::Value::String(raw)) => {
                Self::parse(raw).ok_or_else(|| unknown_sender_reason(raw))
            }
            Some(other) => Err(unknown_sender_reason(&other.to_string())),
        }
    }
}

pub fn unknown_sender_reason(raw: &str) -> String {
    let valid = SenderPreference::WIRE_NAMES.join(", ");
    format!("unknown sender \"{raw}\"; valid senders: {valid}")
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
            assert_eq!(SenderPreference::parse(raw), None, "{raw:?}");
        }
    }

    #[test]
    fn unknown_sender_reason_names_the_value_and_every_valid_sender() {
        let reason = unknown_sender_reason("robot");

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
            assert_eq!(SenderPreference::parse(sender.as_str()), Some(sender));
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
