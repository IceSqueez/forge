use std::path::PathBuf;

use directories::BaseDirs;

pub const MACOS_BUNDLE_ID: &str = "com.icesqueez.forge";

#[cfg(not(target_os = "macos"))]
const APP_DIR_NAME: &str = "forge";

pub const MEDIA_DIR_NAME: &str = "media";

pub const ASSETS_DIR_NAME: &str = "assets";

pub const OVERLAYS_DIR_NAME: &str = "overlays";

pub const TTS_DIR_NAME: &str = "tts";

pub const BACKUPS_DIR_NAME: &str = "backups";

pub const CRASHES_DIR_NAME: &str = "crashes";

pub const PROBLEM_REPORTS_DIR_NAME: &str = "reports";

pub const LOGS_DIR_NAME: &str = "logs";

#[expect(
    clippy::expect_used,
    reason = "running without a home directory is unsupported"
)]
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
