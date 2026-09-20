use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::watch;

const NO_GENERATION: u64 = 0;

#[derive(Clone, Copy)]
pub(crate) struct Generation(u64);

#[derive(Clone)]
pub(crate) struct RunState {
    running: watch::Sender<bool>,
    current: Arc<Mutex<u64>>,
}

impl RunState {
    pub(crate) fn new(running: watch::Sender<bool>) -> Self {
        Self {
            running,
            current: Arc::new(Mutex::new(NO_GENERATION)),
        }
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<bool> {
        self.running.subscribe()
    }

    pub(crate) fn claim(&self) -> Generation {
        let mut current = self.current();
        *current += 1;
        Generation(*current)
    }

    pub(crate) fn report_running(&self, generation: Generation) {
        let current = self.current();
        if *current == generation.0 {
            self.running.send_replace(true);
        }
    }

    pub(crate) fn report_stopped(&self, generation: Generation) {
        let current = self.current();
        if *current == generation.0 {
            self.running.send_replace(false);
        }
    }

    pub(crate) fn stop(&self) {
        let mut current = self.current();
        *current += 1;
        self.running.send_replace(false);
    }

    fn current(&self) -> MutexGuard<'_, u64> {
        self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNNING: bool = true;
    const STOPPED: bool = false;

    #[derive(Clone, Copy)]
    enum Step {
        Claim,
        Stop,
        ReportRunning(usize),
        ReportStopped(usize),
    }

    fn replay(initial: bool, script: &[Step]) -> bool {
        let (tx, rx) = watch::channel(initial);
        let state = RunState::new(tx);
        let mut claimed: Vec<Generation> = Vec::new();
        for step in script {
            match *step {
                Step::Claim => claimed.push(state.claim()),
                Step::Stop => state.stop(),
                Step::ReportRunning(nth) => state.report_running(claimed[nth]),
                Step::ReportStopped(nth) => state.report_stopped(claimed[nth]),
            }
        }
        *rx.borrow()
    }

    #[test]
    fn only_the_newest_claim_may_move_the_run_state() {
        use Step::{Claim, ReportRunning, ReportStopped, Stop};

        for (case, initial, script, expected) in [
            (
                "the newest claim reports running",
                STOPPED,
                &[Claim, ReportRunning(0)][..],
                RUNNING,
            ),
            (
                "the newest claim reports stopped",
                RUNNING,
                &[Claim, ReportStopped(0)][..],
                STOPPED,
            ),
            (
                "a superseded claim cannot report stopped",
                RUNNING,
                &[Claim, ReportRunning(0), Claim, ReportStopped(0)][..],
                RUNNING,
            ),
            (
                "a superseded claim cannot report running",
                STOPPED,
                &[Claim, Claim, ReportRunning(0)][..],
                STOPPED,
            ),
            (
                "an explicit stop supersedes the claim that was live when it ran",
                RUNNING,
                &[Claim, ReportRunning(0), Stop, ReportRunning(0)][..],
                STOPPED,
            ),
            (
                "a claim taken after a stop reports again",
                RUNNING,
                &[Claim, Stop, Claim, ReportRunning(1)][..],
                RUNNING,
            ),
        ] {
            assert_eq!(replay(initial, script), expected, "{case}");
        }
    }
}
