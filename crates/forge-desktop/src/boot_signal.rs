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
const CAUSE_LAN_BIND_UNAUTHENTICATED: &str = "lan_bind_unauthenticated";
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
        ServerError::NoTokenForLanBind { .. } => CAUSE_LAN_BIND_UNAUTHENTICATED,
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
