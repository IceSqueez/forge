#![allow(clippy::unwrap_used)]

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::backend::AwakeBackend;
use crate::supervisor::{Command, Supervisor};
use crate::{Aspect, AwakeError, AwakeStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Call {
    Acquire(Aspect),
    Release(Aspect),
}

#[derive(Default)]
struct Inner {
    refusals: HashMap<Aspect, AwakeError>,
    held: HashSet<Aspect>,
    calls: Vec<Call>,
}

#[derive(Clone, Default)]
pub(crate) struct FakeBackend(Arc<Mutex<Inner>>);

impl FakeBackend {
    pub(crate) fn refuse(&self, aspect: Aspect, err: AwakeError) {
        self.0.lock().unwrap().refusals.insert(aspect, err);
    }

    pub(crate) fn grant(&self, aspect: Aspect) {
        self.0.lock().unwrap().refusals.remove(&aspect);
    }

    pub(crate) fn calls(&self) -> Vec<Call> {
        self.0.lock().unwrap().calls.clone()
    }

    pub(crate) fn holds(&self, aspect: Aspect) -> bool {
        self.0.lock().unwrap().held.contains(&aspect)
    }

    pub(crate) fn acquire_count(&self, aspect: Aspect) -> usize {
        self.calls()
            .iter()
            .filter(|call| **call == Call::Acquire(aspect))
            .count()
    }
}

#[async_trait]
impl AwakeBackend for FakeBackend {
    async fn acquire(&mut self, aspect: Aspect) -> Result<(), AwakeError> {
        let mut inner = self.0.lock().unwrap();
        inner.calls.push(Call::Acquire(aspect));
        inner.held.remove(&aspect);
        match inner.refusals.get(&aspect) {
            Some(err) => Err(err.clone()),
            None => {
                inner.held.insert(aspect);
                Ok(())
            }
        }
    }

    async fn release(&mut self, aspect: Aspect) {
        let mut inner = self.0.lock().unwrap();
        inner.calls.push(Call::Release(aspect));
        inner.held.remove(&aspect);
    }

    fn is_held(&self, aspect: Aspect) -> bool {
        self.holds(aspect)
    }
}

pub(crate) struct Harness {
    pub(crate) commands: mpsc::UnboundedSender<Command>,
    pub(crate) disruptions: mpsc::UnboundedSender<Aspect>,
    pub(crate) status: watch::Receiver<AwakeStatus>,
}

pub(crate) fn supervisor(backend: &FakeBackend, enabled: bool) -> (Supervisor, Harness) {
    let (status_tx, status_rx) = watch::channel(AwakeStatus::initial(enabled));
    let (command_tx, command_rx) = mpsc::unbounded_channel();
    let (disruption_tx, disruption_rx) = mpsc::unbounded_channel();
    let supervisor = Supervisor {
        backend: Box::new(backend.clone()),
        enabled,
        status: status_tx,
        commands: command_rx,
        disruptions: disruption_rx,
    };
    let harness = Harness {
        commands: command_tx,
        disruptions: disruption_tx,
        status: status_rx,
    };
    (supervisor, harness)
}

pub(crate) fn spawn_supervisor(backend: &FakeBackend, enabled: bool) -> (Harness, JoinHandle<()>) {
    let (supervisor, harness) = supervisor(backend, enabled);
    (harness, tokio::spawn(supervisor.run()))
}
