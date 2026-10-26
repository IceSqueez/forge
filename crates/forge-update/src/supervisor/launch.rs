use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::UpdateError;
use crate::apply_state::ApplyState;

pub const SUPERVISOR_FLAG: &str = "--forge-update-supervisor";
pub const CANDIDATE_FLAG: &str = "--forge-update-candidate";

const SUPERVISOR_DIR_PREFIX: &str = "forge-update-supervisor-";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchRole {
    Supervisor { data_dir: PathBuf },
    Candidate,
    Regular,
}

pub fn launch_role(args: impl IntoIterator<Item = OsString>) -> LaunchRole {
    let mut args = args.into_iter();
    match args.next() {
        Some(flag) if flag == SUPERVISOR_FLAG => {
            args.next()
                .map_or(LaunchRole::Regular, |data_dir| LaunchRole::Supervisor {
                    data_dir: PathBuf::from(data_dir),
                })
        }
        Some(flag) if flag == CANDIDATE_FLAG => LaunchRole::Candidate,
        _ => LaunchRole::Regular,
    }
}

pub fn launch_supervisor(data_dir: &Path) -> Result<u32, UpdateError> {
    let state = ApplyState::read(data_dir)?.ok_or(UpdateError::NoPendingUpdate)?;
    let program = copy_to_temp(&state.launch_executable)?;
    let mut supervisor = Command::new(program)
        .arg(SUPERVISOR_FLAG)
        .arg(data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let pid = supervisor.id();
    std::mem::forget(supervisor.stdin.take());
    Ok(pid)
}

fn copy_to_temp(executable: &Path) -> Result<PathBuf, UpdateError> {
    let name = executable
        .file_name()
        .ok_or(UpdateError::PathWithoutFileName)?;
    let temp = std::env::temp_dir();
    let dir = create_private_dir(&temp)?;
    sweep_previous_copies(&temp, &dir);
    let copy = dir.join(name);
    std::fs::copy(executable, &copy)?;
    Ok(copy)
}

fn create_private_dir(temp: &Path) -> std::io::Result<PathBuf> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(SUPERVISOR_DIR_PREFIX);
    #[cfg(unix)]
    builder.permissions(owner_only_permissions());
    Ok(builder.tempdir_in(temp)?.keep())
}

#[cfg(unix)]
fn owner_only_permissions() -> std::fs::Permissions {
    use std::os::unix::fs::PermissionsExt;

    const OWNER_ONLY_DIR_MODE: u32 = 0o700;
    std::fs::Permissions::from_mode(OWNER_ONLY_DIR_MODE)
}

fn sweep_previous_copies(temp: &Path, current: &Path) {
    let Ok(owner) = std::fs::symlink_metadata(current) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(temp) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let ours = entry
            .file_name()
            .to_string_lossy()
            .starts_with(SUPERVISOR_DIR_PREFIX);
        if !ours || path == current {
            continue;
        }
        let owned_dir = std::fs::symlink_metadata(&path)
            .is_ok_and(|metadata| metadata.is_dir() && same_owner(&metadata, &owner));
        if owned_dir {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "best-effort cleanup of an owned leftover directory"
            )]
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

#[cfg(unix)]
fn same_owner(candidate: &std::fs::Metadata, reference: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    candidate.uid() == reference.uid()
}

#[cfg(not(unix))]
fn same_owner(_candidate: &std::fs::Metadata, _reference: &std::fs::Metadata) -> bool {
    true
}
