use forge_storage::StorageError;

use crate::PlatformError;

pub fn reauth_required(platform: &str) -> PlatformError {
    PlatformError::ReauthRequired {
        platform: platform.to_owned(),
    }
}

pub fn credential_storage_error(platform: &str, error: StorageError) -> PlatformError {
    match error {
        StorageError::Decryption => reauth_required(platform),
        other => PlatformError::Io(std::io::Error::other(other)),
    }
}
