#![cfg(target_os = "linux")]
#![allow(unsafe_code)]

use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};

use tokio::io::unix::AsyncFd;

pub(crate) trait SoleDescriptorOwner: AsRawFd {}

impl SoleDescriptorOwner for OwnedFd {}

impl SoleDescriptorOwner for File {}

pub(crate) fn register_readiness<T: SoleDescriptorOwner>(inner: T) -> io::Result<AsyncFd<T>> {
    // SAFETY: implementors are `OwnedFd` and `File` only; each owns its descriptor until dropped and `as_raw_fd` always returns that same descriptor.
    unsafe { AsyncFd::register(inner) }.map_err(io::Error::from)
}
