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

    #[derive(Debug, thiserror::Error)]
    #[error("database is locked")]
    struct LockedStore;

    #[test]
    fn an_unrecoverable_stored_secret_asks_the_named_platform_for_reauth() {
        let mapped = credential_storage_error("kick", true, LockedStore);

        assert!(
            matches!(&mapped, PlatformError::ReauthRequired { platform } if platform == "kick"),
            "got {mapped:?}"
        );
    }

    #[test]
    fn a_recoverable_storage_failure_becomes_io_carrying_the_source_message() {
        let mapped = credential_storage_error("twitch", false, LockedStore);

        assert!(
            matches!(&mapped, PlatformError::Io(io) if io.to_string() == LockedStore.to_string()),
            "got {mapped:?}"
        );
    }
}
