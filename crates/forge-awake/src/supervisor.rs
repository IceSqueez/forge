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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::time::Duration;

    use tokio::sync::watch;

    use super::Command;
    use crate::fake_backend::{Call, FakeBackend, spawn_supervisor, supervisor};
    use crate::{Aspect, AspectState, AwakeError, AwakeStatus};

    const WAIT: Duration = Duration::from_secs(5);

    async fn wait_until(
        status: &mut watch::Receiver<AwakeStatus>,
        done: impl FnMut(&AwakeStatus) -> bool,
    ) {
        tokio::time::timeout(WAIT, status.wait_for(done))
            .await
            .unwrap()
            .unwrap();
    }

    fn missing_screen_saver() -> AwakeError {
        AwakeError::ServiceMissing {
            service: "screen saver service",
        }
    }

    #[tokio::test]
    async fn starting_enabled_holds_both_aspects() {
        let backend = FakeBackend::default();
        let (mut harness, _task) = spawn_supervisor(&backend, true);

        wait_until(&mut harness.status, AwakeStatus::fully_held).await;

        assert!(backend.holds(Aspect::Display) && backend.holds(Aspect::System));
    }

    #[tokio::test]
    async fn starting_disabled_takes_no_hold() {
        let backend = FakeBackend::default();
        let (harness, task) = spawn_supervisor(&backend, false);

        harness.commands.send(Command::Release).unwrap();
        tokio::time::timeout(WAIT, task).await.unwrap().unwrap();

        assert!(
            !backend
                .calls()
                .iter()
                .any(|call| matches!(call, Call::Acquire(_)))
        );
    }

    #[tokio::test]
    async fn failed_acquire_reports_the_error_text_as_the_unavailable_reason() {
        for err in [
            missing_screen_saver(),
            AwakeError::Unreachable {
                service: "session bus",
                reason: "connection timed out".into(),
            },
            AwakeError::Refused {
                service: "power manager",
                reason: "access denied".into(),
            },
            AwakeError::Unsupported,
        ] {
            let backend = FakeBackend::default();
            backend.refuse(Aspect::Display, err.clone());
            let (mut sup, harness) = supervisor(&backend, true);

            sup.acquire_all().await;

            assert_eq!(
                *harness.status.borrow().aspect(Aspect::Display),
                AspectState::Unavailable {
                    reason: err.to_string()
                },
                "for {err:?}"
            );
        }
    }

    #[tokio::test]
    async fn refused_display_does_not_prevent_holding_the_system() {
        let backend = FakeBackend::default();
        backend.refuse(Aspect::Display, missing_screen_saver());
        let (mut sup, harness) = supervisor(&backend, true);

        sup.acquire_all().await;

        assert_eq!(
            *harness.status.borrow().aspect(Aspect::System),
            AspectState::Held
        );
    }

    #[tokio::test]
    async fn disruption_acquires_the_display_once_a_provider_appears() {
        let backend = FakeBackend::default();
        backend.refuse(Aspect::Display, missing_screen_saver());
        let (mut harness, _task) = spawn_supervisor(&backend, true);
        wait_until(&mut harness.status, |s| {
            matches!(s.display, AspectState::Unavailable { .. })
        })
        .await;

        backend.grant(Aspect::Display);
        harness.disruptions.send(Aspect::Display).unwrap();

        wait_until(&mut harness.status, |s| s.display == AspectState::Held).await;

        assert!(backend.holds(Aspect::Display));
    }

    #[tokio::test]
    async fn disruption_reports_the_loss_when_the_os_refuses_a_new_hold() {
        let backend = FakeBackend::default();
        let (mut sup, harness) = supervisor(&backend, true);
        sup.acquire_all().await;
        backend.refuse(Aspect::Display, missing_screen_saver());

        sup.reacquire(Aspect::Display).await;

        assert!(matches!(
            harness.status.borrow().display,
            AspectState::Unavailable { .. }
        ));
    }

    #[tokio::test]
    async fn disruption_while_disabled_takes_no_hold() {
        let backend = FakeBackend::default();
        let (mut sup, _harness) = supervisor(&backend, false);

        sup.reacquire(Aspect::Display).await;

        assert!(backend.calls().is_empty());
    }

    #[tokio::test]
    async fn reacquire_with_the_same_outcome_does_not_notify_watchers() {
        let backend = FakeBackend::default();
        let (mut sup, mut harness) = supervisor(&backend, true);
        sup.acquire_all().await;
        harness.status.mark_unchanged();

        sup.reacquire(Aspect::Display).await;

        assert!(!harness.status.has_changed().unwrap());
    }

    #[tokio::test]
    async fn disabling_releases_every_hold() {
        let backend = FakeBackend::default();
        let (mut sup, _harness) = supervisor(&backend, true);
        sup.acquire_all().await;

        sup.apply_enabled(false).await;

        assert!(!backend.holds(Aspect::Display) && !backend.holds(Aspect::System));
    }

    #[tokio::test]
    async fn disabling_reports_both_aspects_off_even_when_one_was_unavailable() {
        let backend = FakeBackend::default();
        backend.refuse(Aspect::Display, missing_screen_saver());
        let (mut sup, harness) = supervisor(&backend, true);
        sup.acquire_all().await;

        sup.apply_enabled(false).await;

        assert_eq!(*harness.status.borrow(), AwakeStatus::initial(false));
    }

    #[tokio::test]
    async fn enabling_acquires_both_aspects() {
        let backend = FakeBackend::default();
        let (mut sup, harness) = supervisor(&backend, false);

        sup.apply_enabled(true).await;

        assert_eq!(
            *harness.status.borrow(),
            AwakeStatus {
                enabled: true,
                display: AspectState::Held,
                system: AspectState::Held,
            }
        );
    }

    #[tokio::test]
    async fn enabling_when_already_enabled_does_not_reacquire() {
        let backend = FakeBackend::default();
        let (mut sup, _harness) = supervisor(&backend, true);
        sup.acquire_all().await;

        sup.apply_enabled(true).await;

        assert_eq!(backend.acquire_count(Aspect::Display), 1);
    }

    #[tokio::test]
    async fn release_command_releases_every_hold_and_stops() {
        let backend = FakeBackend::default();
        let (mut harness, task) = spawn_supervisor(&backend, true);
        wait_until(&mut harness.status, AwakeStatus::fully_held).await;

        harness.commands.send(Command::Release).unwrap();
        tokio::time::timeout(WAIT, task).await.unwrap().unwrap();

        assert!(!backend.holds(Aspect::Display) && !backend.holds(Aspect::System));
    }

    #[tokio::test]
    async fn dropping_every_handle_releases_every_hold() {
        let backend = FakeBackend::default();
        let (mut harness, task) = spawn_supervisor(&backend, true);
        wait_until(&mut harness.status, AwakeStatus::fully_held).await;

        drop(harness.commands);
        tokio::time::timeout(WAIT, task).await.unwrap().unwrap();

        assert!(!backend.holds(Aspect::Display) && !backend.holds(Aspect::System));
    }
}
