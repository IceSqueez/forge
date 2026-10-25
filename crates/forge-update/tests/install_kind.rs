#![allow(clippy::unwrap_used)]
use std::path::{Path, PathBuf};

use forge_update::{
    InstallEnvironment, InstallKind, InstallLayout, ManualReason, UpdatableInstall,
    classify_install,
};

fn env(executable: impl Into<PathBuf>) -> InstallEnvironment {
    InstallEnvironment {
        release_build: true,
        executable: executable.into(),
        appimage: None,
        appdir: None,
        program_files: Vec::new(),
        local_app_data: None,
    }
}

fn updatable(layout: InstallLayout, target: impl Into<PathBuf>) -> InstallKind {
    InstallKind::Updatable(UpdatableInstall {
        layout,
        target: target.into(),
    })
}

fn manual(reason: ManualReason) -> InstallKind {
    InstallKind::ManualOnly(reason)
}

fn unwritable_dir(root: &Path) -> PathBuf {
    let blocker = root.join("not-a-dir");
    std::fs::write(&blocker, b"").unwrap();
    blocker
}

#[test]
fn a_build_without_the_release_marker_is_never_updatable() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("forge.AppImage");
    let local = dir.path().join("Local");
    let mut environment = env(local.join("Programs").join("Forge").join("forge.exe"));
    environment.release_build = false;
    environment.appimage = Some(image);
    environment.appdir = Some(dir.path().to_path_buf());
    environment.local_app_data = Some(local);

    assert_eq!(
        classify_install(&environment),
        manual(ManualReason::NotReleaseBuild)
    );
}

#[test]
fn an_updatable_install_stages_next_to_its_target() {
    let target = PathBuf::from("/home/nova/Apps/forge.AppImage");
    let install = UpdatableInstall {
        layout: InstallLayout::LinuxAppImage,
        target,
    };

    assert_eq!(install.staging_dir(), Some(Path::new("/home/nova/Apps")));
}

#[cfg(target_os = "linux")]
#[test]
fn linux_install_kind_requires_the_executable_to_run_from_the_appimage_mount() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("forge.AppImage");
    let foreign_image = dir.path().join("terminal.AppImage");
    let blocked_image = unwritable_dir(dir.path()).join("forge.AppImage");
    let mount = dir.path().join(".mount_forgeXYZ");
    let mounted_exe = mount.join("usr").join("bin").join("forge");
    std::fs::create_dir_all(mounted_exe.parent().unwrap()).unwrap();
    std::fs::write(&mounted_exe, b"").unwrap();
    let mount_link = dir.path().join("mount-link");
    std::os::unix::fs::symlink(&mount, &mount_link).unwrap();
    let other_mount = dir.path().join(".mount_terminalABC");
    std::fs::create_dir_all(&other_mount).unwrap();
    let appimage_env = |executable: &Path, appimage: &Path, appdir: Option<&Path>| {
        let mut environment = env(executable);
        environment.appimage = Some(appimage.to_path_buf());
        environment.appdir = appdir.map(Path::to_path_buf);
        environment
    };

    for (label, environment, expected) in [
        (
            "exe inside the mount, image dir writable",
            appimage_env(&mounted_exe, &image, Some(&mount)),
            updatable(InstallLayout::LinuxAppImage, &image),
        ),
        (
            "mount reached through a symlinked appdir",
            appimage_env(&mounted_exe, &image, Some(&mount_link)),
            updatable(InstallLayout::LinuxAppImage, &image),
        ),
        (
            "exe reached through a symlinked mount",
            appimage_env(
                &mount_link.join("usr").join("bin").join("forge"),
                &image,
                Some(&mount),
            ),
            updatable(InstallLayout::LinuxAppImage, &image),
        ),
        (
            "exe inside the mount, image dir unwritable",
            appimage_env(&mounted_exe, &blocked_image, Some(&mount)),
            manual(ManualReason::ReadOnlyLocation),
        ),
        (
            "inherited appimage of another app, distro exe",
            appimage_env(
                Path::new("/usr/bin/forge"),
                &foreign_image,
                Some(&other_mount),
            ),
            manual(ManualReason::LinuxPackageManaged),
        ),
        (
            "appimage set without appdir",
            appimage_env(&mounted_exe, &image, None),
            manual(ManualReason::LinuxPackageManaged),
        ),
        (
            "exe outside appdir",
            appimage_env(&mounted_exe, &image, Some(&other_mount)),
            manual(ManualReason::LinuxPackageManaged),
        ),
        (
            "distro package",
            env("/usr/bin/forge"),
            manual(ManualReason::LinuxPackageManaged),
        ),
    ] {
        assert_eq!(classify_install(&environment), expected, "{label}");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn macos_install_kind_follows_the_enclosing_app_bundle() {
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("Forge.app");
    let executable = bundle.join("Contents").join("MacOS").join("forge");
    let blocked_bundle = unwritable_dir(dir.path()).join("Forge.app");
    let blocked_executable = blocked_bundle.join("Contents").join("MacOS").join("forge");

    for (label, environment, expected) in [
        (
            "bundle in a writable dir",
            env(&executable),
            updatable(InstallLayout::MacAppBundle, bundle),
        ),
        (
            "bundle in an unwritable dir",
            env(blocked_executable),
            manual(ManualReason::ReadOnlyLocation),
        ),
        (
            "translocated bundle",
            env("/private/var/folders/xy/T/AppTranslocation/0A1B/d/Forge.app/Contents/MacOS/forge"),
            manual(ManualReason::MacTranslocated),
        ),
        (
            "bare binary",
            env("/usr/local/bin/forge"),
            manual(ManualReason::UnknownExecutable),
        ),
    ] {
        assert_eq!(classify_install(&environment), expected, "{label}");
    }
}

#[cfg(target_os = "windows")]
#[test]
fn windows_install_kind_follows_the_install_root_of_the_executable() {
    let dir = tempfile::tempdir().unwrap();
    let program_files = dir.path().join("Program Files");
    let program_files_x86 = dir.path().join("Program Files (x86)");
    let local = dir.path().join("Local");
    let per_user_dir = local.join("Programs").join("Forge");
    let sibling_dir = dir.path().join("Program Files Extra");
    for created in [
        &program_files,
        &program_files_x86,
        &per_user_dir,
        &sibling_dir,
    ] {
        std::fs::create_dir_all(created).unwrap();
    }
    let per_user_exe = per_user_dir.join("forge.exe");
    let portable_exe = dir.path().join("forge.exe");
    let sibling_exe = sibling_dir.join("forge.exe");
    let blocked_exe = unwritable_dir(dir.path()).join("forge.exe");
    let shouted_x86 = PathBuf::from(program_files_x86.to_string_lossy().to_uppercase())
        .join("Forge")
        .join("forge.exe");
    let with_roots = |executable: PathBuf| {
        let mut environment = env(executable);
        environment.program_files = vec![program_files.clone(), program_files_x86.clone()];
        environment.local_app_data = Some(local.clone());
        environment
    };

    for (label, environment, expected) in [
        (
            "machine-wide install",
            with_roots(program_files.join("Forge").join("bin").join("forge.exe")),
            manual(ManualReason::WindowsMachineWideInstall),
        ),
        (
            "machine-wide install under another case",
            with_roots(shouted_x86),
            manual(ManualReason::WindowsMachineWideInstall),
        ),
        (
            "per-user msi",
            with_roots(per_user_exe.clone()),
            updatable(InstallLayout::WindowsPerUserMsi, per_user_exe),
        ),
        (
            "portable exe",
            with_roots(portable_exe.clone()),
            updatable(InstallLayout::WindowsPortable, portable_exe),
        ),
        (
            "sibling of program files is portable",
            with_roots(sibling_exe.clone()),
            updatable(InstallLayout::WindowsPortable, sibling_exe),
        ),
        (
            "portable exe in an unwritable dir",
            with_roots(blocked_exe),
            manual(ManualReason::ReadOnlyLocation),
        ),
    ] {
        assert_eq!(classify_install(&environment), expected, "{label}");
    }
}

#[cfg(target_os = "windows")]
#[test]
fn a_leftover_machine_install_is_reported_only_when_forge_runs_from_elsewhere() {
    use forge_update::leftover_machine_install;

    let dir = tempfile::tempdir().unwrap();
    let program_files = dir.path().join("Program Files");
    let install_dir = program_files.join("Forge");
    std::fs::create_dir_all(install_dir.join("bin")).unwrap();
    std::fs::write(install_dir.join("bin").join("forge.exe"), b"").unwrap();
    let empty_root = dir.path().join("Program Files (x86)");
    std::fs::create_dir_all(&empty_root).unwrap();
    let with_root = |root: &Path, executable: PathBuf| {
        let mut environment = env(executable);
        environment.program_files = vec![root.to_path_buf()];
        environment
    };

    for (label, environment, expected) in [
        (
            "running a per-user copy",
            with_root(&program_files, dir.path().join("Local").join("forge.exe")),
            Some(install_dir.clone()),
        ),
        (
            "running the machine-wide copy",
            with_root(&program_files, install_dir.join("bin").join("forge.exe")),
            None,
        ),
        (
            "no machine-wide copy",
            with_root(&empty_root, dir.path().join("forge.exe")),
            None,
        ),
    ] {
        assert_eq!(leftover_machine_install(&environment), expected, "{label}");
    }
}
