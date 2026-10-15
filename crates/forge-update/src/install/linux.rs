use std::path::{Path, PathBuf};

use super::{InstallEnvironment, InstallKind, InstallLayout, ManualReason, updatable};

pub(super) fn classify(env: &InstallEnvironment) -> InstallKind {
    match (&env.appimage, &env.appdir) {
        (Some(image), Some(mount)) if runs_from_mount(&env.executable, mount) => {
            updatable(InstallLayout::LinuxAppImage, image.clone())
        }
        _ => InstallKind::ManualOnly(ManualReason::LinuxPackageManaged),
    }
}

fn runs_from_mount(executable: &Path, mount: &Path) -> bool {
    resolved(executable).starts_with(resolved(mount))
}

fn resolved(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
