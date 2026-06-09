use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformId {
    Twitch,
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
