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
    fn serde_roundtrip_all_variants() {
        for variant in [PlatformId::Twitch, PlatformId::YouTube, PlatformId::Kick] {
            let json = serde_json::to_string(&variant).unwrap();
            let back: PlatformId = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back);
        }
    }
}
