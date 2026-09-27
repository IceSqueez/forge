use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, watch};

use crate::backend::open_platform_backend;
use crate::supervisor::{Command, Supervisor};
use crate::{AwakeError, AwakeStatus};

pub struct StayAwakeConfig {
    pub enabled: bool,
    /// Shown to the user by the OS power tools next to the hold.
    pub reason: String,
}

#[async_trait]
pub trait EnabledPreference: Send + Sync {
    async fn store(&self, enabled: bool);
}

#[derive(Clone)]
pub struct StayAwake {
    commands: mpsc::UnboundedSender<Command>,
    status: watch::Receiver<AwakeStatus>,
    preference: Arc<dyn EnabledPreference>,
}

impl StayAwake {
    /// Must be called inside a tokio runtime; the holds live on a task it spawns.
    pub fn spawn(config: StayAwakeConfig, preference: Arc<dyn EnabledPreference>) -> Self {
        let (status_tx, status_rx) = watch::channel(AwakeStatus::initial(config.enabled));
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let reason = config.reason;
        let enabled = config.enabled;
        tokio::spawn(async move {
            let (disruption_tx, disruption_rx) = mpsc::unbounded_channel();
            let backend = open_platform_backend(reason, disruption_tx).await;
            Supervisor {
                backend,
                enabled,
                status: status_tx,
                commands: command_rx,
                disruptions: disruption_rx,
            }
            .run()
            .await;
        });
        Self {
            commands: command_tx,
            status: status_rx,
            preference,
        }
    }

    /// Persists the choice, then applies it to the running holds.
    pub async fn set_enabled(&self, enabled: bool) {
        self.preference.store(enabled).await;
        let _ = self.commands.send(Command::SetEnabled(enabled));
    }

    pub fn status(&self) -> AwakeStatus {
        self.status.borrow().clone()
    }

    pub fn watch(&self) -> StatusWatch {
        StatusWatch(self.status.clone())
    }

    /// Releases every hold and stops the service without waiting; later calls do nothing.
    pub fn release(&self) {
        let _ = self.commands.send(Command::Release);
    }
}

pub struct StatusWatch(watch::Receiver<AwakeStatus>);

impl StatusWatch {
    pub fn current(&self) -> AwakeStatus {
        self.0.borrow().clone()
    }

    pub async fn changed(&mut self) -> Result<AwakeStatus, AwakeError> {
        self.0.changed().await.map_err(|_| AwakeError::Stopped)?;
        Ok(self.0.borrow_and_update().clone())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;
    use tokio::task::JoinHandle;

    use super::{EnabledPreference, StatusWatch, StayAwake};
    use crate::fake_backend::{Call, FakeBackend, spawn_supervisor};
    use crate::{Aspect, AwakeError, AwakeStatus};

    const WAIT: Duration = Duration::from_secs(5);

    #[derive(Default)]
    struct RecordingPreference(Mutex<Vec<bool>>);

    #[async_trait]
    impl EnabledPreference for RecordingPreference {
        async fn store(&self, enabled: bool) {
            self.0.lock().unwrap().push(enabled);
        }
    }

    fn stay_awake(
        backend: &FakeBackend,
        enabled: bool,
    ) -> (StayAwake, Arc<RecordingPreference>, JoinHandle<()>) {
        let (harness, task) = spawn_supervisor(backend, enabled);
        let preference = Arc::new(RecordingPreference::default());
        let handle = StayAwake {
            commands: harness.commands,
            status: harness.status,
            preference: preference.clone(),
        };
        (handle, preference, task)
    }

    async fn wait_until(watch: &mut StatusWatch, mut done: impl FnMut(&AwakeStatus) -> bool) {
        tokio::time::timeout(WAIT, async {
            while !done(&watch.current()) {
                watch.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn set_enabled_persists_the_choice() {
        let backend = FakeBackend::default();
        let (handle, preference, _task) = stay_awake(&backend, true);

        handle.set_enabled(false).await;

        assert_eq!(*preference.0.lock().unwrap(), vec![false]);
    }

    #[tokio::test]
    async fn set_enabled_false_releases_the_running_holds() {
        let backend = FakeBackend::default();
        let (handle, _preference, _task) = stay_awake(&backend, true);
        let mut watch = handle.watch();
        wait_until(&mut watch, AwakeStatus::fully_held).await;

        handle.set_enabled(false).await;
        wait_until(&mut watch, |s| *s == AwakeStatus::initial(false)).await;

        assert!(!backend.holds(Aspect::Display) && !backend.holds(Aspect::System));
    }

    #[tokio::test]
    async fn set_enabled_true_acquires_holds_without_restart() {
        let backend = FakeBackend::default();
        let (handle, _preference, _task) = stay_awake(&backend, false);
        let mut watch = handle.watch();

        handle.set_enabled(true).await;
        wait_until(&mut watch, AwakeStatus::fully_held).await;

        assert!(backend.holds(Aspect::Display) && backend.holds(Aspect::System));
    }

    #[tokio::test]
    async fn release_twice_releases_each_hold_once_and_stops() {
        let backend = FakeBackend::default();
        let (handle, _preference, task) = stay_awake(&backend, true);
        wait_until(&mut handle.watch(), AwakeStatus::fully_held).await;

        handle.release();
        handle.release();
        tokio::time::timeout(WAIT, task).await.unwrap().unwrap();
        handle.release();

        let releases: Vec<Call> = backend
            .calls()
            .into_iter()
            .filter(|call| matches!(call, Call::Release(_)))
            .collect();
        assert_eq!(
            releases,
            vec![
                Call::Release(Aspect::Display),
                Call::Release(Aspect::System)
            ]
        );
    }

    #[tokio::test]
    async fn status_watch_reports_stopped_once_the_service_ends() {
        let backend = FakeBackend::default();
        let (handle, _preference, task) = stay_awake(&backend, true);
        let mut watch = handle.watch();

        handle.release();
        tokio::time::timeout(WAIT, task).await.unwrap().unwrap();
        let mut outcome = watch.changed().await;
        while outcome.is_ok() {
            outcome = watch.changed().await;
        }

        assert_eq!(outcome, Err(AwakeError::Stopped));
    }
}
