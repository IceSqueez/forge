use std::ffi::OsString;
use std::io::{ErrorKind, Read};
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
            drain_until_closed(std::io::stdin().lock());
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the waiter may already have stopped listening"
            )]
            let _ = closed_tx.send(());
        })?;
    Ok(closed_rx)
}

fn drain_until_closed(mut input: impl Read) {
    let mut chunk = [0_u8; STDIN_DRAIN_CHUNK];
    loop {
        match input.read(&mut chunk) {
            Ok(0) => return,
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io::{self, ErrorKind, Read};

    use super::drain_until_closed;

    enum Step {
        Bytes(usize),
        Closed,
        Fails(ErrorKind),
    }

    struct ScriptedInput<'a> {
        steps: VecDeque<Step>,
        reads: &'a mut usize,
    }

    impl Read for ScriptedInput<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            *self.reads += 1;
            match self
                .steps
                .pop_front()
                .expect("read past the end of the script")
            {
                Step::Bytes(count) => Ok(count.min(buffer.len())),
                Step::Closed => Ok(0),
                Step::Fails(kind) => Err(kind.into()),
            }
        }
    }

    #[test]
    fn the_drain_outlasts_interruptions_and_data_and_stops_at_the_first_close_or_real_error() {
        let cases = [
            (
                "interrupted, then data, then closed",
                vec![
                    Step::Fails(ErrorKind::Interrupted),
                    Step::Bytes(5),
                    Step::Closed,
                ],
                3,
            ),
            (
                "a chunk past the buffer, then interrupted twice, then closed",
                vec![
                    Step::Bytes(1024),
                    Step::Fails(ErrorKind::Interrupted),
                    Step::Fails(ErrorKind::Interrupted),
                    Step::Closed,
                ],
                4,
            ),
            ("closed at once", vec![Step::Closed], 1),
            (
                "a broken pipe ends it before later data",
                vec![Step::Fails(ErrorKind::BrokenPipe), Step::Bytes(5)],
                1,
            ),
            (
                "an error after data ends it",
                vec![
                    Step::Bytes(5),
                    Step::Fails(ErrorKind::Other),
                    Step::Bytes(5),
                ],
                2,
            ),
        ];
        for (label, steps, expected_reads) in cases {
            let mut reads = 0;
            drain_until_closed(ScriptedInput {
                steps: steps.into(),
                reads: &mut reads,
            });

            assert_eq!(reads, expected_reads, "{label}");
        }
    }
}
