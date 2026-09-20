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
