use tokio::sync::{mpsc, watch};

use crate::backend::AwakeBackend;
use crate::{Aspect, AspectState, AwakeStatus};

pub(crate) enum Command {
    SetEnabled(bool),
    Release,
}

pub(crate) struct Supervisor {
    pub(crate) backend: Box<dyn AwakeBackend>,
    pub(crate) enabled: bool,
    pub(crate) status: watch::Sender<AwakeStatus>,
    pub(crate) commands: mpsc::UnboundedReceiver<Command>,
    pub(crate) disruptions: mpsc::UnboundedReceiver<Aspect>,
}

impl Supervisor {
    pub(crate) async fn run(mut self) {
        if self.enabled {
            self.acquire_all().await;
        }
        loop {
            tokio::select! {
                command = self.commands.recv() => match command {
                    Some(Command::SetEnabled(enabled)) => self.apply_enabled(enabled).await,
                    Some(Command::Release) | None => {
                        self.release_all().await;
                        tracing::info!("stay awake: holds released");
                        return;
                    }
                },
                Some(aspect) = self.disruptions.recv() => self.reacquire(aspect).await,
            }
        }
    }

    async fn apply_enabled(&mut self, enabled: bool) {
        if enabled == self.enabled {
            return;
        }
        self.enabled = enabled;
        self.status.send_modify(|status| status.enabled = enabled);
        if enabled {
            tracing::info!("stay awake: turned on");
            self.acquire_all().await;
        } else {
            tracing::info!("stay awake: turned off");
            self.release_all().await;
        }
    }

    async fn acquire_all(&mut self) {
        for aspect in Aspect::ALL {
            self.acquire(aspect).await;
        }
    }

    async fn release_all(&mut self) {
        for aspect in Aspect::ALL {
            if self.backend.is_held(aspect) {
                self.backend.release(aspect).await;
            }
            self.record(aspect, AspectState::Off);
        }
    }

    async fn reacquire(&mut self, aspect: Aspect) {
        if !self.enabled {
            return;
        }
        if self.backend.is_held(aspect) {
            self.backend.release(aspect).await;
        }
        self.acquire(aspect).await;
    }

    async fn acquire(&mut self, aspect: Aspect) {
        let state = match self.backend.acquire(aspect).await {
            Ok(()) => AspectState::Held,
            Err(e) => AspectState::Unavailable {
                reason: e.to_string(),
            },
        };
        self.record(aspect, state);
    }

    fn record(&mut self, aspect: Aspect, state: AspectState) {
        let previous = self.status.borrow().aspect(aspect).clone();
        if previous == state {
            return;
        }
        match &state {
            AspectState::Held => tracing::info!(?aspect, "stay awake: hold acquired"),
            AspectState::Unavailable { reason } => {
                tracing::warn!(?aspect, %reason, "stay awake: hold unavailable")
            }
            AspectState::Off | AspectState::Pending => {}
        }
        self.status.send_modify(|status| status.set(aspect, state));
    }
}
