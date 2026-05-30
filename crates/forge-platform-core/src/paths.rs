use std::path::PathBuf;

use directories::BaseDirs;

#[cfg(target_os = "macos")]
const MACOS_BUNDLE_ID: &str = "com.icesqueez.forge";

#[cfg(not(target_os = "macos"))]
const APP_DIR_NAME: &str = "forge";

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

pub fn config_dir() -> PathBuf {
    base_dirs().config_dir().join(app_segment())
}

pub fn data_dir() -> PathBuf {
    base_dirs().data_dir().join(app_segment())
}

pub fn cache_dir() -> PathBuf {
    base_dirs().cache_dir().join(app_segment())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_ends_with_app_segment() {
        let path = data_dir();
        let last = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        assert_eq!(last, app_segment());
    }

    #[test]
    fn config_dir_ends_with_app_segment() {
        let path = config_dir();
        let last = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        assert_eq!(last, app_segment());
    }

    #[test]
    fn cache_dir_ends_with_app_segment() {
        let path = cache_dir();
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
