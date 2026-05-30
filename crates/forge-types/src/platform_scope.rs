use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::PlatformId;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformScope {
    #[default]
    Any,
    /// INVARIANT: the set is never empty.
    /// Construct via [`PlatformScope::only`]. Direct enum construction outside of
    /// `forge-types` tests is forbidden by convention; deserialization runs through
    /// the `TryFrom<PlatformScopeRaw>` impl, which enforces the same invariant.
    Only(BTreeSet<PlatformId>),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PlatformScopeError {
    #[error("PlatformScope::Only requires at least one platform")]
    EmptySet,
}

impl PlatformScope {
    pub fn only(platforms: BTreeSet<PlatformId>) -> Result<Self, PlatformScopeError> {
        if platforms.is_empty() {
            return Err(PlatformScopeError::EmptySet);
        }
        Ok(Self::Only(platforms))
    }

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
    fn only_rejects_empty_set() {
        assert!(PlatformScope::only(BTreeSet::new()).is_err());
    }

    #[test]
    fn only_accepts_non_empty_set() {
        let mut set = BTreeSet::new();
        set.insert(PlatformId::Twitch);
        assert!(PlatformScope::only(set).is_ok());
    }

    #[test]
    fn any_matches_every_source() {
        assert!(PlatformScope::Any.matches(Some(PlatformId::Twitch)));
        assert!(PlatformScope::Any.matches(None));
    }

    #[test]
    fn only_matches_listed_platform() {
        let mut set = BTreeSet::new();
        set.insert(PlatformId::Twitch);
        let scope = PlatformScope::only(set).unwrap();
        assert!(scope.matches(Some(PlatformId::Twitch)));
        assert!(!scope.matches(Some(PlatformId::YouTube)));
        assert!(!scope.matches(None));
    }

    #[test]
    fn default_is_any() {
        assert_eq!(PlatformScope::default(), PlatformScope::Any);
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
        let scope = PlatformScope::only(set).unwrap();
        let json = serde_json::to_string(&scope).unwrap();
        assert_eq!(json, r#"{"only":["twitch"]}"#);
        let back: PlatformScope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, scope);
    }
}
