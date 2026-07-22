use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformId {
    Twitch,
    // snake_case would yield "you_tube"; the wire protocol uses "youtube".
    #[serde(rename = "youtube")]
    YouTube,
    Kick,
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
}
