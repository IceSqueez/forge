use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::unified_chat::UserBadge;

/// Declaration order is the ladder: the derived `Ord` is the authorization comparison, not an incidental one.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRung {
    #[default]
    Everyone,
    Subscriber,
    Vip,
    Moderator,
    Broadcaster,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PermissionRungError {
    #[error("unknown permission rung: {0}")]
    Unknown(String),
}

impl PermissionRung {
    /// Highest rung wins; an empty badge list resolves to the floor, so badge-less events never authorize.
    pub fn from_badges(badges: &[UserBadge]) -> Self {
        badges
            .iter()
            .filter_map(Self::from_badge)
            .max()
            .unwrap_or_default()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Everyone => "everyone",
            Self::Subscriber => "subscriber",
            Self::Vip => "vip",
            Self::Moderator => "moderator",
            Self::Broadcaster => "broadcaster",
        }
    }

    fn from_badge(badge: &UserBadge) -> Option<Self> {
        match badge {
            UserBadge::Broadcaster => Some(Self::Broadcaster),
            UserBadge::Moderator => Some(Self::Moderator),
            UserBadge::Vip => Some(Self::Vip),
            UserBadge::Subscriber { .. } | UserBadge::Founder | UserBadge::Member { .. } => {
                Some(Self::Subscriber)
            }
            // Account-level and cosmetic badges assert no channel role and must never satisfy a rung.
            UserBadge::Bot
            | UserBadge::Partner
            | UserBadge::Premium
            | UserBadge::Turbo
            | UserBadge::HypeTrain
            | UserBadge::Bits { .. }
            | UserBadge::BitsLeader { .. } => None,
        }
    }
}

impl fmt::Display for PermissionRung {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PermissionRung {
    type Err = PermissionRungError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "everyone" => Ok(Self::Everyone),
            "subscriber" => Ok(Self::Subscriber),
            "vip" => Ok(Self::Vip),
            "moderator" => Ok(Self::Moderator),
            "broadcaster" => Ok(Self::Broadcaster),
            _ => Err(PermissionRungError::Unknown(s.to_owned())),
        }
    }
}
