use std::path::PathBuf;

use directories::BaseDirs;

#[cfg(target_os = "macos")]
const MACOS_BUNDLE_ID: &str = "com.icesqueez.forge";

#[cfg(not(target_os = "macos"))]
const APP_DIR_NAME: &str = "forge";

pub const MEDIA_DIR_NAME: &str = "media";

pub const ASSETS_DIR_NAME: &str = "assets";

pub const OVERLAYS_DIR_NAME: &str = "overlays";

pub const TTS_DIR_NAME: &str = "tts";

pub const BACKUPS_DIR_NAME: &str = "backups";

pub const CRASHES_DIR_NAME: &str = "crashes";

pub const PROBLEM_REPORTS_DIR_NAME: &str = "reports";

#[allow(clippy::expect_used)]
fn base_dirs() -> BaseDirs {
    BaseDirs::new().expect("home directory must be discoverable on supported platforms")
}

#[cfg(target_os = "macos")]
fn app_segment() -> &'static str {
    MACOS_BUNDLE_ID
}

#[cfg(not(target_os = "macos"))]
fn app_segment() -> &'static str {
    APP_DIR_NAME
}

pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("FORGE_DATA_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    base_dirs().data_dir().join(app_segment())
}

pub fn overlays_dir() -> PathBuf {
    data_dir().join(OVERLAYS_DIR_NAME)
}

pub fn media_dir() -> PathBuf {
    data_dir().join(MEDIA_DIR_NAME)
}

pub fn assets_dir() -> PathBuf {
    data_dir().join(ASSETS_DIR_NAME)
}

pub fn crashes_dir() -> PathBuf {
    data_dir().join(CRASHES_DIR_NAME)
}

pub fn problem_reports_dir() -> PathBuf {
    data_dir().join(PROBLEM_REPORTS_DIR_NAME)
}

pub const CREDENTIALS_KEY_FILE_NAME: &str = "credentials-key";

const CREDENTIALS_KEY_FILE_ENV: &str = "FORGE_CREDENTIAL_KEY_FILE";

pub fn credentials_key_path_in(data_dir: &std::path::Path) -> PathBuf {
    match std::env::var_os(CREDENTIALS_KEY_FILE_ENV) {
        Some(overridden) => PathBuf::from(overridden),
        None => data_dir.join(CREDENTIALS_KEY_FILE_NAME),
    }
}

pub fn credentials_key_path() -> PathBuf {
    credentials_key_path_in(&data_dir())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::Path;

    use super::*;

    static CREDENTIALS_KEY_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[allow(unsafe_code)]
    fn with_credentials_key_env<R>(value: Option<&str>, body: impl FnOnce() -> R) -> R {
        let _guard = CREDENTIALS_KEY_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = std::env::var_os(CREDENTIALS_KEY_FILE_ENV);
        // SAFETY: CREDENTIALS_KEY_ENV_LOCK serialises every test in this binary that touches this variable, and no other code in it reads it.
        unsafe {
            match value {
                Some(v) => std::env::set_var(CREDENTIALS_KEY_FILE_ENV, v),
                None => std::env::remove_var(CREDENTIALS_KEY_FILE_ENV),
            }
        }
        let result = body();
        // SAFETY: still under CREDENTIALS_KEY_ENV_LOCK, restoring the value seen on entry.
        unsafe {
            match previous {
                Some(v) => std::env::set_var(CREDENTIALS_KEY_FILE_ENV, v),
                None => std::env::remove_var(CREDENTIALS_KEY_FILE_ENV),
            }
        }
        result
    }

    #[test]
    fn credentials_key_path_in_uses_the_given_data_dir_without_an_override() {
        let data_dir = Path::new("/srv/forge-data");

        let path = with_credentials_key_env(None, || credentials_key_path_in(data_dir));

        assert_eq!(path, data_dir.join(CREDENTIALS_KEY_FILE_NAME));
    }

    #[test]
    fn credentials_key_path_in_prefers_the_overridden_file_over_the_data_dir() {
        let overridden = "/elsewhere/keys/forge.key";

        let path = with_credentials_key_env(Some(overridden), || {
            credentials_key_path_in(Path::new("/srv/forge-data"))
        });

        assert_eq!(path, PathBuf::from(overridden));
    }

    #[test]
    fn data_dir_ends_with_app_segment() {
        let path = data_dir();
        let last = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        assert_eq!(last, app_segment());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_data_dir_appends_forge_only() {
        let suffix = data_dir()
            .strip_prefix(base_dirs().data_dir())
            .unwrap()
            .to_path_buf();
        assert_eq!(suffix, PathBuf::from("forge"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_data_dir_appends_forge_only() {
        let suffix = data_dir()
            .strip_prefix(base_dirs().data_dir())
            .unwrap()
            .to_path_buf();
        assert_eq!(suffix, PathBuf::from("forge"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_data_dir_uses_bundle_id() {
        let suffix = data_dir()
            .strip_prefix(base_dirs().data_dir())
            .unwrap()
            .to_path_buf();
        assert_eq!(suffix, PathBuf::from(MACOS_BUNDLE_ID));
    }
}
