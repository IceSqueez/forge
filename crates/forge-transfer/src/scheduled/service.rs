use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use forge_storage::{DataProvider, SettingsRepo};
use time::OffsetDateTime;
use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;

use crate::bundle::exclusivity::TransferExclusivity;
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
                    let now = self.parts.clock.now();
                    if self.is_due(now) {
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
        self.due_at_from(self.parts.clock.now())
    }

    fn is_due(&self, now: OffsetDateTime) -> bool {
        self.due_at_from(now) <= now
    }

    fn due_at_from(&self, now: OffsetDateTime) -> OffsetDateTime {
        let scheduled = next_due(self.progress.last_success_at, self.settings.interval(), now);
        self.retry_after
            .map_or(scheduled, |retry| retry.max(scheduled))
    }

    fn nap(&self) -> StdDuration {
        let now = self.parts.clock.now();
        let remaining = self.due_at_from(now) - now;
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_storage::DATABASE_FILE_NAME;
    use forge_storage_sqlite::SqliteBackend;
    use time::{Date, Month, OffsetDateTime, UtcOffset};
    use tokio::sync::{Notify, watch};
    use tokio_util::sync::CancellationToken;

    use super::{ScheduledBackupParts, Service, Signals, Trigger};
    use crate::exclusivity::TransferExclusivity;
    use crate::passphrase::Passphrase;
    use crate::schedule_key::create_schedule_key;
    use crate::scheduled::clock::BackupClock;
    use crate::scheduled::settings::{Progress, ScheduleSettings};
    use crate::scheduled::status::{ScheduleState, ScheduledBackupStatus};

    struct FixedClock;

    impl BackupClock for FixedClock {
        fn now(&self) -> OffsetDateTime {
            Date::from_calendar_date(2026, Month::October, 21)
                .unwrap()
                .with_hms(12, 0, 0)
                .unwrap()
                .assume_utc()
        }

        fn offset_at(&self, _instant: OffsetDateTime) -> UtcOffset {
            UtcOffset::UTC
        }
    }

    async fn service() -> (
        tempfile::TempDir,
        Service,
        watch::Receiver<ScheduledBackupStatus>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join(DATABASE_FILE_NAME).display());
        let backend = SqliteBackend::open_for_test(&url, [7; 32], dir.path().join("media"), None)
            .await
            .unwrap();
        let (status, receiver) = watch::channel(ScheduledBackupStatus::initial());
        let service = Service {
            parts: ScheduledBackupParts {
                provider: Arc::new(backend),
                data_dir: dir.path().to_path_buf(),
                exclusivity: TransferExclusivity::new(),
                clock: Arc::new(FixedClock),
            },
            signals: Arc::new(Signals {
                settings_changed: Notify::new(),
                storage_changed: Notify::new(),
                run_now: Notify::new(),
                cancel_in_flight: Notify::new(),
                shutdown: CancellationToken::new(),
            }),
            status,
            settings: ScheduleSettings::default(),
            progress: Progress::default(),
            retry_after: None,
            streak: None,
            prune_failures: 0,
            storage_used: None,
            suspended: false,
            running: false,
        };
        (dir, service, receiver)
    }

    #[tokio::test]
    async fn published_state_ranks_disabled_over_suspended_over_running_over_idle() {
        let (_dir, mut service, status) = service().await;
        for (enabled, suspended, running, expected) in [
            (false, true, true, ScheduleState::Disabled),
            (true, true, true, ScheduleState::Suspended),
            (true, false, true, ScheduleState::Running),
            (true, false, false, ScheduleState::Idle),
        ] {
            service.settings.enabled = enabled;
            service.suspended = suspended;
            service.running = running;

            service.publish();

            assert_eq!(
                status.borrow().state,
                expected,
                "{enabled} {suspended} {running}"
            );
        }
        service.parts.provider.shutdown().await;
    }

    #[tokio::test]
    async fn next_due_is_published_only_while_the_schedule_can_run() {
        let (_dir, mut service, status) = service().await;
        let mut seen = Vec::new();
        for (enabled, suspended) in [(true, false), (true, true), (false, false)] {
            service.settings.enabled = enabled;
            service.suspended = suspended;

            service.publish();

            seen.push(status.borrow().next_due.is_some());
        }

        assert_eq!(seen, [true, false, false]);
        service.parts.provider.shutdown().await;
    }

    #[tokio::test]
    async fn a_run_cancelled_by_shutdown_stops_the_writer_releases_the_guard_and_leaves_nothing() {
        let (dir, mut service, _status) = service().await;
        let folder = dir.path().join("out");
        std::fs::create_dir_all(&folder).unwrap();
        service.settings.enabled = true;
        service.settings.destination = Some(folder.clone());
        service.settings.key = Some(
            create_schedule_key(&Passphrase::new("correct horse battery staple".to_owned()))
                .unwrap(),
        );
        service.signals.shutdown.cancel();

        service.attempt(Trigger::Due).await;

        let backups = std::fs::read_dir(dir.path().join(forge_paths::BACKUPS_DIR_NAME))
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(
            (
                service.parts.exclusivity.is_held(),
                std::fs::read_dir(&folder).unwrap().count(),
                backups,
                service.streak,
                service.progress.last_success_at,
                service.retry_after.is_some(),
            ),
            (false, 0, 0, None, None, true)
        );
        service.parts.provider.shutdown().await;
    }

    struct TickingClock {
        calls: std::sync::atomic::AtomicI64,
    }

    impl BackupClock for TickingClock {
        fn now(&self) -> OffsetDateTime {
            let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            FixedClock.now() + time::Duration::microseconds(call)
        }

        fn offset_at(&self, _instant: OffsetDateTime) -> UtcOffset {
            UtcOffset::UTC
        }
    }

    #[tokio::test]
    async fn a_schedule_that_never_backed_up_runs_its_first_backup_on_a_moving_clock() {
        use forge_storage::{SettingsRepo, reserved_keys};
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join(DATABASE_FILE_NAME).display());
        let backend = SqliteBackend::open_for_test(&url, [7; 32], dir.path().join("media"), None)
            .await
            .unwrap();
        let key = create_schedule_key(&Passphrase::new("correct horse battery staple".to_owned()))
            .unwrap();
        backend
            .set_string(reserved_keys::BACKUP_SCHEDULE_ENABLED, "true")
            .await
            .unwrap();
        backend
            .set_string(reserved_keys::BACKUP_SCHEDULE_KEY, &key.encode().unwrap())
            .await
            .unwrap();
        let handle = super::spawn_scheduled_backups(ScheduledBackupParts {
            provider: Arc::new(backend),
            data_dir: dir.path().to_path_buf(),
            exclusivity: TransferExclusivity::new(),
            clock: Arc::new(TickingClock {
                calls: std::sync::atomic::AtomicI64::new(0),
            }),
        });
        let mut status = handle.status();

        let first_run = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            status.wait_for(|status| status.state == ScheduleState::Running),
        )
        .await;

        handle.shutdown();
        assert!(first_run.is_ok(), "the first backup never started");
    }
}
