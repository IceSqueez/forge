use forge_host::boot_signal::BOOT_TARGET;
use forge_platform_core::PlatformError;

use crate::app::boot::BootFailure;

const SIGNAL_READINESS: &str = "readiness";
const SIGNAL_STARTUP_REFUSED: &str = "startup_refused";

const OUTCOME_FAILED: &str = "failed";
const OUTCOME_READY: &str = "ready";
const OUTCOME_ENDPOINT_OVERRIDE_REFUSED: &str = "endpoint_override_refused";
const OUTCOME_ALREADY_RUNNING: &str = "already_running";

const FAILURE_UPGRADE_REQUIRED: &str = "upgrade_required";
const FAILURE_PRE_BASELINE: &str = "pre_baseline";
const FAILURE_RETRY: &str = "retry";

const VARIABLE_UNKNOWN: &str = "unknown";

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
    use forge_host::test_support::{console_line, signal_fields};
    use forge_platform_core::EndpointRefusal;

    use super::*;

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
                    failure: crate::shared::repo_failure::RepoFailure::startup_stopped(
                        "database is locked",
                    ),
                },
                "retry",
            ),
        ] {
            let line = console_line(|| boot_failed(&failure));
            assert_eq!(
                signal_fields(&line),
                format!("signal=readiness outcome=failed failure={kind}"),
                "{line}"
            );
        }
    }

    #[test]
    fn override_refusal_line_carries_the_refused_variable_before_the_error() {
        let variable = forge_platform_twitch::endpoint_surfaces::TWITCH_API.env_var();
        let error = PlatformError::EndpointOverrideRefused {
            variable,
            reason: EndpointRefusal::NotLoopback,
        };
        let line = console_line(|| endpoint_override_refused(&error));
        assert_eq!(
            signal_fields(&line),
            format!(
                "signal=startup_refused outcome=endpoint_override_refused variable={variable} error={error}"
            )
        );
    }
}
