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
            #[expect(clippy::new_without_default, reason = "a fresh random id is no default")]
            pub fn new() -> Self {
                Self(Ulid::generate())
            }
        }

        impl FromStr for $name {
            type Err = ulid::DecodeError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                s.parse::<Ulid>().map(Self)
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
define_id!(ClipId);

pub const DEFAULT_QUEUE_ID: QueueId = QueueId(Ulid::nil());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_id_new_is_unique() {
        let a = EventId::new();
        let b = EventId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn event_id_displays_as_a_parseable_ulid() {
        assert!(ulid::Ulid::from_string(&EventId::new().to_string()).is_ok());
    }
}
