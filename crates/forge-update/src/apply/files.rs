use std::io;
use std::path::Path;
use std::time::Duration;

use crate::UpdateError;
use crate::apply_state::prefixed_sibling;
use crate::process::Clock;

const FAILED_BUILD_PREFIX: &str = ".forge-failed-";

#[derive(Clone, Copy)]
pub(crate) struct FileRetry<'a> {
    pub(crate) clock: &'a dyn Clock,
    pub(crate) attempts: u32,
    pub(crate) delay: Duration,
}

impl FileRetry<'_> {
    pub(crate) fn run<T>(&self, mut operation: impl FnMut() -> io::Result<T>) -> io::Result<T> {
        let mut attempt: u32 = 1;
        loop {
            match operation() {
                Ok(value) => return Ok(value),
                Err(error)
                    if attempt >= self.attempts || error.kind() == io::ErrorKind::NotFound =>
                {
                    return Err(error);
                }
                Err(_) => {
                    attempt = attempt.saturating_add(1);
                    self.clock.sleep(self.delay);
                }
            }
        }
    }

    pub(crate) fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.run(|| std::fs::rename(from, to))
    }
}

pub(crate) fn path_exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

pub(crate) fn remove_path(path: &Path) -> io::Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

pub(crate) fn swap_in(
    target: &Path,
    replacement: &Path,
    previous: &Path,
    retry: FileRetry<'_>,
) -> Result<(), UpdateError> {
    retry.run(|| remove_path(previous))?;
    retry.rename(target, previous)?;
    if let Err(error) = retry.rename(replacement, target) {
        retry.rename(previous, target)?;
        return Err(error.into());
    }
    Ok(())
}

pub(crate) fn restore_previous_build(
    target: &Path,
    previous: &Path,
    retry: FileRetry<'_>,
) -> Result<(), UpdateError> {
    if !path_exists(previous) {
        return Err(UpdateError::PreviousBuildMissing);
    }
    let failed = prefixed_sibling(target, FAILED_BUILD_PREFIX)?;
    retry.run(|| remove_path(&failed))?;
    let moved_aside = path_exists(target);
    if moved_aside {
        retry.rename(target, &failed)?;
    }
    if let Err(error) = retry.rename(previous, target) {
        if moved_aside {
            retry.rename(&failed, target)?;
        }
        return Err(error.into());
    }
    let _ = retry.run(|| remove_path(&failed));
    Ok(())
}
