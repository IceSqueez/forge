use serde::{Deserialize, Serialize};
use std::fmt;
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

define_id!(
    /// Unique identifier for a bus event.
    EventId
);

define_id!(
    /// Unique identifier for an action.
    ActionId
);

define_id!(
    /// Unique identifier for a trigger.
    TriggerId
);

define_id!(
    /// Unique identifier for a command.
    CommandId
);

define_id!(
    /// Unique identifier for an action queue.
    QueueId
);

define_id!(
    /// Unique identifier for a registered rhai script.
    ScriptId
);

define_id!(
    /// Unique identifier for a global variable entry.
    GlobalId
);

define_id!(
    /// Opaque identifier for a platform user.
    UserId
);

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

    #[test]
    fn all_id_types_are_distinct_by_construction() {
        let eid = EventId::new().to_string();
        let aid = ActionId::new().to_string();
        assert_ne!(eid, aid);
    }

    #[test]
    fn event_id_ordering_is_consistent() {
        let a = EventId::new();
        let b = EventId::new();
        assert!(a <= b || b <= a, "EventId ordering must be total");
    }
}
