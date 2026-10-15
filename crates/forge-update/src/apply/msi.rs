use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::files::{FileRetry, remove_path};
use crate::UpdateError;
use crate::process::{ChildExit, Clock, ProcessHost};

const SYSTEM_ROOT_ENV: &str = "SystemRoot";
const SYSTEM_DIR: &str = "System32";
const MSIEXEC: &str = "msiexec.exe";
const INSTALL_FLAG: &str = "/i";
const QUIET_FLAG: &str = "/qn";
const NO_RESTART_FLAG: &str = "/norestart";
const SUCCESS: i32 = 0;
const SUCCESS_REBOOT_REQUIRED: i32 = 3010;
const SUCCESS_REBOOT_INITIATED: i32 = 1641;

pub(super) struct InstallerRun<'a> {
    pub(super) host: &'a mut dyn ProcessHost,
    pub(super) clock: &'a dyn Clock,
    pub(super) timeout: Duration,
    pub(super) poll_interval: Duration,
    pub(super) retry: FileRetry<'a>,
}

pub(super) fn apply(
    package: &Path,
    target: &Path,
    previous: &Path,
    run: InstallerRun<'_>,
) -> Result<(), UpdateError> {
    let retry = run.retry;
    retry.run(|| remove_path(previous))?;
    std::fs::copy(target, previous)?;
    if let Err(error) = run_installer(package, run) {
        let _ = remove_path(previous);
        return Err(error);
    }
    remove_path(package)?;
    Ok(())
}

fn run_installer(package: &Path, run: InstallerRun<'_>) -> Result<(), UpdateError> {
    let args: Vec<OsString> = vec![
        INSTALL_FLAG.into(),
        package.as_os_str().to_owned(),
        QUIET_FLAG.into(),
        NO_RESTART_FLAG.into(),
    ];
    let mut installer = run.host.spawn(&msiexec_program(), &args)?;
    let started = run.clock.now();
    loop {
        if let Some(exit) = installer.try_exit()? {
            return match exit {
                ChildExit::Code(SUCCESS | SUCCESS_REBOOT_REQUIRED | SUCCESS_REBOOT_INITIATED) => {
                    Ok(())
                }
                ChildExit::Code(code) => Err(UpdateError::InstallerFailed(code)),
                ChildExit::Terminated => Err(UpdateError::InstallerTerminated),
            };
        }
        if run.clock.now().saturating_duration_since(started) >= run.timeout {
            installer.kill()?;
            return Err(UpdateError::InstallerTimedOut);
        }
        run.clock.sleep(run.poll_interval);
    }
}

fn msiexec_program() -> PathBuf {
    std::env::var_os(SYSTEM_ROOT_ENV)
        .filter(|root| !root.is_empty())
        .map_or_else(
            || PathBuf::from(MSIEXEC),
            |root| PathBuf::from(root).join(SYSTEM_DIR).join(MSIEXEC),
        )
}
