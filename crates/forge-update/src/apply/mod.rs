use std::time::Duration;

use crate::UpdateError;
use crate::apply_state::ApplyState;
use crate::install::InstallLayout;
use crate::journal::UpdateJournal;
use crate::process::{Clock, ProcessHost};

mod bundle;
mod executable;
pub(crate) mod files;
mod msi;

use files::{FileRetry, swap_in};

pub(crate) struct ApplyContext<'a> {
    pub(crate) host: &'a mut dyn ProcessHost,
    pub(crate) clock: &'a dyn Clock,
    pub(crate) installer_timeout: Duration,
    pub(crate) poll_interval: Duration,
    pub(crate) retry: FileRetry<'a>,
}

pub(crate) fn apply_update(
    journal: &UpdateJournal,
    state: &ApplyState,
    context: ApplyContext<'_>,
) -> Result<(), UpdateError> {
    let staged = state.staged_artifact.as_path();
    let target = journal.install_target.as_path();
    let previous = journal.previous_build.as_path();
    match state.layout {
        InstallLayout::MacAppBundle => bundle::apply(staged, target, previous, context.retry),
        InstallLayout::WindowsPortable => swap_in(target, staged, previous, context.retry),
        InstallLayout::LinuxAppImage => {
            executable::copy_mode_with_execute(target, staged)?;
            swap_in(target, staged, previous, context.retry)
        }
        InstallLayout::WindowsPerUserMsi => msi::apply(
            staged,
            target,
            previous,
            msi::InstallerRun {
                host: context.host,
                clock: context.clock,
                timeout: context.installer_timeout,
                poll_interval: context.poll_interval,
                retry: context.retry,
            },
        ),
    }
}
