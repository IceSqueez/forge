use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::UpdateError;

const STDIN_DRAIN_CHUNK: usize = 256;
const PARENT_WATCH_THREAD: &str = "forge-update-parent-watch";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildExit {
    Code(i32),
    Terminated,
}

impl ChildExit {
    pub const fn is_success(self) -> bool {
        matches!(self, Self::Code(0))
    }

    pub const fn code(self) -> Option<i32> {
        match self {
            Self::Code(code) => Some(code),
            Self::Terminated => None,
        }
    }
}

pub trait ChildProcess: Send {
    fn id(&self) -> u32;
    fn try_exit(&mut self) -> Result<Option<ChildExit>, UpdateError>;
    fn kill(&mut self) -> Result<(), UpdateError>;
}

pub trait ProcessHost {
    fn wait_for_parent_exit(&mut self, timeout: Duration) -> Result<bool, UpdateError>;
    fn spawn(
        &mut self,
        program: &Path,
        args: &[OsString],
    ) -> Result<Box<dyn ChildProcess>, UpdateError>;
}

pub trait Clock {
    fn now(&self) -> Instant;
    fn sleep(&self, duration: Duration);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

#[derive(Debug, Default)]
pub struct SystemProcessHost {
    parent_exit: Option<Receiver<()>>,
}

impl SystemProcessHost {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ProcessHost for SystemProcessHost {
    fn wait_for_parent_exit(&mut self, timeout: Duration) -> Result<bool, UpdateError> {
        let receiver = match self.parent_exit.take() {
            Some(receiver) => receiver,
            None => watch_stdin_until_closed()?,
        };
        match receiver.recv_timeout(timeout) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => Ok(true),
            Err(RecvTimeoutError::Timeout) => {
                self.parent_exit = Some(receiver);
                Ok(false)
            }
        }
    }

    fn spawn(
        &mut self,
        program: &Path,
        args: &[OsString],
    ) -> Result<Box<dyn ChildProcess>, UpdateError> {
        let child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Box::new(SystemChild(child)))
    }
}

fn watch_stdin_until_closed() -> Result<Receiver<()>, UpdateError> {
    let (closed_tx, closed_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name(PARENT_WATCH_THREAD.to_owned())
        .spawn(move || {
            let mut stdin = std::io::stdin().lock();
            let mut chunk = [0_u8; STDIN_DRAIN_CHUNK];
            while matches!(stdin.read(&mut chunk), Ok(read) if read > 0) {}
            let _ = closed_tx.send(());
        })?;
    Ok(closed_rx)
}

#[derive(Debug)]
struct SystemChild(Child);

impl ChildProcess for SystemChild {
    fn id(&self) -> u32 {
        self.0.id()
    }

    fn try_exit(&mut self) -> Result<Option<ChildExit>, UpdateError> {
        Ok(self
            .0
            .try_wait()?
            .map(|status| status.code().map_or(ChildExit::Terminated, ChildExit::Code)))
    }

    fn kill(&mut self) -> Result<(), UpdateError> {
        match self.0.kill() {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}
