use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::PlatformId;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformScope {
    #[default]
    Any,
    Only(BTreeSet<PlatformId>),
}

impl PlatformScope {
    pub fn matches(&self, platform: Option<PlatformId>) -> bool {
        match self {
            Self::Any => true,
            Self::Only(set) => {
                debug_assert!(!set.is_empty(), "PlatformScope::Only invariant violated");
                platform.is_some_and(|p| set.contains(&p))
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn any_matches_every_source() {
        assert!(PlatformScope::Any.matches(Some(PlatformId::Twitch)));
        assert!(PlatformScope::Any.matches(None));
    }

    #[test]
    fn only_matches_listed_platform() {
        let mut set = BTreeSet::new();
        set.insert(PlatformId::Twitch);
        let scope = PlatformScope::Only(set);
        assert!(scope.matches(Some(PlatformId::Twitch)));
        assert!(!scope.matches(Some(PlatformId::YouTube)));
        assert!(!scope.matches(None));
    }

    #[test]
    fn any_serde_roundtrip() {
        let scope = PlatformScope::Any;
        let json = serde_json::to_string(&scope).unwrap();
        assert_eq!(json, r#""any""#);
        let back: PlatformScope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, PlatformScope::Any);
    }

    #[test]
    fn only_serde_roundtrip() {
        let mut set = BTreeSet::new();
        set.insert(PlatformId::Twitch);
        let scope = PlatformScope::Only(set);
        let json = serde_json::to_string(&scope).unwrap();
        assert_eq!(json, r#"{"only":["twitch"]}"#);
        let back: PlatformScope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, scope);
    }
}
