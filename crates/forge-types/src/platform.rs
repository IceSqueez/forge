use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformId {
    Twitch,
    // Without this the snake_case rule yields "you_tube", which does not match the
    // "youtube" platform id emitted on the wire (connection events, chat.send targets).
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
        // Regression guard: `#[serde(rename_all = "snake_case")]` yields "you_tube" for
        // `YouTube`, NOT "youtube".  The explicit `#[serde(rename = "youtube")]` annotation
        // on the variant corrects this.  A symmetric roundtrip test passes either way
        // ("you_tube" round-trips back to YouTube), so we MUST assert the exact serialized
        // bytes to catch a future annotation removal.
        //
        // Why this matters: the wire string is compared against the platform id in
        // connection-state events and chat.send targets inside forge-desktop.  A mismatch
        // causes those events to be silently dropped.
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
