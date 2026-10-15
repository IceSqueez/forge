use std::path::{Path, PathBuf};

use crate::platform::ArtifactKind;

mod probe;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

const RELEASE_BUILD_MARKER: Option<&str> = option_env!("FORGE_RELEASE_BUILD");

const APPIMAGE_ENV: &str = "APPIMAGE";
const APPDIR_ENV: &str = "APPDIR";
const PROGRAM_FILES_ENVS: [&str; 2] = ["ProgramFiles", "ProgramW6432"];
const LOCAL_APP_DATA_ENV: &str = "LOCALAPPDATA";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallKind {
    Updatable(UpdatableInstall),
    ManualOnly(ManualReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatableInstall {
    pub layout: InstallLayout,
    pub target: PathBuf,
}

impl UpdatableInstall {
    pub fn artifact_kind(&self) -> ArtifactKind {
        self.layout.artifact_kind()
    }

    pub fn staging_dir(&self) -> Option<&Path> {
        self.target.parent()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstallLayout {
    MacAppBundle,
    WindowsPerUserMsi,
    WindowsPortable,
    LinuxAppImage,
}

impl InstallLayout {
    pub const fn artifact_kind(self) -> ArtifactKind {
        match self {
            Self::MacAppBundle => ArtifactKind::AppBundleArchive,
            Self::WindowsPerUserMsi => ArtifactKind::Msi,
            Self::WindowsPortable => ArtifactKind::PortableExe,
            Self::LinuxAppImage => ArtifactKind::AppImage,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ManualReason {
    NotReleaseBuild,
    UnsupportedPlatform,
    UnknownExecutable,
    MacTranslocated,
    ReadOnlyLocation,
    WindowsMachineWideInstall,
    LinuxPackageManaged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallEnvironment {
    pub release_build: bool,
    pub executable: PathBuf,
    pub appimage: Option<PathBuf>,
    pub appdir: Option<PathBuf>,
    pub program_files: Vec<PathBuf>,
    pub local_app_data: Option<PathBuf>,
}

impl InstallEnvironment {
    pub fn current() -> Option<Self> {
        let executable = std::env::current_exe().ok()?;
        let mut program_files: Vec<PathBuf> = Vec::new();
        for dir in PROGRAM_FILES_ENVS.iter().filter_map(|name| env_path(name)) {
            if !program_files.contains(&dir) {
                program_files.push(dir);
            }
        }
        Some(Self {
            release_build: is_release_build(),
            executable,
            appimage: env_path(APPIMAGE_ENV),
            appdir: env_path(APPDIR_ENV),
            program_files,
            local_app_data: env_path(LOCAL_APP_DATA_ENV),
        })
    }
}

pub const fn is_release_build() -> bool {
    RELEASE_BUILD_MARKER.is_some()
}

pub fn detect_install() -> InstallKind {
    match InstallEnvironment::current() {
        Some(env) => classify_install(&env),
        None if !is_release_build() => InstallKind::ManualOnly(ManualReason::NotReleaseBuild),
        None => InstallKind::ManualOnly(ManualReason::UnknownExecutable),
    }
}

pub fn classify_install(env: &InstallEnvironment) -> InstallKind {
    if !env.release_build {
        return InstallKind::ManualOnly(ManualReason::NotReleaseBuild);
    }
    classify_for_os(env)
}

#[cfg(target_os = "linux")]
fn classify_for_os(env: &InstallEnvironment) -> InstallKind {
    linux::classify(env)
}

#[cfg(target_os = "macos")]
fn classify_for_os(env: &InstallEnvironment) -> InstallKind {
    macos::classify(env)
}

#[cfg(target_os = "windows")]
fn classify_for_os(env: &InstallEnvironment) -> InstallKind {
    windows::classify(env)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn classify_for_os(_env: &InstallEnvironment) -> InstallKind {
    InstallKind::ManualOnly(ManualReason::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
pub fn leftover_machine_install(env: &InstallEnvironment) -> Option<PathBuf> {
    windows::leftover_machine_install(env)
}

#[cfg(not(target_os = "windows"))]
pub fn leftover_machine_install(_env: &InstallEnvironment) -> Option<PathBuf> {
    None
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn updatable(layout: InstallLayout, target: PathBuf) -> InstallKind {
    let writable = target.parent().is_some_and(probe::dir_is_writable);
    if writable {
        InstallKind::Updatable(UpdatableInstall { layout, target })
    } else {
        InstallKind::ManualOnly(ManualReason::ReadOnlyLocation)
    }
}
