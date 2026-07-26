use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::Path;

const LOCK_FILE_NAME: &str = "forge.lock";

/// Releases on process death of any kind, including a crash, because the OS drops the advisory lock with the file descriptor.
pub struct InstanceLock {
    _file: File,
}

pub enum LockOutcome {
    Acquired(InstanceLock),
    AlreadyRunning,
    Unavailable(io::Error),
}

pub fn acquire(data_dir: &Path) -> LockOutcome {
    if let Err(err) = std::fs::create_dir_all(data_dir) {
        return LockOutcome::Unavailable(err);
    }

    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(data_dir.join(LOCK_FILE_NAME))
    {
        Ok(file) => file,
        Err(err) => return LockOutcome::Unavailable(err),
    };

    match file.try_lock() {
        Ok(()) => LockOutcome::Acquired(InstanceLock { _file: file }),
        Err(TryLockError::WouldBlock) => LockOutcome::AlreadyRunning,
        Err(TryLockError::Error(err)) => LockOutcome::Unavailable(err),
    }
}
