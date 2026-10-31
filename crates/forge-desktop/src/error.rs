use forge_components::tr;
use forge_platform_core::PlatformError;
use forge_storage::StorageError;
use thiserror::Error;

use crate::repo_failure::RepoFailure;

#[derive(Debug, Error)]
pub(crate) enum DesktopError {
    #[error("storage failed: {0}")]
    Storage(#[from] StorageError),
    #[error("file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("data could not be encoded: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("the background task stopped")]
    TaskStopped,
    #[error("{0}")]
    Platform(#[from] PlatformError),
    #[error("the sign-in flow was already used")]
    SignInFlowConsumed,
    #[error("the sign-in was cancelled")]
    SignInCancelled,
}

impl DesktopError {
    pub(crate) fn user_message(&self) -> String {
        match self {
            Self::Storage(error) => RepoFailure::from_storage_ref(error).text(),
            Self::Io(_) => tr!("desktop_error_file"),
            Self::Encoding(_) => tr!("desktop_error_encoding"),
            Self::TaskStopped => tr!("desktop_error_task_stopped"),
            Self::Platform(error) => platform_message(error),
            Self::SignInFlowConsumed => tr!("desktop_error_sign_in_consumed"),
            Self::SignInCancelled => tr!("desktop_error_sign_in_cancelled"),
        }
    }
}

fn platform_message(error: &PlatformError) -> String {
    match error {
        PlatformError::Network { .. } => tr!("desktop_error_network"),
        PlatformError::Auth { .. }
        | PlatformError::ReauthRequired { .. }
        | PlatformError::DeviceAuthEnded(_) => tr!("desktop_error_auth_refused"),
        PlatformError::LoopbackPortInUse { port } => {
            tr!("desktop_error_loopback_port", port = i64::from(*port))
        }
        PlatformError::RateLimited { .. }
        | PlatformError::RateLimitExhausted
        | PlatformError::QuotaExhausted => tr!("desktop_error_rate_limited"),
        _ => tr!("desktop_error_platform"),
    }
}

#[cfg(test)]
mod tests {
    use forge_platform_core::{DeviceAuthEnd, PlatformError};
    use forge_storage::{Language, StorageError};

    use super::*;
    use crate::i18n::{install_language, message_in};

    const SECRET: &str = "access_token=s3cr3t";

    fn network() -> PlatformError {
        PlatformError::Network {
            reason: SECRET.to_owned(),
        }
    }

    fn encoding() -> serde_json::Error {
        serde_json::from_str::<u8>("x").unwrap_err()
    }

    #[test]
    fn each_error_reads_as_its_own_message_and_never_echoes_the_cause() {
        install_language(Language::En);
        let reauth = PlatformError::ReauthRequired {
            platform: "twitch".to_owned(),
        };
        for (error, key) in [
            (
                DesktopError::Io(std::io::Error::other(SECRET)),
                "desktop_error_file",
            ),
            (DesktopError::Encoding(encoding()), "desktop_error_encoding"),
            (DesktopError::TaskStopped, "desktop_error_task_stopped"),
            (
                DesktopError::SignInFlowConsumed,
                "desktop_error_sign_in_consumed",
            ),
            (
                DesktopError::SignInCancelled,
                "desktop_error_sign_in_cancelled",
            ),
            (DesktopError::Platform(network()), "desktop_error_network"),
            (
                DesktopError::Platform(PlatformError::Auth {
                    reason: SECRET.to_owned(),
                }),
                "desktop_error_auth_refused",
            ),
            (DesktopError::Platform(reauth), "desktop_error_auth_refused"),
            (
                DesktopError::Platform(PlatformError::DeviceAuthEnded(DeviceAuthEnd::Denied)),
                "desktop_error_auth_refused",
            ),
            (
                DesktopError::Platform(PlatformError::RateLimitExhausted),
                "desktop_error_rate_limited",
            ),
            (
                DesktopError::Platform(PlatformError::QuotaExhausted),
                "desktop_error_rate_limited",
            ),
            (
                DesktopError::Platform(PlatformError::RateLimited {
                    retry_after_secs: 3,
                }),
                "desktop_error_rate_limited",
            ),
            (
                DesktopError::Platform(PlatformError::MalformedResponse {
                    reason: SECRET.to_owned(),
                }),
                "desktop_error_platform",
            ),
        ] {
            let shown = error.user_message();

            assert_eq!(shown, message_in(Language::En, key), "{error:?}");
            assert!(!shown.contains(SECRET), "{error:?}");
        }
    }

    #[test]
    fn a_busy_loopback_port_names_the_port() {
        install_language(Language::En);

        let shown =
            DesktopError::Platform(PlatformError::LoopbackPortInUse { port: 53682 }).user_message();

        assert!(
            shown.contains("53682") || shown.contains("53,682"),
            "{shown}"
        );
    }

    #[test]
    fn a_storage_failure_reads_as_the_repo_failure_of_that_error() {
        install_language(Language::En);
        let storage = || StorageError::NotFound {
            key: SECRET.to_owned(),
        };

        assert_eq!(
            DesktopError::Storage(storage()).user_message(),
            crate::repo_failure::RepoFailure::from_storage_ref(&storage()).text()
        );
    }

    #[test]
    fn a_platform_failure_keeps_its_upstream_reason_only_in_the_diagnostic_text() {
        let error = DesktopError::Platform(network());

        assert!(error.to_string().contains(SECRET));
    }
}
