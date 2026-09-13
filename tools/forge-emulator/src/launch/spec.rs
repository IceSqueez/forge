use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use forge_platform_core::EndpointSurface;
use tokio::process::Command;

use crate::EmulatorError;
use crate::fixture::ForgeDataDir;

const TWITCH_CLIENT_ID_VARIABLE: &str = "FORGE_TWITCH_CLIENT_ID";
pub const DEFAULT_LOG_DIRECTIVES: &str = "info,forge=debug";

/// Passed through from the emulator's environment when present; nothing else is inherited.
pub const INHERITED_VARIABLES: [&str; 20] = [
    "PATH",
    "WAYLAND_DISPLAY",
    "DISPLAY",
    "XAUTHORITY",
    "XDG_RUNTIME_DIR",
    "XDG_SESSION_TYPE",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_DESKTOP",
    "XDG_DATA_DIRS",
    "HYPRLAND_INSTANCE_SIGNATURE",
    "DBUS_SESSION_BUS_ADDRESS",
    "XCURSOR_THEME",
    "XCURSOR_SIZE",
    "GDK_SCALE",
    "__GLX_VENDOR_LIBRARY_NAME",
    "__EGL_VENDOR_LIBRARY_FILENAMES",
    "VK_ICD_FILENAMES",
    "VK_DRIVER_FILES",
    "MESA_LOADER_DRIVER_OVERRIDE",
    "LIBGL_ALWAYS_SOFTWARE",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeCommand {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

impl ForgeCommand {
    pub fn binary(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub forge: ForgeCommand,
    pub data_dir: ForgeDataDir,
    /// Becomes forge's HOME and XDG base, so no home-relative lookup reaches the user's files.
    pub home: PathBuf,
    pub twitch_client_id: Option<String>,
    /// Names must be forge endpoint-override variables, as `FakeTwitch::endpoint_overrides` yields.
    pub endpoint_overrides: Vec<(&'static str, String)>,
    /// `RUST_LOG` filter syntax.
    pub log_directives: String,
}

impl LaunchSpec {
    pub(crate) fn validate(&self) -> Result<(), EmulatorError> {
        let invalid = |reason: String| EmulatorError::InvalidLaunch { reason };
        if !self.home.is_absolute() {
            return Err(invalid(format!(
                "scratch home {} is not absolute",
                self.home.display()
            )));
        }
        for (variable, _) in &self.endpoint_overrides {
            if !EndpointSurface::ALL
                .iter()
                .any(|surface| surface.env_var() == *variable)
            {
                return Err(invalid(format!(
                    "`{variable}` is not a forge endpoint-override variable"
                )));
            }
        }
        Ok(())
    }

    /// Clears the command's environment and rebuilds it from `inherited` through the allowlist.
    pub(crate) fn configure_environment<K, V>(
        &self,
        command: &mut Command,
        inherited: impl IntoIterator<Item = (K, V)>,
    ) where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        command.env_clear();
        for (key, value) in inherited {
            if INHERITED_VARIABLES
                .iter()
                .any(|allowed| OsStr::new(allowed) == key.as_ref())
            {
                command.env(key, value);
            }
        }
        command
            .env("HOME", &self.home)
            .env("XDG_DATA_HOME", self.home.join(".local/share"))
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("XDG_CACHE_HOME", self.home.join(".cache"))
            .env("XDG_STATE_HOME", self.home.join(".local/state"))
            .env("LANG", "C.UTF-8")
            .env("NO_COLOR", "1")
            .env("RUST_BACKTRACE", "1")
            .env("RUST_LOG", &self.log_directives);
        if let Some(client_id) = &self.twitch_client_id {
            command.env(TWITCH_CLIENT_ID_VARIABLE, client_id);
        }
        for (variable, url) in &self.endpoint_overrides {
            command.env(variable, url);
        }
        self.data_dir.configure(command);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn spec(data: &std::path::Path, home: PathBuf) -> LaunchSpec {
        LaunchSpec {
            forge: ForgeCommand::binary("/nonexistent/forge"),
            data_dir: ForgeDataDir::fresh(data).unwrap(),
            home,
            twitch_client_id: Some("fixtureclient".to_owned()),
            endpoint_overrides: vec![(
                EndpointSurface::TwitchApi.env_var(),
                "http://127.0.0.1:1".to_owned(),
            )],
            log_directives: DEFAULT_LOG_DIRECTIVES.to_owned(),
        }
    }

    fn configured(spec: &LaunchSpec, inherited: &[(&str, &str)]) -> BTreeMap<String, String> {
        let mut command = Command::new("forge");
        spec.configure_environment(&mut command, inherited.iter().copied());
        command
            .as_std()
            .get_envs()
            .filter_map(|(key, value)| {
                Some((
                    key.to_str()?.to_owned(),
                    value?.to_str().unwrap().to_owned(),
                ))
            })
            .collect()
    }

    #[test]
    fn only_allowlisted_session_variables_are_inherited() {
        let data = tempfile::tempdir().unwrap();
        let spec = spec(data.path(), PathBuf::from("/scratch/home"));
        let env = configured(
            &spec,
            &[
                ("WAYLAND_DISPLAY", "wayland-1"),
                ("HYPRLAND_INSTANCE_SIGNATURE", "sig"),
                ("FORGE_KICK_CLIENT_SECRET", "real-secret"),
                ("FORGE_YOUTUBE_API_BASE_URL", "http://127.0.0.1:9"),
                ("CARGO_HOME", "/home/user/.cargo"),
                ("SSH_AUTH_SOCK", "/run/user/1000/ssh"),
            ],
        );
        assert_eq!(
            env.get("WAYLAND_DISPLAY").map(String::as_str),
            Some("wayland-1")
        );
        assert_eq!(
            env.get("HYPRLAND_INSTANCE_SIGNATURE").map(String::as_str),
            Some("sig")
        );
        for leaked in [
            "FORGE_KICK_CLIENT_SECRET",
            "FORGE_YOUTUBE_API_BASE_URL",
            "CARGO_HOME",
            "SSH_AUTH_SOCK",
        ] {
            assert!(!env.contains_key(leaked), "{leaked} leaked into {env:?}");
        }
    }

    #[test]
    fn home_and_xdg_bases_point_into_the_scratch_home_over_inherited_values() {
        let data = tempfile::tempdir().unwrap();
        let spec = spec(data.path(), PathBuf::from("/scratch/home"));
        let env = configured(
            &spec,
            &[
                ("HOME", "/home/user"),
                ("XDG_DATA_HOME", "/home/user/.local/share"),
                ("XDG_CONFIG_HOME", "/home/user/.config"),
            ],
        );
        for (variable, expected) in [
            ("HOME", "/scratch/home"),
            ("XDG_DATA_HOME", "/scratch/home/.local/share"),
            ("XDG_CONFIG_HOME", "/scratch/home/.config"),
            ("XDG_CACHE_HOME", "/scratch/home/.cache"),
            ("XDG_STATE_HOME", "/scratch/home/.local/state"),
        ] {
            assert_eq!(
                env.get(variable).map(String::as_str),
                Some(expected),
                "{variable}"
            );
        }
    }

    #[test]
    fn fixture_directory_client_id_overrides_and_log_directives_are_set() {
        let data = tempfile::tempdir().unwrap();
        let spec = spec(data.path(), PathBuf::from("/scratch/home"));
        let env = configured(&spec, &[]);
        let data_dir = spec.data_dir.path().to_str().unwrap();
        for (variable, expected) in [
            ("FORGE_DATA_DIR", data_dir),
            ("FORGE_TWITCH_CLIENT_ID", "fixtureclient"),
            ("FORGE_TWITCH_API_BASE_URL", "http://127.0.0.1:1"),
            ("RUST_LOG", DEFAULT_LOG_DIRECTIVES),
            ("NO_COLOR", "1"),
        ] {
            assert_eq!(
                env.get(variable).map(String::as_str),
                Some(expected),
                "{variable}"
            );
        }
    }

    #[test]
    fn launch_without_a_twitch_account_sets_no_client_id() {
        let data = tempfile::tempdir().unwrap();
        let mut spec = spec(data.path(), PathBuf::from("/scratch/home"));
        spec.twitch_client_id = None;
        let env = configured(&spec, &[("FORGE_TWITCH_CLIENT_ID", "inherited")]);
        assert!(!env.contains_key("FORGE_TWITCH_CLIENT_ID"), "{env:?}");
    }

    #[test]
    fn override_names_outside_the_forge_surfaces_are_refused() {
        let data = tempfile::tempdir().unwrap();
        let mut spec = spec(data.path(), PathBuf::from("/scratch/home"));
        spec.endpoint_overrides = vec![("LD_PRELOAD", "/tmp/evil.so".to_owned())];
        assert!(matches!(
            spec.validate(),
            Err(EmulatorError::InvalidLaunch { reason }) if reason.contains("LD_PRELOAD")
        ));
    }

    #[test]
    fn relative_scratch_home_is_refused() {
        let data = tempfile::tempdir().unwrap();
        let spec = spec(data.path(), PathBuf::from("scratch/home"));
        assert!(matches!(
            spec.validate(),
            Err(EmulatorError::InvalidLaunch { .. })
        ));
    }
}
