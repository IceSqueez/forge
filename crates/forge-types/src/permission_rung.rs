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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const LADDER: [PermissionRung; 5] = [
        PermissionRung::Everyone,
        PermissionRung::Subscriber,
        PermissionRung::Vip,
        PermissionRung::Moderator,
        PermissionRung::Broadcaster,
    ];

    #[test]
    fn ladder_orders_authority_from_everyone_up_to_broadcaster() {
        for (i, lower) in LADDER.iter().enumerate() {
            for (j, higher) in LADDER.iter().enumerate() {
                assert_eq!(
                    lower < higher,
                    i < j,
                    "{lower} vs {higher}: ladder position {i} vs {j}"
                );
            }
        }
        // The floor is the default: a missing or unreadable role signal must never authorize.
        assert_eq!(PermissionRung::default(), PermissionRung::Everyone);
    }

    #[test]
    fn from_badges_resolves_the_highest_channel_role_present() {
        for (badges, expected) in [
            (vec![], PermissionRung::Everyone),
            (
                vec![UserBadge::Subscriber { months: 3 }],
                PermissionRung::Subscriber,
            ),
            // A legacy channel subscriber, so it satisfies the subscriber rung.
            (vec![UserBadge::Founder], PermissionRung::Subscriber),
            // YouTube channel membership normalizes to Member, and that is what satisfies
            // the subscriber rung there; the level string carries no ladder meaning.
            (
                vec![UserBadge::Member {
                    level: "gold".to_owned(),
                }],
                PermissionRung::Subscriber,
            ),
            (vec![UserBadge::Vip], PermissionRung::Vip),
            (vec![UserBadge::Moderator], PermissionRung::Moderator),
            (vec![UserBadge::Broadcaster], PermissionRung::Broadcaster),
            (
                vec![UserBadge::Broadcaster, UserBadge::Subscriber { months: 1 }],
                PermissionRung::Broadcaster,
            ),
            (
                vec![UserBadge::Subscriber { months: 1 }, UserBadge::Broadcaster],
                PermissionRung::Broadcaster,
            ),
            (
                vec![
                    UserBadge::Vip,
                    UserBadge::Moderator,
                    UserBadge::Subscriber { months: 9 },
                ],
                PermissionRung::Moderator,
            ),
        ] {
            assert_eq!(
                PermissionRung::from_badges(&badges),
                expected,
                "badges: {badges:?}"
            );
        }
    }

    #[test]
    fn account_level_and_cosmetic_badges_never_authorize() {
        // Turbo, Prime and partner are account-wide, not channel-scoped: they assert no role.
        let non_roles = [
            UserBadge::Turbo,
            UserBadge::Premium,
            UserBadge::Partner,
            UserBadge::Bot,
            UserBadge::HypeTrain,
            UserBadge::Bits { amount: 5000 },
            UserBadge::BitsLeader { rank: 1 },
        ];
        for badge in &non_roles {
            assert_eq!(
                PermissionRung::from_badges(std::slice::from_ref(badge)),
                PermissionRung::Everyone,
                "{badge:?} must not authorize"
            );
        }

        let mut with_subscriber = non_roles.to_vec();
        with_subscriber.push(UserBadge::Subscriber { months: 1 });
        assert_eq!(
            PermissionRung::from_badges(&with_subscriber),
            PermissionRung::Subscriber,
            "cosmetics alongside a real role must not lift the resolved rung"
        );
    }

    #[test]
    fn a_narrower_badge_vocabulary_never_resolves_higher() {
        // Collapse never weakens: a rung a platform cannot express is simply absent from the
        // badge list, so the resolution must be monotone under badge-set inclusion - fewer
        // badges may only tighten the gate.
        let universe = [
            UserBadge::Broadcaster,
            UserBadge::Moderator,
            UserBadge::Vip,
            UserBadge::Subscriber { months: 2 },
            UserBadge::Turbo,
        ];
        let subset = |mask: u32| -> Vec<UserBadge> {
            universe
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, b)| b.clone())
                .collect()
        };

        let full = 1u32 << universe.len();
        for narrow in 0..full {
            for wide in 0..full {
                if narrow & wide != narrow {
                    continue;
                }
                let narrow_rung = PermissionRung::from_badges(&subset(narrow));
                let wide_rung = PermissionRung::from_badges(&subset(wide));
                assert!(
                    narrow_rung <= wide_rung,
                    "{:?} resolved {narrow_rung} above its superset {:?} at {wide_rung}",
                    subset(narrow),
                    subset(wide)
                );
            }
        }
    }

    #[test]
    fn rung_round_trips_through_its_persisted_and_serde_forms() {
        for rung in LADDER {
            assert_eq!(rung.as_str().parse::<PermissionRung>().unwrap(), rung);
            // The column form and the serde wire form must not drift apart - the same value is
            // written to the trigger_instances column and to exported trigger JSON.
            assert_eq!(
                serde_json::to_string(&rung).unwrap(),
                format!("\"{}\"", rung.as_str())
            );
        }
    }

    #[test]
    fn from_str_rejects_non_canonical_names_preserving_the_input() {
        for bad in ["Everyone", "VIP", "mods", "", "vip ", "owner", "regular"] {
            assert_eq!(
                bad.parse::<PermissionRung>().unwrap_err(),
                PermissionRungError::Unknown(bad.to_owned())
            );
        }
    }
}
