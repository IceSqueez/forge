use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaidDirection {
    Received,
    Sent,
}

impl RaidDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Sent => "sent",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RaidDirection;

    #[test]
    fn raid_direction_wire_names_match_as_str_and_round_trip() {
        for (direction, wire) in [
            (RaidDirection::Received, "received"),
            (RaidDirection::Sent, "sent"),
        ] {
            assert_eq!(
                (
                    serde_json::to_value(direction).unwrap(),
                    direction.as_str(),
                    serde_json::from_value::<RaidDirection>(serde_json::json!(wire)).unwrap(),
                ),
                (serde_json::json!(wire), wire, direction)
            );
        }
    }

    #[test]
    fn raid_direction_rejects_names_outside_the_wire_vocabulary() {
        for wire in ["Received", "SENT", "incoming", "", "received "] {
            assert!(
                serde_json::from_value::<RaidDirection>(serde_json::json!(wire)).is_err(),
                "{wire:?} must not decode"
            );
        }
    }
}
