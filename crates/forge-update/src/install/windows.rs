use std::path::{Component, Path, PathBuf};

use super::{InstallEnvironment, InstallKind, InstallLayout, ManualReason, updatable};

const MACHINE_INSTALL_DIR: &str = "Forge";
const INSTALL_BIN_DIR: &str = "bin";
const EXECUTABLE_NAME: &str = "forge.exe";
const PER_USER_PROGRAMS_DIR: &str = "Programs";

pub(super) fn classify(env: &InstallEnvironment) -> InstallKind {
    let executable = &env.executable;
    if env
        .program_files
        .iter()
        .any(|root| is_within(executable, root))
    {
        return InstallKind::ManualOnly(ManualReason::WindowsMachineWideInstall);
    }
    let per_user = env
        .local_app_data
        .as_ref()
        .is_some_and(|local| is_within(executable, &local.join(PER_USER_PROGRAMS_DIR)));
    let layout = if per_user {
        InstallLayout::WindowsPerUserMsi
    } else {
        InstallLayout::WindowsPortable
    };
    updatable(layout, executable.clone())
}

pub(super) fn leftover_machine_install(env: &InstallEnvironment) -> Option<PathBuf> {
    env.program_files.iter().find_map(|root| {
        let install_dir = root.join(MACHINE_INSTALL_DIR);
        let installed_exe = install_dir.join(INSTALL_BIN_DIR).join(EXECUTABLE_NAME);
        (installed_exe.is_file() && !is_within(&env.executable, &install_dir))
            .then_some(install_dir)
    })
}

fn is_within(path: &Path, root: &Path) -> bool {
    let mut path_parts = path.components().filter(is_meaningful);
    root.components().filter(is_meaningful).all(|root_part| {
        path_parts.next().is_some_and(|part| {
            part.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&root_part.as_os_str().to_string_lossy())
        })
    })
}

fn is_meaningful(component: &Component<'_>) -> bool {
    !matches!(component, Component::CurDir)
}
