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

pub const DATA_DIR_ENV: &str = "FORGE_DATA_DIR";

#[cfg(target_os = "macos")]
pub fn app_segment() -> &'static str {
    MACOS_BUNDLE_ID
}

#[cfg(not(target_os = "macos"))]
pub fn app_segment() -> &'static str {
    APP_DIR_NAME
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    data_dir: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Self {
        Self::at(resolve_data_dir())
    }

    pub fn at(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    pub fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    pub fn overlays_dir(&self) -> PathBuf {
        self.data_dir.join(OVERLAYS_DIR_NAME)
    }

    pub fn media_dir(&self) -> PathBuf {
        self.data_dir.join(MEDIA_DIR_NAME)
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.data_dir.join(ASSETS_DIR_NAME)
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join(LOGS_DIR_NAME)
    }

    pub fn crashes_dir(&self) -> PathBuf {
        self.data_dir.join(CRASHES_DIR_NAME)
    }

    pub fn problem_reports_dir(&self) -> PathBuf {
        self.data_dir.join(PROBLEM_REPORTS_DIR_NAME)
    }
}

fn resolve_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV).filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    base_dirs().data_dir().join(app_segment())
}

pub fn data_dir() -> PathBuf {
    resolve_data_dir()
}

pub fn overlays_dir() -> PathBuf {
    AppPaths::resolve().overlays_dir()
}

pub fn media_dir() -> PathBuf {
    AppPaths::resolve().media_dir()
}

pub fn assets_dir() -> PathBuf {
    AppPaths::resolve().assets_dir()
}

pub const CREDENTIALS_KEY_FILE_NAME: &str = "credentials-key";

pub const CREDENTIALS_KEY_FILE_ENV: &str = "FORGE_CREDENTIAL_KEY_FILE";

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
mod tests {
    use std::path::Path;

    use super::*;

    static CREDENTIALS_KEY_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[expect(unsafe_code, reason = "env mutation is unsafe in edition 2024")]
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
    fn app_paths_at_a_root_keeps_each_folder_under_its_on_disk_name() {
        let root = PathBuf::from("/srv/forge data");
        let paths = AppPaths::at(root.clone());

        for (folder, name) in [
            (paths.overlays_dir(), "overlays"),
            (paths.media_dir(), "media"),
            (paths.assets_dir(), "assets"),
            (paths.logs_dir(), "logs"),
            (paths.crashes_dir(), "crashes"),
            (paths.problem_reports_dir(), "reports"),
        ] {
            assert_eq!(folder, root.join(name));
        }
    }

    #[test]
    fn app_paths_at_a_root_stays_inside_that_root_for_a_relative_one() {
        let paths = AppPaths::at(PathBuf::from("portable"));

        assert_eq!(paths.data_dir(), Path::new("portable"));
        assert_eq!(paths.logs_dir(), Path::new("portable").join("logs"));
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
