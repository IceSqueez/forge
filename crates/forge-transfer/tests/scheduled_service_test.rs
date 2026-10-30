#![allow(clippy::unwrap_used)]
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use forge_paths::{BACKUPS_DIR_NAME, MEDIA_DIR_NAME};
use forge_storage::settings::core_settings::{
    BACKUP_SCHEDULE_DESTINATION, BACKUP_SCHEDULE_ENABLED, BACKUP_SCHEDULE_INTERVAL_MINUTES,
    BACKUP_SCHEDULE_KEEP_DAILY_DAYS, BACKUP_SCHEDULE_KEEP_NEWEST, BACKUP_SCHEDULE_KEY,
    BACKUP_SCHEDULE_LAST_SUCCESS_AT,
};
use forge_storage::{
    DATABASE_FILE_NAME, DataProvider, RowClasses, Setting, SettingCodec, SettingsRepo,
    write_setting,
};
use forge_storage_sqlite::SqliteBackend;
use forge_transfer::{
    BUNDLE_EXTENSION, BackupClock, FailureCategory, Passphrase, ScheduleKey, ScheduleState,
    ScheduledBackupHandle, ScheduledBackupParts, ScheduledBackupStatus, TransferExclusivity,
    create_schedule_key, spawn_scheduled_backups,
};
use tempfile::TempDir;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime, UtcOffset};

const PASSPHRASE: &str = "correct horse battery staple";
const SQLITE_KEY: [u8; 32] = [0x42; 32];
const INTERVAL_MINUTES: i64 = 60;

struct FakeClock {
    now: Mutex<OffsetDateTime>,
}

impl FakeClock {
    fn starting_now() -> Arc<Self> {
        let now = OffsetDateTime::now_utc().replace_nanosecond(0).unwrap();
        Arc::new(Self {
            now: Mutex::new(now),
        })
    }

    fn advance(&self, by: Duration) -> OffsetDateTime {
        let mut now = self.now.lock().unwrap();
        *now += by;
        *now
    }
}

impl BackupClock for FakeClock {
    fn now(&self) -> OffsetDateTime {
        *self.now.lock().unwrap()
    }

    fn offset_at(&self, _instant: OffsetDateTime) -> UtcOffset {
        UtcOffset::UTC
    }
}

struct Fixture {
    dir: TempDir,
    backend: Arc<SqliteBackend>,
    provider: Arc<dyn DataProvider>,
    clock: Arc<FakeClock>,
    exclusivity: TransferExclusivity,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(data.join(MEDIA_DIR_NAME)).unwrap();
        std::fs::write(data.join(MEDIA_DIR_NAME).join("clip.ogg"), b"media").unwrap();
        std::fs::create_dir_all(dir.path().join("out")).unwrap();
        let url = format!("sqlite://{}", data.join(DATABASE_FILE_NAME).display());
        let backend =
            SqliteBackend::open_for_test(&url, SQLITE_KEY, data.join(MEDIA_DIR_NAME), None)
                .await
                .unwrap();
        let backend = Arc::new(backend);
        Self {
            dir,
            provider: Arc::clone(&backend) as Arc<dyn DataProvider>,
            backend,
            clock: FakeClock::starting_now(),
            exclusivity: TransferExclusivity::new(),
        }
    }

    fn data_dir(&self) -> PathBuf {
        self.dir.path().join("data")
    }

    fn folder(&self) -> PathBuf {
        self.dir.path().join("out")
    }

    async fn write<C: SettingCodec>(&self, setting: &Setting<C>, value: &C::Value) {
        let repo: &dyn SettingsRepo = self.provider.as_ref();
        write_setting(repo, setting, value).await.unwrap();
    }

    async fn seed_raw<C: SettingCodec>(&self, setting: &Setting<C>, raw: &str) {
        self.backend
            .seed_foreign_setting(setting.name(), raw, Some(RowClasses::from(&setting.spec())))
            .await
            .unwrap();
    }

    async fn enable_into(&self, folder: &Path) {
        let key = create_schedule_key(&Passphrase::new(PASSPHRASE.to_owned())).unwrap();
        self.store_key(&key).await;
        self.write(&BACKUP_SCHEDULE_ENABLED, &true).await;
        self.write(
            &BACKUP_SCHEDULE_INTERVAL_MINUTES,
            &u32::try_from(INTERVAL_MINUTES).unwrap(),
        )
        .await;
        self.write(
            &BACKUP_SCHEDULE_DESTINATION,
            &Some(folder.to_string_lossy().into_owned()),
        )
        .await;
    }

    async fn store_key(&self, key: &ScheduleKey) {
        self.write(&BACKUP_SCHEDULE_KEY, &Some(key.encode().unwrap()))
            .await;
    }

    async fn last_succeeded_at(&self, at: OffsetDateTime) {
        self.write(
            &BACKUP_SCHEDULE_LAST_SUCCESS_AT,
            &Some(at.format(&Rfc3339).unwrap()),
        )
        .await;
    }

    fn spawn(&self) -> ScheduledBackupHandle {
        let clock: Arc<dyn BackupClock> = self.clock.clone();
        spawn_scheduled_backups(ScheduledBackupParts {
            provider: Arc::clone(&self.provider),
            data_dir: self.data_dir(),
            exclusivity: self.exclusivity.clone(),
            clock,
        })
    }

    async fn finish(self, handle: ScheduledBackupHandle) {
        handle.shutdown();
        self.provider.shutdown().await;
    }
}

fn bundles_in(folder: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(&format!(".{BUNDLE_EXTENSION}")))
        .collect();
    names.sort();
    names
}

async fn settled(
    handle: &ScheduledBackupHandle,
    ready: impl FnMut(&ScheduledBackupStatus) -> bool,
) -> ScheduledBackupStatus {
    handle.status().wait_for(ready).await.unwrap().clone()
}

async fn succeeded_at(handle: &ScheduledBackupHandle, at: OffsetDateTime) -> ScheduledBackupStatus {
    settled(handle, |status| status.last_success_at == Some(at)).await
}

async fn failed_attempts(handle: &ScheduledBackupHandle, attempts: u32) -> ScheduledBackupStatus {
    settled(handle, |status| {
        status
            .failure
            .is_some_and(|streak| streak.attempts == attempts)
    })
    .await
}

fn interval() -> Duration {
    Duration::minutes(INTERVAL_MINUTES)
}

#[tokio::test]
async fn a_disabled_schedule_publishes_disabled_and_writes_nothing() {
    let fixture = Fixture::new().await;
    let handle = fixture.spawn();

    let status = settled(&handle, |status| status.storage_used.is_some()).await;

    assert_eq!(
        (
            status.state,
            status.next_due,
            bundles_in(&fixture.data_dir().join(BACKUPS_DIR_NAME))
        ),
        (ScheduleState::Disabled, None, Vec::<String>::new())
    );
    fixture.finish(handle).await;
}

#[tokio::test]
async fn runs_missed_while_closed_catch_up_once_and_the_next_is_one_interval_later() {
    let fixture = Fixture::new().await;
    fixture.enable_into(&fixture.folder()).await;
    let now = fixture.clock.now();
    fixture.last_succeeded_at(now - Duration::days(3)).await;
    let handle = fixture.spawn();

    let status = succeeded_at(&handle, now).await;

    assert_eq!(
        (
            bundles_in(&fixture.folder()).len(),
            status.state,
            status.next_due
        ),
        (1, ScheduleState::Idle, Some(now + interval()))
    );
    fixture.finish(handle).await;
}

#[tokio::test]
async fn a_due_run_with_unchanged_content_writes_no_second_bundle() {
    let fixture = Fixture::new().await;
    fixture.enable_into(&fixture.folder()).await;
    let handle = fixture.spawn();
    succeeded_at(&handle, fixture.clock.now()).await;

    let later = fixture.clock.advance(interval() + Duration::minutes(1));
    handle.storage_changed();
    succeeded_at(&handle, later).await;

    assert_eq!(bundles_in(&fixture.folder()).len(), 1);
    fixture.finish(handle).await;
}

#[tokio::test]
async fn backup_now_writes_even_when_nothing_changed_and_nothing_is_due() {
    let fixture = Fixture::new().await;
    fixture.enable_into(&fixture.folder()).await;
    let handle = fixture.spawn();
    succeeded_at(&handle, fixture.clock.now()).await;

    let soon = fixture.clock.advance(Duration::minutes(1));
    handle.run_now();
    succeeded_at(&handle, soon).await;

    assert_eq!(bundles_in(&fixture.folder()).len(), 2);
    fixture.finish(handle).await;
}

#[tokio::test]
async fn a_due_run_finding_the_transfer_held_skips_to_the_next_due_time() {
    let fixture = Fixture::new().await;
    fixture.enable_into(&fixture.folder()).await;
    let guard = fixture.exclusivity.try_acquire().unwrap();
    let now = fixture.clock.now();
    let handle = fixture.spawn();

    let skipped = settled(&handle, |status| status.next_due == Some(now + interval())).await;
    let while_held = (
        bundles_in(&fixture.folder()).len(),
        skipped.last_success_at,
        skipped.failure,
    );
    drop(guard);
    let next = fixture.clock.advance(interval());
    handle.storage_changed();
    succeeded_at(&handle, next).await;

    assert_eq!(
        (while_held, bundles_in(&fixture.folder()).len()),
        ((0, None, None), 1)
    );
    fixture.finish(handle).await;
}

#[tokio::test]
async fn a_missing_destination_grows_a_failure_streak_until_a_run_succeeds() {
    let fixture = Fixture::new().await;
    let unplugged = fixture.dir.path().join("unplugged");
    fixture.enable_into(&unplugged).await;
    let handle = fixture.spawn();

    let first = failed_attempts(&handle, 1).await.failure.unwrap();
    fixture.clock.advance(interval());
    handle.storage_changed();
    let second = failed_attempts(&handle, 2).await.failure.unwrap();
    let never_created = !unplugged.exists();
    std::fs::create_dir_all(&unplugged).unwrap();
    let recovered_at = fixture.clock.advance(interval());
    handle.storage_changed();
    let recovered = succeeded_at(&handle, recovered_at).await;

    assert_eq!(
        (
            (first.category, second.category),
            second.since == first.since,
            never_created,
            recovered.failure
        ),
        (
            (
                FailureCategory::DestinationMissing,
                FailureCategory::DestinationMissing
            ),
            true,
            true,
            None
        )
    );
    fixture.finish(handle).await;
}

#[cfg(unix)]
#[tokio::test]
async fn a_read_only_destination_fails_as_not_writable_and_leaves_no_file() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new().await;
    let locked = fixture.dir.path().join("locked");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    fixture.enable_into(&locked).await;
    let handle = fixture.spawn();

    let streak = failed_attempts(&handle, 1).await.failure.unwrap();
    let left = std::fs::read_dir(&locked).unwrap().count();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(
        (streak.category, left),
        (FailureCategory::DestinationNotWritable, 0)
    );
    fixture.finish(handle).await;
}

#[tokio::test]
async fn an_enabled_schedule_without_a_key_fails_as_key_missing() {
    let fixture = Fixture::new().await;
    fixture.write(&BACKUP_SCHEDULE_ENABLED, &true).await;
    let handle = fixture.spawn();

    let streak = failed_attempts(&handle, 1).await.failure.unwrap();

    assert_eq!(streak.category, FailureCategory::KeyMissing);
    fixture.finish(handle).await;
}

#[tokio::test]
async fn a_corrupted_stored_key_fails_as_key_unreadable_and_writes_no_bundle() {
    let fixture = Fixture::new().await;
    fixture.enable_into(&fixture.folder()).await;
    fixture
        .seed_raw(&BACKUP_SCHEDULE_KEY, "{\"recipient\":")
        .await;
    let handle = fixture.spawn();

    let streak = failed_attempts(&handle, 1).await.failure.unwrap();

    assert_eq!(
        (streak.category, bundles_in(&fixture.folder())),
        (FailureCategory::KeyUnreadable, Vec::<String>::new())
    );
    fixture.finish(handle).await;
}

#[tokio::test]
async fn a_successful_write_prunes_older_scheduled_bundles_beyond_retention() {
    let fixture = Fixture::new().await;
    fixture.enable_into(&fixture.folder()).await;
    fixture.write(&BACKUP_SCHEDULE_KEEP_NEWEST, &1).await;
    fixture.write(&BACKUP_SCHEDULE_KEEP_DAILY_DAYS, &0).await;
    let handle = fixture.spawn();
    succeeded_at(&handle, fixture.clock.now()).await;
    let first = bundles_in(&fixture.folder());

    let soon = fixture.clock.advance(Duration::minutes(1));
    handle.run_now();
    let status = succeeded_at(&handle, soon).await;
    let remaining = bundles_in(&fixture.folder());

    assert_eq!(
        (remaining.len(), remaining == first, status.prune_failures),
        (1, false, 0)
    );
    fixture.finish(handle).await;
}

#[tokio::test]
async fn suspending_publishes_suspended_without_a_next_due_and_resuming_restores_idle() {
    let fixture = Fixture::new().await;
    fixture.enable_into(&fixture.folder()).await;
    let handle = fixture.spawn();
    succeeded_at(&handle, fixture.clock.now()).await;

    handle.set_suspended(true);
    let suspended = settled(&handle, |status| status.state == ScheduleState::Suspended).await;
    handle.set_suspended(false);
    let resumed = settled(&handle, |status| status.state == ScheduleState::Idle).await;

    assert_eq!(
        (suspended.next_due, resumed.next_due.is_some()),
        (None, true)
    );
    fixture.finish(handle).await;
}
