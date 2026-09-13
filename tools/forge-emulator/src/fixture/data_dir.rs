use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::EmulatorError;

pub const DATA_DIR_VARIABLE: &str = "FORGE_DATA_DIR";
pub const KEY_FILE_VARIABLE: &str = "FORGE_CREDENTIAL_KEY_FILE";

/// Absolute and empty when constructed; forge treats it as its whole data directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeDataDir {
    path: PathBuf,
}

impl ForgeDataDir {
    pub fn fresh(path: &Path) -> Result<Self, EmulatorError> {
        let unusable = |reason: String| EmulatorError::DataDir {
            path: path.to_owned(),
            reason,
        };
        let path = path.canonicalize().map_err(|e| unusable(e.to_string()))?;
        let mut entries = std::fs::read_dir(&path).map_err(|e| unusable(e.to_string()))?;
        if entries.next().is_some() {
            return Err(EmulatorError::DataDirNotEmpty { path });
        }
        Ok(Self { path })
    }

    /// Refuses a key-file override: it would move the credential key out of the directory.
    pub fn from_forge_environment(
        data_dir: Option<OsString>,
        key_file: Option<OsString>,
    ) -> Result<Self, EmulatorError> {
        if key_file.is_some() {
            return Err(EmulatorError::KeyFileOverride {
                variable: KEY_FILE_VARIABLE,
            });
        }
        let path = data_dir
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or(EmulatorError::DataDirUnset {
                variable: DATA_DIR_VARIABLE,
            })?;
        if !path.is_absolute() {
            return Err(EmulatorError::DataDir {
                path,
                reason: "path is not absolute".to_owned(),
            });
        }
        Self::fresh(&path)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Launch both the seeder and forge through this so both resolve `<dir>/credentials-key`.
    pub fn configure(&self, command: &mut tokio::process::Command) {
        command
            .env(DATA_DIR_VARIABLE, &self.path)
            .env_remove(KEY_FILE_VARIABLE);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn fresh_accepts_an_empty_directory_as_its_canonical_path() {
        let dir = tempfile::tempdir().unwrap();
        let dotted = dir.path().join(".");
        let data_dir = ForgeDataDir::fresh(&dotted).unwrap();
        assert_eq!(data_dir.path(), dir.path().canonicalize().unwrap());
    }

    #[test]
    fn fresh_refuses_a_directory_holding_any_entry() {
        for entry in ["forge.db", ".hidden"] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join(entry), b"").unwrap();
            let refusal = ForgeDataDir::fresh(dir.path());
            assert!(
                matches!(refusal, Err(EmulatorError::DataDirNotEmpty { .. })),
                "entry {entry}: got {refusal:?}"
            );
        }
    }

    #[test]
    fn fresh_refuses_a_missing_path_and_a_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, b"").unwrap();
        for path in [dir.path().join("missing"), file] {
            let refusal = ForgeDataDir::fresh(&path);
            assert!(
                matches!(&refusal, Err(EmulatorError::DataDir { path: echoed, .. }) if *echoed == path),
                "{}: got {refusal:?}",
                path.display()
            );
        }
    }

    #[test]
    fn forge_environment_without_a_data_directory_is_refused() {
        for data_dir in [None, Some(OsString::new())] {
            let refusal = ForgeDataDir::from_forge_environment(data_dir.clone(), None);
            assert!(
                matches!(refusal, Err(EmulatorError::DataDirUnset { .. })),
                "{data_dir:?}: got {refusal:?}"
            );
        }
    }

    #[test]
    fn forge_environment_with_a_key_file_override_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let refusal = ForgeDataDir::from_forge_environment(
            Some(dir.path().as_os_str().to_owned()),
            Some(OsString::from("/elsewhere/credentials-key")),
        );
        assert!(matches!(
            refusal,
            Err(EmulatorError::KeyFileOverride { .. })
        ));
    }

    #[test]
    fn forge_environment_with_a_relative_directory_is_refused() {
        let refusal =
            ForgeDataDir::from_forge_environment(Some(OsString::from("relative/dir")), None);
        assert!(
            matches!(&refusal, Err(EmulatorError::DataDir { reason, .. }) if reason.contains("absolute")),
            "got {refusal:?}"
        );
    }

    #[test]
    fn configured_command_names_the_directory_and_drops_the_key_file_override() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = ForgeDataDir::fresh(dir.path()).unwrap();
        let mut command = tokio::process::Command::new("forge");
        command.env(KEY_FILE_VARIABLE, "/elsewhere/credentials-key");
        data_dir.configure(&mut command);

        let envs: Vec<(&OsStr, Option<&OsStr>)> = command.as_std().get_envs().collect();
        assert!(envs.contains(&(
            OsStr::new(DATA_DIR_VARIABLE),
            Some(data_dir.path().as_os_str())
        )));
        assert!(envs.contains(&(OsStr::new(KEY_FILE_VARIABLE), None)));
    }
}
