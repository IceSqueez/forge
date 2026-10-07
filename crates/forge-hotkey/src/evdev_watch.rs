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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    use super::*;

    const WAIT: Duration = Duration::from_secs(5);
    const INOTIFY_QUEUE_LIMIT: &str = "/proc/sys/fs/inotify/max_queued_events";

    #[derive(Debug, Clone, Copy)]
    enum Arrival {
        Created,
        PermissionsChanged,
        MovedIn,
    }

    async fn next_change(watcher: &mut InputDirWatcher) -> InputDirChange {
        tokio::time::timeout(WAIT, watcher.next_change())
            .await
            .unwrap()
            .unwrap()
    }

    async fn appeared(watcher: &mut InputDirWatcher) -> Vec<PathBuf> {
        match next_change(watcher).await {
            InputDirChange::Appeared(paths) => paths,
            InputDirChange::Rescan => panic!("expected Appeared, got Rescan"),
        }
    }

    fn set_mode(path: &Path, mode: u32) {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[tokio::test]
    async fn an_event_node_that_is_created_chmodded_or_moved_in_is_reported() {
        for arrival in [
            Arrival::Created,
            Arrival::PermissionsChanged,
            Arrival::MovedIn,
        ] {
            let root = tempfile::tempdir().unwrap();
            let input = root.path().join("input");
            let staging = root.path().join("staging");
            std::fs::create_dir(&input).unwrap();
            std::fs::create_dir(&staging).unwrap();
            let node = input.join("event7");
            if matches!(arrival, Arrival::PermissionsChanged) {
                std::fs::File::create(&node).unwrap();
            }
            let mut watcher = InputDirWatcher::new(&input).unwrap();

            match arrival {
                Arrival::Created => {
                    std::fs::File::create(&node).unwrap();
                }
                Arrival::PermissionsChanged => set_mode(&node, 0o640),
                Arrival::MovedIn => {
                    let staged = staging.join("event7");
                    std::fs::File::create(&staged).unwrap();
                    std::fs::rename(&staged, &node).unwrap();
                }
            }

            assert_eq!(appeared(&mut watcher).await, vec![node], "{arrival:?}");
        }
    }

    #[tokio::test]
    async fn entries_that_are_not_event_nodes_are_not_reported() {
        let dir = tempfile::tempdir().unwrap();
        let mut watcher = InputDirWatcher::new(dir.path()).unwrap();

        std::fs::File::create(dir.path().join("mouse0")).unwrap();
        std::fs::create_dir(dir.path().join("by-id")).unwrap();
        std::fs::File::create(dir.path().join("event2")).unwrap();

        assert_eq!(
            appeared(&mut watcher).await,
            vec![dir.path().join("event2")]
        );
    }

    #[tokio::test]
    async fn an_overflowed_event_queue_asks_for_a_rescan() {
        let dir = tempfile::tempdir().unwrap();
        let entries = [dir.path().join("mouse0"), dir.path().join("mouse1")];
        for entry in &entries {
            std::fs::File::create(entry).unwrap();
        }
        let limit: usize = std::fs::read_to_string(INOTIFY_QUEUE_LIMIT)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let mut watcher = InputDirWatcher::new(dir.path()).unwrap();

        for change in 0..=limit {
            set_mode(&entries[change % entries.len()], 0o600);
        }

        assert!(matches!(
            next_change(&mut watcher).await,
            InputDirChange::Rescan
        ));
    }
}
