use crate::PlatformError;

pub fn reauth_required(platform: &str) -> PlatformError {
    PlatformError::ReauthRequired {
        platform: platform.to_owned(),
    }
}

pub fn credential_storage_error<E>(
    platform: &str,
    stored_secret_unrecoverable: bool,
    source: E,
) -> PlatformError
where
    E: std::error::Error + Send + Sync + 'static,
{
    if stored_secret_unrecoverable {
        reauth_required(platform)
    } else {
        PlatformError::Io(std::io::Error::other(source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_undecryptable_credential_asks_the_named_platform_for_reauth() {
        let mapped = credential_storage_error("kick", StorageError::Decryption);

        assert!(
            matches!(&mapped, PlatformError::ReauthRequired { platform } if platform == "kick"),
            "got {mapped:?}"
        );
    }

    #[test]
    fn every_other_storage_failure_becomes_io_carrying_the_storage_message() {
        let failures = [
            StorageError::Connection {
                reason: "database is locked".to_owned(),
            },
            StorageError::Encryption,
            StorageError::NotFound {
                key: "twitch:broadcaster".to_owned(),
            },
        ];
        for failure in failures {
            let expected = failure.to_string();

            let mapped = credential_storage_error("twitch", failure);

            assert!(
                matches!(&mapped, PlatformError::Io(io) if io.to_string() == expected),
                "expected Io({expected:?}), got {mapped:?}"
            );
        }
    }
}
