use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use forge_storage::{DataProvider, SettingsRepo};
use time::OffsetDateTime;
use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;

use crate::exclusivity::TransferExclusivity;
use crate::scheduled::clock::BackupClock;
use crate::scheduled::due::next_due;
use crate::scheduled::settings::{Progress, ScheduleSettings};
use crate::scheduled::status::{FailureStreak, ScheduleState, ScheduledBackupStatus};
use crate::scheduled::usage::{StorageUsed, measure};

const CLOCK_RECHECK_INTERVAL: StdDuration = StdDuration::from_secs(60);

pub struct ScheduledBackupParts {
    pub provider: Arc<dyn DataProvider>,
    pub data_dir: PathBuf,
    pub exclusivity: TransferExclusivity,
    pub clock: Arc<dyn BackupClock>,
}

pub(crate) struct Signals {
    pub settings_changed: Notify,
    pub storage_changed: Notify,
    pub run_now: Notify,
    pub cancel_in_flight: Notify,
    pub shutdown: CancellationToken,
}

#[derive(Clone)]
pub struct ScheduledBackupHandle {
    signals: Arc<Signals>,
    suspended: Arc<watch::Sender<bool>>,
    status: watch::Receiver<ScheduledBackupStatus>,
}

impl ScheduledBackupHandle {
    pub fn status(&self) -> watch::Receiver<ScheduledBackupStatus> {
        self.status.clone()
    }

    pub fn settings_changed(&self) {
        self.signals.settings_changed.notify_one();
    }

    pub fn storage_changed(&self) {
        self.signals.storage_changed.notify_one();
    }

    pub fn run_now(&self) {
        self.signals.run_now.notify_one();
    }

    pub fn cancel_in_flight(&self) {
        self.signals.cancel_in_flight.notify_waiters();
    }

    pub fn set_suspended(&self, suspended: bool) {
        self.suspended.send_replace(suspended);
        if suspended {
            self.cancel_in_flight();
        }
    }

    pub fn shutdown(&self) {
        self.signals.shutdown.cancel();
    }
}

pub fn spawn_scheduled_backups(parts: ScheduledBackupParts) -> ScheduledBackupHandle {
    let signals = Arc::new(Signals {
        settings_changed: Notify::new(),
        storage_changed: Notify::new(),
        run_now: Notify::new(),
        cancel_in_flight: Notify::new(),
        shutdown: CancellationToken::new(),
    });
    let (status_tx, status_rx) = watch::channel(ScheduledBackupStatus::initial());
    let (suspended_tx, suspended_rx) = watch::channel(false);
    let service = Service {
        parts,
        signals: Arc::clone(&signals),
        status: status_tx,
        settings: ScheduleSettings::default(),
        progress: Progress::default(),
        retry_after: None,
        streak: None,
        prune_failures: 0,
        storage_used: None,
        suspended: false,
        running: false,
    };
    tokio::spawn(service.drive(suspended_rx));
    ScheduledBackupHandle {
        signals,
        suspended: Arc::new(suspended_tx),
        status: status_rx,
    }
}

pub(crate) struct Service {
    pub(crate) parts: ScheduledBackupParts,
    pub(crate) signals: Arc<Signals>,
    pub(crate) status: watch::Sender<ScheduledBackupStatus>,
    pub(crate) settings: ScheduleSettings,
    pub(crate) progress: Progress,
    pub(crate) retry_after: Option<OffsetDateTime>,
    pub(crate) streak: Option<FailureStreak>,
    pub(crate) prune_failures: u32,
    pub(crate) storage_used: Option<StorageUsed>,
    pub(crate) suspended: bool,
    pub(crate) running: bool,
}

pub(crate) enum Trigger {
    Due,
    Manual,
}

impl Service {
    async fn drive(mut self, mut suspended_rx: watch::Receiver<bool>) {
        self.reload_settings().await;
        self.reload_progress().await;
        self.refresh_storage().await;
        self.publish();
        let signals = Arc::clone(&self.signals);
        loop {
            let nap = self.nap();
            tokio::select! {
                () = signals.shutdown.cancelled() => break,
                () = signals.settings_changed.notified() => self.on_settings_changed().await,
                () = signals.storage_changed.notified() => {
                    self.refresh_storage().await;
                    self.publish();
                }
                () = signals.run_now.notified() => {
                    if self.runnable() {
                        self.attempt(Trigger::Manual).await;
                    }
                }
                changed = suspended_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    self.suspended = *suspended_rx.borrow_and_update();
                    self.publish();
                }
                () = tokio::time::sleep(nap), if self.runnable() => {
                    if self.parts.clock.now() >= self.due_at() {
                        self.attempt(Trigger::Due).await;
                    }
                }
            }
        }
    }

    pub(crate) fn runnable(&self) -> bool {
        self.settings.enabled && !self.suspended
    }

    pub(crate) fn due_at(&self) -> OffsetDateTime {
        let now = self.parts.clock.now();
        let scheduled = next_due(self.progress.last_success_at, self.settings.interval(), now);
        self.retry_after
            .map_or(scheduled, |retry| retry.max(scheduled))
    }

    fn nap(&self) -> StdDuration {
        let remaining = self.due_at() - self.parts.clock.now();
        StdDuration::try_from(remaining)
            .unwrap_or(StdDuration::ZERO)
            .min(CLOCK_RECHECK_INTERVAL)
    }

    pub(crate) fn defer(&mut self) {
        self.retry_after = Some(self.parts.clock.now() + self.settings.interval());
        self.publish();
    }

    async fn on_settings_changed(&mut self) {
        self.reload_settings().await;
        self.retry_after = None;
        self.refresh_storage().await;
        self.publish();
    }

    fn settings_repo(&self) -> &dyn SettingsRepo {
        self.parts.provider.as_ref()
    }

    async fn reload_settings(&mut self) {
        if let Ok(loaded) = ScheduleSettings::load(self.settings_repo()).await {
            self.settings = loaded;
        }
    }

    async fn reload_progress(&mut self) {
        if let Ok(loaded) = Progress::load(self.settings_repo()).await {
            self.progress = loaded;
        }
    }

    pub(crate) async fn refresh_storage(&mut self) {
        let data_dir = self.parts.data_dir.clone();
        let destination = self.settings.destination.clone();
        self.storage_used =
            tokio::task::spawn_blocking(move || measure(&data_dir, destination.as_deref()))
                .await
                .ok();
    }

    pub(crate) fn publish(&self) {
        let state = if !self.settings.enabled {
            ScheduleState::Disabled
        } else if self.suspended {
            ScheduleState::Suspended
        } else if self.running {
            ScheduleState::Running
        } else {
            ScheduleState::Idle
        };
        let next = ScheduledBackupStatus {
            state,
            last_success_at: self.progress.last_success_at,
            failure: self.streak,
            next_due: self.runnable().then(|| self.due_at()),
            prune_failures: self.prune_failures,
            storage_used: self.storage_used,
        };
        self.status.send_if_modified(|current| {
            if *current == next {
                false
            } else {
                *current = next;
                true
            }
        });
    }
}
