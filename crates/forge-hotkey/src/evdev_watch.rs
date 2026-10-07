#![cfg(target_os = "linux")]

use std::mem::MaybeUninit;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};

use rustix::fs::inotify::{self, CreateFlags, ReadFlags, WatchFlags};
use rustix::io::Errno;
use tokio::io::unix::AsyncFd;

pub(crate) const INPUT_DIR: &str = "/dev/input";
pub(crate) const EVENT_NODE_PREFIX: &str = "event";
const WATCH_BUFFER_BYTES: usize = 4096;

pub(crate) enum InputDirChange {
    Appeared(Vec<PathBuf>),
    Rescan,
}

pub(crate) struct InputDirWatcher {
    dir: PathBuf,
    fd: AsyncFd<OwnedFd>,
    buf: Vec<MaybeUninit<u8>>,
}

impl InputDirWatcher {
    pub(crate) fn new(dir: &Path) -> std::io::Result<Self> {
        let fd = inotify::init(CreateFlags::NONBLOCK | CreateFlags::CLOEXEC)?;
        inotify::add_watch(
            &fd,
            dir,
            WatchFlags::CREATE | WatchFlags::ATTRIB | WatchFlags::MOVED_TO,
        )?;
        Ok(Self {
            dir: dir.to_path_buf(),
            fd: AsyncFd::new(fd)?,
            buf: vec![MaybeUninit::uninit(); WATCH_BUFFER_BYTES],
        })
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) async fn next_change(&mut self) -> std::io::Result<InputDirChange> {
        loop {
            let mut guard = self.fd.readable().await?;
            let mut appeared = Vec::new();
            let mut overflowed = false;
            let mut drained = false;
            {
                let mut reader = inotify::Reader::new(guard.get_inner(), &mut self.buf);
                loop {
                    match reader.next() {
                        Ok(event) => {
                            overflowed |= event.events().contains(ReadFlags::QUEUE_OVERFLOW);
                            if let Some(path) = event
                                .file_name()
                                .and_then(|name| name.to_str().ok())
                                .filter(|name| name.starts_with(EVENT_NODE_PREFIX))
                                .map(|name| self.dir.join(name))
                            {
                                appeared.push(path);
                            }
                            if reader.is_buffer_empty() {
                                break;
                            }
                        }
                        Err(Errno::AGAIN) => {
                            drained = true;
                            break;
                        }
                        Err(Errno::INTR) => continue,
                        Err(e) => return Err(e.into()),
                    }
                }
            }
            if drained {
                guard.clear_ready();
            }

            if overflowed {
                return Ok(InputDirChange::Rescan);
            }
            if !appeared.is_empty() {
                return Ok(InputDirChange::Appeared(appeared));
            }
        }
    }
}
