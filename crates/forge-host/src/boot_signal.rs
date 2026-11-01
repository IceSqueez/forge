use std::net::SocketAddr;

use forge_server::ServerError;

pub const BOOT_TARGET: &str = "forge::boot";

const SIGNAL_SERVER_BIND: &str = "server_bind";

const OUTCOME_BOUND: &str = "bound";
const OUTCOME_FAILED: &str = "failed";
const OUTCOME_DISABLED: &str = "disabled";
const OUTCOME_INVALID_ADDRESS: &str = "invalid_address";
const OUTCOME_SETTINGS_UNAVAILABLE: &str = "settings_unavailable";

const CAUSE_ADDRESS_UNAVAILABLE: &str = "address_unavailable";
const CAUSE_LAN_BIND_DISABLED: &str = "lan_bind_disabled";
const CAUSE_OTHER: &str = "other";

pub(crate) fn server_bound(addr: SocketAddr) {
    tracing::info!(
        target: BOOT_TARGET,
        signal = %SIGNAL_SERVER_BIND,
        outcome = %OUTCOME_BOUND,
        addr = %addr,
        port = addr.port(),
        "server listening"
    );
}

pub(crate) fn server_bind_failed(addr: SocketAddr, error: &ServerError) {
    tracing::error!(
        target: BOOT_TARGET,
        signal = %SIGNAL_SERVER_BIND,
        outcome = %OUTCOME_FAILED,
        cause = %bind_failure_cause(error),
        addr = %addr,
        port = addr.port(),
        error = %error,
        "server failed to start, leaving it off"
    );
}

pub(crate) fn server_disabled() {
    tracing::info!(
        target: BOOT_TARGET,
        signal = %SIGNAL_SERVER_BIND,
        outcome = %OUTCOME_DISABLED,
        "server disabled in settings, leaving it off"
    );
}

pub(crate) fn server_address_invalid(bind_address: &str) {
    tracing::warn!(
        target: BOOT_TARGET,
        signal = %SIGNAL_SERVER_BIND,
        outcome = %OUTCOME_INVALID_ADDRESS,
        bind_address = %bind_address,
        "server bind address is invalid, leaving it off"
    );
}

pub(crate) fn server_settings_unavailable(error: &impl std::fmt::Display) {
    tracing::error!(
        target: BOOT_TARGET,
        signal = %SIGNAL_SERVER_BIND,
        outcome = %OUTCOME_SETTINGS_UNAVAILABLE,
        error = %error,
        "server settings load failed, leaving server off"
    );
}

fn bind_failure_cause(error: &ServerError) -> &'static str {
    match error {
        ServerError::Bind { .. } | ServerError::InvalidBindAddress { .. } => {
            CAUSE_ADDRESS_UNAVAILABLE
        }
        ServerError::LanBindNotEnabled { .. } => CAUSE_LAN_BIND_DISABLED,
        _ => CAUSE_OTHER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{console_line, signal_fields};

    #[test]
    fn bind_failure_line_names_the_cause_and_ends_with_the_error() {
        let addr: SocketAddr = "127.0.0.1:40000".parse().unwrap();
        for (error, cause) in [
            (
                ServerError::Bind {
                    addr: addr.to_string(),
                    source: std::io::ErrorKind::AddrInUse.into(),
                },
                "address_unavailable",
            ),
            (
                ServerError::LanBindNotEnabled {
                    addr: "0.0.0.0:40000".to_owned(),
                },
                "lan_bind_disabled",
            ),
            (
                ServerError::Storage(forge_storage::StorageError::Connection {
                    reason: "database is locked".to_owned(),
                }),
                "other",
            ),
        ] {
            let line = console_line(|| server_bind_failed(addr, &error));
            assert_eq!(
                signal_fields(&line),
                format!(
                    "signal=server_bind outcome=failed cause={cause} addr=127.0.0.1:40000 port=40000 error={error}"
                ),
                "{line}"
            );
        }
    }
}
