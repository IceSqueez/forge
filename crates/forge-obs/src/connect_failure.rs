use std::error::Error;
use std::io::ErrorKind;

use forge_platform_core::CredentialState;
use forge_types::MessageRef;

use crate::vocabulary::messages;

pub(crate) fn credential_state_after(failure: Option<&MessageRef>) -> CredentialState {
    if failure == Some(&messages::password_rejected()) {
        CredentialState::Rejected
    } else {
        CredentialState::Valid
    }
}

pub(crate) fn connect_failure_message(error: &obws::error::Error) -> MessageRef {
    match error {
        obws::error::Error::Timeout => messages::unreachable(),
        obws::error::Error::Handshake(obws::client::HandshakeError::NoIdentified) => {
            messages::password_rejected()
        }
        obws::error::Error::Handshake(obws::client::HandshakeError::ConnectionClosed(Some(
            details,
        ))) if u16::from(details.code)
            == u16::from(obws::responses::WebSocketCloseCode::AuthenticationFailed) =>
        {
            messages::password_rejected()
        }
        obws::error::Error::Connect(_) => match io_error_kind(error) {
            Some(ErrorKind::ConnectionRefused) => messages::connect_refused(),
            Some(_) => messages::unreachable(),
            None => messages::connect_failed(),
        },
        _ => messages::connect_failed(),
    }
}

fn io_error_kind(error: &(dyn Error + 'static)) -> Option<ErrorKind> {
    let mut current = error.source();
    while let Some(source) = current {
        if let Some(io_error) = source.downcast_ref::<std::io::Error>() {
            return Some(io_error.kind());
        }
        current = source.source();
    }
    None
}
#[cfg(test)]
mod tests {
    use obws::client::HandshakeError;
    use obws::error::Error;

    use forge_platform_core::CredentialState;

    use super::{connect_failure_message, credential_state_after};
    use crate::vocabulary::messages;

    #[test]
    fn only_a_rejected_password_counts_as_a_rejected_credential() {
        for (failure, expected) in [
            (None, CredentialState::Valid),
            (Some(messages::unreachable()), CredentialState::Valid),
            (Some(messages::connection_lost()), CredentialState::Valid),
            (
                Some(messages::password_rejected()),
                CredentialState::Rejected,
            ),
        ] {
            assert_eq!(
                credential_state_after(failure.as_ref()),
                expected,
                "{failure:?}"
            );
        }
    }

    #[test]
    fn errors_that_carry_no_connect_cause_map_to_their_user_facing_code() {
        for (label, error, expected) in [
            ("connect timeout", Error::Timeout, "unreachable"),
            (
                "no Identified reply",
                Error::Handshake(HandshakeError::NoIdentified),
                "password_rejected",
            ),
            (
                "closed without details",
                Error::Handshake(HandshakeError::ConnectionClosed(None)),
                "connect_failed",
            ),
            (
                "no Hello",
                Error::Handshake(HandshakeError::NoHello),
                "connect_failed",
            ),
        ] {
            assert_eq!(
                connect_failure_message(&error).code().name(),
                expected,
                "{label}"
            );
        }
    }
}
