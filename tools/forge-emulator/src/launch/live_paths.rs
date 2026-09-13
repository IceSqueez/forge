use std::path::{Path, PathBuf};

use directories::BaseDirs;

use crate::EmulatorError;

#[cfg(target_os = "macos")]
const FORGE_APP_SEGMENT: &str = "com.icesqueez.forge";
#[cfg(not(target_os = "macos"))]
const FORGE_APP_SEGMENT: &str = "forge";

/// Where an unconfigured forge would read and write, resolved from this process's own environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LivePaths {
    pub data_dir: PathBuf,
    pub home: PathBuf,
}

impl LivePaths {
    pub fn discover() -> Result<Self, EmulatorError> {
        let base = BaseDirs::new().ok_or_else(|| EmulatorError::InvalidLaunch {
            reason: "the real home directory cannot be resolved, so the live data directory cannot be ruled out".to_owned(),
        })?;
        Ok(Self {
            data_dir: base.data_dir().join(FORGE_APP_SEGMENT),
            home: base.home_dir().to_owned(),
        })
    }

    /// A fixture data directory may not equal, contain, or sit inside the live one; a scratch
    /// home may not equal or contain the real home.
    pub fn refuse_overlap(&self, data_dir: &Path, home: &Path) -> Result<(), EmulatorError> {
        let live_data = resolved(&self.data_dir);
        let fixture_data = resolved(data_dir);
        if fixture_data.starts_with(&live_data) || live_data.starts_with(&fixture_data) {
            return Err(EmulatorError::LiveDataDir { path: fixture_data });
        }
        let scratch_home = resolved(home);
        if resolved(&self.home).starts_with(&scratch_home) {
            return Err(EmulatorError::LiveHome { path: scratch_home });
        }
        Ok(())
    }
}

fn resolved(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn live() -> LivePaths {
        LivePaths {
            data_dir: PathBuf::from("/nonexistent-user/.local/share/forge"),
            home: PathBuf::from("/nonexistent-user"),
        }
    }

    #[test]
    fn scratch_directories_apart_from_the_live_ones_are_accepted() {
        let refusal = live().refuse_overlap(
            Path::new("/nonexistent-run/attempt-1/data"),
            Path::new("/nonexistent-run/attempt-1/home"),
        );
        assert!(refusal.is_ok(), "got {refusal:?}");
    }

    #[test]
    fn scratch_home_inside_the_real_home_is_accepted() {
        let refusal = live().refuse_overlap(
            Path::new("/nonexistent-user/runs/data"),
            Path::new("/nonexistent-user/runs/home"),
        );
        assert!(refusal.is_ok(), "got {refusal:?}");
    }

    #[test]
    fn data_directory_overlapping_the_live_one_is_refused() {
        for data_dir in [
            "/nonexistent-user/.local/share/forge",
            "/nonexistent-user/.local/share/forge/nested",
            "/nonexistent-user/.local/share",
        ] {
            let refusal = live().refuse_overlap(Path::new(data_dir), Path::new("/scratch/home"));
            assert!(
                matches!(&refusal, Err(EmulatorError::LiveDataDir { path }) if path == Path::new(data_dir)),
                "{data_dir}: got {refusal:?}"
            );
        }
    }

    #[test]
    fn scratch_home_equal_to_or_above_the_real_home_is_refused() {
        for home in ["/nonexistent-user", "/"] {
            let refusal = live().refuse_overlap(Path::new("/scratch/data"), Path::new(home));
            assert!(
                matches!(refusal, Err(EmulatorError::LiveHome { .. })),
                "{home}: got {refusal:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_alias_of_the_live_data_directory_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("share/forge");
        std::fs::create_dir_all(&real).unwrap();
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        let live = LivePaths {
            data_dir: real,
            home: PathBuf::from("/nonexistent-user"),
        };
        let refusal = live.refuse_overlap(&alias, Path::new("/scratch/home"));
        assert!(
            matches!(refusal, Err(EmulatorError::LiveDataDir { .. })),
            "got {refusal:?}"
        );
    }
}
