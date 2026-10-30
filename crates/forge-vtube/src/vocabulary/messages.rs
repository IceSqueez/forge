use forge_types::{MessageCatalog, MessageCode, MessageRef};

use crate::vocabulary::{INTEGRATION_ID, VTubeDisconnectReason};

const CONNECT_REFUSED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "connect_refused");
const UNREACHABLE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "unreachable");
const CONNECT_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "connect_failed");
const TOKEN_REJECTED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "token_rejected");
const ACCESS_DENIED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "access_denied");
const APPROVAL_TIMEOUT: MessageCode = MessageCode::from_static(INTEGRATION_ID, "approval_timeout");
const AUTH_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "auth_failed");
const SUBSCRIBE_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "subscribe_failed");
const CONNECTION_LOST: MessageCode = MessageCode::from_static(INTEGRATION_ID, "connection_lost");
const UNRESPONSIVE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "unresponsive");

pub const VTUBE_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        CONNECT_REFUSED,
        UNREACHABLE,
        CONNECT_FAILED,
        TOKEN_REJECTED,
        ACCESS_DENIED,
        APPROVAL_TIMEOUT,
        AUTH_FAILED,
        SUBSCRIBE_FAILED,
        CONNECTION_LOST,
        UNRESPONSIVE,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

pub(crate) fn connect_refused() -> MessageRef {
    MessageRef::new(CONNECT_REFUSED)
}

pub(crate) fn unreachable() -> MessageRef {
    MessageRef::new(UNREACHABLE)
}

pub(crate) fn connect_failed() -> MessageRef {
    MessageRef::new(CONNECT_FAILED)
}

pub(crate) fn for_disconnect_reason(reason: VTubeDisconnectReason) -> Option<MessageRef> {
    let code = match reason {
        VTubeDisconnectReason::AwaitingApproval => return None,
        VTubeDisconnectReason::ConnectFailed => CONNECT_FAILED,
        VTubeDisconnectReason::AuthRequired => TOKEN_REJECTED,
        VTubeDisconnectReason::AuthDenied => ACCESS_DENIED,
        VTubeDisconnectReason::AuthTimeout => APPROVAL_TIMEOUT,
        VTubeDisconnectReason::AuthFailed => AUTH_FAILED,
        VTubeDisconnectReason::SubscribeFailed => SUBSCRIBE_FAILED,
        VTubeDisconnectReason::SocketClosed => CONNECTION_LOST,
        VTubeDisconnectReason::Unresponsive => UNRESPONSIVE,
    };
    Some(MessageRef::new(code))
}
#[cfg(test)]
mod tests {
    use super::*;

    const REASON_CODES: [(VTubeDisconnectReason, Option<&str>); 9] = [
        (VTubeDisconnectReason::AwaitingApproval, None),
        (VTubeDisconnectReason::ConnectFailed, Some("connect_failed")),
        (VTubeDisconnectReason::AuthRequired, Some("token_rejected")),
        (VTubeDisconnectReason::AuthDenied, Some("access_denied")),
        (VTubeDisconnectReason::AuthTimeout, Some("approval_timeout")),
        (VTubeDisconnectReason::AuthFailed, Some("auth_failed")),
        (
            VTubeDisconnectReason::SubscribeFailed,
            Some("subscribe_failed"),
        ),
        (VTubeDisconnectReason::SocketClosed, Some("connection_lost")),
        (VTubeDisconnectReason::Unresponsive, Some("unresponsive")),
    ];

    #[test]
    fn each_disconnect_reason_maps_to_its_own_catalogued_code_and_awaiting_approval_to_none() {
        for (reason, expected) in REASON_CODES {
            let mapped = for_disconnect_reason(reason);

            assert_eq!(
                mapped.as_ref().map(|message| message.code().name()),
                expected,
                "{reason:?}"
            );
            if let Some(message) = mapped {
                assert!(
                    VTUBE_MESSAGES.codes.contains(message.code()),
                    "{reason:?} maps to an uncatalogued code"
                );
            }
        }
    }

    #[test]
    fn every_dial_failure_code_is_listed_in_the_catalog() {
        for emitted in [connect_refused(), unreachable(), connect_failed()] {
            assert!(
                VTUBE_MESSAGES.codes.contains(emitted.code()),
                "{} is emitted but not catalogued",
                emitted.code()
            );
        }
    }
}
