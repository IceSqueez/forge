use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformId {
    Twitch,
    #[serde(rename = "youtube")]
    YouTube,
    Kick,
}

impl PlatformId {
    pub const ALL: [PlatformId; 3] = [PlatformId::Twitch, PlatformId::YouTube, PlatformId::Kick];

    pub const fn as_str(self) -> &'static str {
        match self {
            PlatformId::Twitch => "twitch",
            PlatformId::YouTube => "youtube",
            PlatformId::Kick => "kick",
        }
    }
}

pub fn requested_chat_target(raw: &str) -> Option<&str> {
    let target = raw.trim();
    (!target.is_empty()).then_some(target)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn platform_id_wire_strings_match_protocol_contract() {
        for (variant, expected_wire) in [
            (PlatformId::Twitch, "twitch"),
            (PlatformId::YouTube, "youtube"),
            (PlatformId::Kick, "kick"),
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(
                json,
                format!("\"{expected_wire}\""),
                "{variant:?} serializes to wrong wire string"
            );
            let back: PlatformId = serde_json::from_str(&json).unwrap();
            assert_eq!(
                back, variant,
                "deserializing {json} did not yield {variant:?}"
            );
        }
    }

    #[test]
    fn requested_chat_target_trims_padding_and_reads_blank_as_broadcast() {
        for (raw, expected) in [
            ("twitch", Some("twitch")),
            ("  kick \t", Some("kick")),
            ("\r\nyoutube\n", Some("youtube")),
            ("my chat", Some("my chat")),
            ("", None),
            ("   ", None),
            ("\t\r\n", None),
            ("\u{3000}\u{a0}", None),
        ] {
            assert_eq!(requested_chat_target(raw), expected, "raw {raw:?}");
        }
    }
}
