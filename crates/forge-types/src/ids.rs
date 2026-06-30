use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use ulid::Ulid;

macro_rules! define_id {
    ($(#[$attr:meta])* $name:ident) => {
        $(#[$attr])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Ulid);

        impl $name {
            pub fn new() -> Self {
                Self(Ulid::new())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<Ulid> for $name {
            fn from(ulid: Ulid) -> Self {
                Self(ulid)
            }
        }
    };
}

define_id!(EventId);
define_id!(ActionId);
define_id!(TriggerInstanceId);
define_id!(QueueId);
define_id!(ScriptId);
define_id!(GlobalId);
define_id!(UserId);
define_id!(ClipId);

impl FromStr for ActionId {
    type Err = ulid::DecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<Ulid>().map(Self)
    }
}

impl FromStr for TriggerInstanceId {
    type Err = ulid::DecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<Ulid>().map(Self)
    }
}

impl FromStr for QueueId {
    type Err = ulid::DecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<Ulid>().map(Self)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn event_id_new_is_unique() {
        let a = EventId::new();
        let b = EventId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn event_id_display_roundtrips_via_debug() {
        let id = EventId::new();
        let s = id.to_string();
        assert_eq!(s.len(), 26, "ULID string length must be 26 chars");
    }

    #[test]
    fn event_id_serde_roundtrip() {
        let id = EventId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: EventId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }
}
