use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlatformId {
    Twitch,
    YouTube,
    Kick,
    Trovo,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip_all_variants() {
        for variant in [
            PlatformId::Twitch,
            PlatformId::YouTube,
            PlatformId::Kick,
            PlatformId::Trovo,
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            let back: PlatformId = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back);
        }
    }

    #[test]
    fn copy_semantics() {
        let a = PlatformId::Twitch;
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn hash_usable_as_map_key() {
        use std::collections::HashMap;
        let mut m: HashMap<PlatformId, &str> = HashMap::new();
        m.insert(PlatformId::Twitch, "twitch");
        assert_eq!(m[&PlatformId::Twitch], "twitch");
    }
}
