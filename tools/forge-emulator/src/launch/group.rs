use tokio::process::Child;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stop {
    Terminate,
    Kill,
}

/// Signals forge's whole process group (it leads its own), so helpers it spawned stop with it.
/// Only call while `child` is unreaped: its pid, and so the group id, cannot be reused before then.
#[cfg(unix)]
pub(crate) fn signal(child: &mut Child, leader: u32, stop: Stop) {
    use rustix::process::{Pid, Signal, kill_process_group};

    let signal = match stop {
        Stop::Terminate => Signal::TERM,
        Stop::Kill => Signal::KILL,
    };
    let group = i32::try_from(leader)
        .ok()
        .filter(|raw| *raw > 1)
        .and_then(Pid::from_raw);
    match group {
        Some(group) => {
            if kill_process_group(group, signal).is_err() && stop == Stop::Kill {
                let _ = child.start_kill();
            }
        }
        None => {
            let _ = child.start_kill();
        }
    }
}

#[cfg(not(unix))]
pub(crate) fn signal(child: &mut Child, _leader: u32, _stop: Stop) {
    let _ = child.start_kill();
}
