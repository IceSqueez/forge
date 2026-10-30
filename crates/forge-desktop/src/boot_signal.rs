use std::net::SocketAddr;

use forge_platform_core::PlatformError;
use forge_server::ServerError;

use crate::boot::BootFailure;

pub(crate) const BOOT_TARGET: &str = "forge::boot";

const SIGNAL_SERVER_BIND: &str = "server_bind";
const SIGNAL_READINESS: &str = "readiness";
const SIGNAL_STARTUP_REFUSED: &str = "startup_refused";

const OUTCOME_BOUND: &str = "bound";
const OUTCOME_FAILED: &str = "failed";
const OUTCOME_DISABLED: &str = "disabled";
const OUTCOME_INVALID_ADDRESS: &str = "invalid_address";
const OUTCOME_SETTINGS_UNAVAILABLE: &str = "settings_unavailable";
const OUTCOME_READY: &str = "ready";
const OUTCOME_ENDPOINT_OVERRIDE_REFUSED: &str = "endpoint_override_refused";
const OUTCOME_ALREADY_RUNNING: &str = "already_running";

const CAUSE_ADDRESS_UNAVAILABLE: &str = "address_unavailable";
const CAUSE_LAN_BIND_DISABLED: &str = "lan_bind_disabled";
const CAUSE_OTHER: &str = "other";

const FAILURE_UPGRADE_REQUIRED: &str = "upgrade_required";
const FAILURE_PRE_BASELINE: &str = "pre_baseline";
const FAILURE_RETRY: &str = "retry";

const VARIABLE_UNKNOWN: &str = "unknown";

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

pub(crate) fn ready() {
    tracing::info!(
        target: BOOT_TARGET,
        signal = %SIGNAL_READINESS,
        outcome = %OUTCOME_READY,
        "boot complete"
    );
}

pub(crate) fn boot_failed(failure: &BootFailure) {
    tracing::error!(
        target: BOOT_TARGET,
        signal = %SIGNAL_READINESS,
        outcome = %OUTCOME_FAILED,
        failure = %boot_failure_kind(failure),
        "boot failed"
    );
}

pub(crate) fn endpoint_override_refused(error: &PlatformError) {
    tracing::error!(
        target: BOOT_TARGET,
        signal = %SIGNAL_STARTUP_REFUSED,
        outcome = %OUTCOME_ENDPOINT_OVERRIDE_REFUSED,
        variable = %refused_variable(error),
        error = %error,
        "platform endpoint override refused; exiting"
    );
}

pub(crate) fn already_running() {
    tracing::error!(
        target: BOOT_TARGET,
        signal = %SIGNAL_STARTUP_REFUSED,
        outcome = %OUTCOME_ALREADY_RUNNING,
        "another forge instance is already running for this data directory; exiting"
    );
}

fn bind_failure_cause(error: &ServerError) -> &'static str {
    match error {
        ServerError::Bind { .. } => CAUSE_ADDRESS_UNAVAILABLE,
        ServerError::LanBindNotEnabled { .. } => CAUSE_LAN_BIND_DISABLED,
        _ => CAUSE_OTHER,
    }
}

fn boot_failure_kind(failure: &BootFailure) -> &'static str {
    match failure {
        BootFailure::UpgradeRequired { .. } => FAILURE_UPGRADE_REQUIRED,
        BootFailure::PreBaseline { .. } => FAILURE_PRE_BASELINE,
        BootFailure::Retry { .. } => FAILURE_RETRY,
    }
}

fn refused_variable(error: &PlatformError) -> &'static str {
    match error {
        PlatformError::EndpointOverrideRefused { variable, .. } => variable,
        _ => VARIABLE_UNKNOWN,
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::{Arc, Mutex, PoisonError};

    use forge_platform_core::{EndpointRefusal, EndpointSurface};
    use tracing_subscriber::fmt::MakeWriter;
    use tracing_subscriber::layer::SubscriberExt as _;

    use super::*;

    #[derive(Clone, Default)]
    struct Console(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Console {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Console {
        type Writer = Console;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    fn console_line(emit: impl FnOnce()) -> String {
        let console = Console::default();
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_ansi(false)
                .with_writer(console.clone()),
        );
        tracing::subscriber::with_default(subscriber, emit);
        let written = console
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        String::from_utf8_lossy(&written).trim_end().to_owned()
    }

    fn field_run(line: &str) -> &str {
        line.find(" signal=").map_or("", |start| &line[start + 1..])
    }

    #[test]
    fn bind_failure_line_names_the_cause_and_ends_with_the_error() {
        let addr: SocketAddr = "127.0.0.1:40000".parse().unwrap();
        for (error, cause) in [
            (
                ServerError::Bind {
                    addr: addr.to_string(),
                    reason: "Address already in use (os error 98)".to_owned(),
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
                field_run(&line),
                format!(
                    "signal=server_bind outcome=failed cause={cause} addr=127.0.0.1:40000 port=40000 error={error}"
                ),
                "{line}"
            );
        }
    }

    #[test]
    fn boot_failure_line_names_the_failure_kind() {
        for (failure, kind) in [
            (
                BootFailure::UpgradeRequired {
                    expected: 60,
                    found: 61,
                },
                "upgrade_required",
            ),
            (BootFailure::PreBaseline { found: 3 }, "pre_baseline"),
            (
                BootFailure::Retry {
                    reason: "database is locked".to_owned(),
                },
                "retry",
            ),
        ] {
            let line = console_line(|| boot_failed(&failure));
            assert_eq!(
                field_run(&line),
                format!("signal=readiness outcome=failed failure={kind}"),
                "{line}"
            );
        }
    }

    #[test]
    fn override_refusal_line_carries_the_refused_variable_before_the_error() {
        let variable = EndpointSurface::TwitchApi.env_var();
        let error = PlatformError::EndpointOverrideRefused {
            variable,
            reason: EndpointRefusal::NotLoopback,
        };
        let line = console_line(|| endpoint_override_refused(&error));
        assert_eq!(
            field_run(&line),
            format!(
                "signal=startup_refused outcome=endpoint_override_refused variable={variable} error={error}"
            )
        );
    }
}
