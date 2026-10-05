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

    pub fn from_wire(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        Self::ALL
            .into_iter()
            .find(|platform| platform.as_str().eq_ignore_ascii_case(raw))
    }

    pub const fn supports_whispers(self) -> bool {
        matches!(self, PlatformId::Twitch)
    }
}

pub const WHISPER_RECIPIENT_FIELD: &str = "whisper_to_login";

pub const REPLY_PARENT_FIELD: &str = "reply_to_message_id";

pub fn requested_chat_target(raw: &str) -> Option<&str> {
    let target = raw.trim();
    (!target.is_empty()).then_some(target)
}

pub fn resolve_chat_target(raw: &str) -> Result<Option<PlatformId>, String> {
    let Some(target) = requested_chat_target(raw) else {
        return Ok(None);
    };
    PlatformId::from_wire(target)
        .map(Some)
        .ok_or_else(|| unknown_chat_target_reason(target))
}

pub fn unknown_chat_target_reason(target: &str) -> String {
    let valid = PlatformId::ALL.map(PlatformId::as_str).join(", ");
    format!("unknown chat target \"{target}\"; valid targets: {valid}")
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
    fn from_wire_accepts_any_case_and_padding_and_rejects_everything_else() {
        for (raw, expected) in [
            ("Twitch", Some(PlatformId::Twitch)),
            (" KICK\t", Some(PlatformId::Kick)),
            ("YouTube", Some(PlatformId::YouTube)),
            ("youtube", Some(PlatformId::YouTube)),
            ("", None),
            ("   ", None),
            ("twitc", None),
            ("twitch2", None),
            ("tw itch", None),
        ] {
            assert_eq!(PlatformId::from_wire(raw), expected, "raw {raw:?}");
        }
    }

    #[test]
    fn resolve_chat_target_reads_blank_as_broadcast_and_normalizes_known_platforms() {
        for (raw, expected) in [
            ("", None),
            (" \t", None),
            ("Twitch", Some(PlatformId::Twitch)),
            ("  YOUTUBE ", Some(PlatformId::YouTube)),
            ("kick", Some(PlatformId::Kick)),
        ] {
            assert_eq!(resolve_chat_target(raw), Ok(expected), "raw {raw:?}");
        }
    }

    #[test]
    fn resolve_chat_target_rejects_unknown_names_listing_every_valid_target() {
        let reason = resolve_chat_target(" myspace ").unwrap_err();

        assert!(reason.contains("\"myspace\""), "{reason}");
        for valid in ["twitch", "youtube", "kick"] {
            assert!(reason.contains(valid), "{reason}");
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
