use forge_events::TypedEvent;
use serde::{Deserialize, Serialize};

use super::event_kinds;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VTubeDisconnectReason {
    AwaitingApproval,
    ConnectFailed,
    AuthRequired,
    AuthDenied,
    AuthTimeout,
    AuthFailed,
    SubscribeFailed,
    SocketClosed,
    Unresponsive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VTubeConnectionChanged {
    pub is_connected: bool,
    pub endpoint: String,
    pub reason: Option<VTubeDisconnectReason>,
    pub detail: Option<String>,
}

impl TypedEvent for VTubeConnectionChanged {
    const KIND: &'static str = event_kinds::CONNECTION_CHANGED;
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::VTubeDisconnectReason;

    #[test]
    fn every_disconnect_reason_round_trips_through_its_snake_case_wire_word() {
        for (reason, word) in [
            (VTubeDisconnectReason::AwaitingApproval, "awaiting_approval"),
            (VTubeDisconnectReason::ConnectFailed, "connect_failed"),
            (VTubeDisconnectReason::AuthRequired, "auth_required"),
            (VTubeDisconnectReason::AuthDenied, "auth_denied"),
            (VTubeDisconnectReason::AuthTimeout, "auth_timeout"),
            (VTubeDisconnectReason::AuthFailed, "auth_failed"),
            (VTubeDisconnectReason::SubscribeFailed, "subscribe_failed"),
            (VTubeDisconnectReason::SocketClosed, "socket_closed"),
            (VTubeDisconnectReason::Unresponsive, "unresponsive"),
        ] {
            assert_eq!(
                (
                    serde_json::to_value(reason).unwrap(),
                    serde_json::from_value::<VTubeDisconnectReason>(json!(word)).ok(),
                ),
                (json!(word), Some(reason)),
                "{word}"
            );
        }
    }

    #[test]
    fn disconnect_reason_rejects_words_outside_the_vocabulary() {
        for raw in [
            json!(""),
            json!("AUTH_DENIED"),
            json!("authDenied"),
            json!("AuthDenied"),
            json!("auth-denied"),
            json!(" auth_denied"),
            json!("something_new"),
            json!(7),
            json!(null),
        ] {
            assert!(
                serde_json::from_value::<VTubeDisconnectReason>(raw.clone()).is_err(),
                "{raw}"
            );
        }
    }
}
