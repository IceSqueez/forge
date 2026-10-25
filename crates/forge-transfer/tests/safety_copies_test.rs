#![allow(clippy::unwrap_used)]
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use forge_storage::DATABASE_FILE_NAME;
use forge_storage_sqlite::SqliteBackend;
use forge_transfer::{
    RESTORE_JOURNAL_FILE, SafetyCopyMode, SafetyCopyReport, SafetyCopySettings, SafetyCopyUsage,
    TransferExclusivity, backup_file_stem, backups_dir, change_safety_copy_settings,
    clean_up_safety_copies, prune_safety_copies, safety_copy_usage,
};
use forge_update::ApplyState;
use tempfile::TempDir;
use time::{Duration, OffsetDateTime};

const SQLITE_KEY: [u8; 32] = [0x42; 32];
const STEM_PREFIX: &str = "forge-backup";
const PRE_PACK: &str = "forge-pre-pack";
const PRE_SETUP: &str = "forge-pre-setup";
const PRE_RESTORE: &str = "forge-pre-restore";
const ARCHIVE: &str = "tar.gz";
const PRE_UPDATE: &str = "pre-update.db";
const COPY_BYTES: usize = 64;
const KEEP: u32 = 2;
const DAYS: u32 = 30;

struct Store {
    dir: TempDir,
    repo: SqliteBackend,
    exclusivity: TransferExclusivity,
}

impl Store {
    async fn open() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join(DATABASE_FILE_NAME).display());
        let repo = SqliteBackend::open_for_test(&url, SQLITE_KEY, dir.path().join("media"), None)
            .await
            .unwrap();
        std::fs::create_dir_all(backups_dir(dir.path())).unwrap();
        Self {
            dir,
            repo,
            exclusivity: TransferExclusivity::new(),
        }
    }

    fn data_dir(&self) -> &Path {
        self.dir.path()
    }

    fn backups(&self) -> PathBuf {
        backups_dir(self.data_dir())
    }

    async fn policy(&self, mode: SafetyCopyMode) {
        SafetyCopySettings {
            mode,
            keep_newest: KEEP,
            delete_after_days: DAYS,
        }
        .save(&self.repo)
        .await
        .unwrap();
    }

    async fn prune(&self) -> SafetyCopyReport {
        prune_safety_copies(&self.repo, self.data_dir(), &self.exclusivity)
            .await
            .unwrap()
    }

    async fn clean_up(&self) -> SafetyCopyReport {
        clean_up_safety_copies(self.data_dir(), &self.exclusivity)
            .await
            .unwrap()
    }

    fn file(&self, name: &str) -> String {
        std::fs::write(self.backups().join(name), vec![0_u8; COPY_BYTES]).unwrap();
        name.to_owned()
    }

    fn dir_copy(&self, name: &str) -> String {
        let dir = self.backups().join(name);
        std::fs::create_dir_all(dir.join("media")).unwrap();
        std::fs::write(dir.join(DATABASE_FILE_NAME), vec![0_u8; COPY_BYTES]).unwrap();
        std::fs::write(dir.join("media").join("clip"), vec![0_u8; COPY_BYTES]).unwrap();
        name.to_owned()
    }

    fn pre_pack(&self, age: Duration, suffix: &str) -> String {
        self.file(&format!("{}.{ARCHIVE}", stamped(PRE_PACK, age, suffix)))
    }

    fn pre_setup(&self, age: Duration, suffix: &str) -> String {
        self.file(&format!("{}.{ARCHIVE}", stamped(PRE_SETUP, age, suffix)))
    }

    fn pre_update(&self, age: Duration, suffix: &str) -> String {
        self.file(&format!(
            "{}.{PRE_UPDATE}",
            stamped(STEM_PREFIX, age, suffix)
        ))
    }

    fn pre_restore(&self, age: Duration, suffix: &str) -> String {
        self.dir_copy(&stamped(PRE_RESTORE, age, suffix))
    }

    fn remaining(&self) -> BTreeSet<String> {
        std::fs::read_dir(self.backups())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }
}

fn stamped(prefix: &str, age: Duration, suffix: &str) -> String {
    let stem = backup_file_stem(OffsetDateTime::now_utc() - age, suffix);
    format!("{prefix}{}", stem.strip_prefix(STEM_PREFIX).unwrap())
}

fn hours(n: i64) -> Duration {
    Duration::hours(n)
}

fn days(n: i64) -> Duration {
    Duration::days(n)
}

fn names<const N: usize>(items: [&String; N]) -> BTreeSet<String> {
    items.into_iter().cloned().collect()
}

#[tokio::test]
async fn policy_prune_keeps_the_newest_copies_of_each_kind_and_touches_nothing_else() {
    let store = Store::open().await;
    store.policy(SafetyCopyMode::KeepNewest).await;
    let pack_new = store.pre_pack(hours(1), "aaaaa1");
    let pack_mid = store.pre_pack(hours(2), "aaaaa2");
    store.pre_pack(hours(3), "aaaaa3");
    let setup_only = store.pre_setup(days(900), "bbbbb1");
    let restore_new = store.pre_restore(hours(1), "ccccc1");
    let restore_mid = store.pre_restore(hours(2), "ccccc2");
    store.pre_restore(hours(3), "ccccc3");
    let update_new = store.pre_update(hours(1), "ddddd1");
    let update_mid = store.pre_update(hours(2), "ddddd2");
    store.pre_update(hours(3), "ddddd3");

    store.prune().await;

    assert_eq!(
        store.remaining(),
        names([
            &pack_new,
            &pack_mid,
            &setup_only,
            &restore_new,
            &restore_mid,
            &update_new,
            &update_mid
        ])
    );
}

#[tokio::test]
async fn clean_up_never_touches_bundles_partials_foreign_entries_or_unparseable_names() {
    let store = Store::open().await;
    let old = days(4000);
    let bystanders = [
        store.file(&format!(
            "{}.forgebundle",
            stamped(STEM_PREFIX, old, "eeeee1")
        )),
        store.file(&format!(
            "{}.forgebundle",
            stamped("forge-scheduled", old, "eeeee2")
        )),
        store.file(&format!(
            "{}.{ARCHIVE}",
            stamped(STEM_PREFIX, old, "eeeee3")
        )),
        store.file(&format!(
            "{}.{ARCHIVE}.partial",
            stamped(PRE_PACK, old, "eeeee4")
        )),
        store.file(&format!(
            "{}.{PRE_UPDATE}.partial",
            stamped(STEM_PREFIX, old, "eeeee5")
        )),
        store.file(&format!("{PRE_SETUP}-yesterday.{ARCHIVE}")),
        store.dir_copy(&format!("{PRE_RESTORE}-by-hand")),
        store.dir_copy("restore-staging"),
        store.file("notes.txt"),
    ];

    let report = store.clean_up().await;

    assert_eq!(
        (report.removed, store.remaining()),
        (0, bystanders.into_iter().collect())
    );
}

#[cfg(unix)]
#[tokio::test]
async fn clean_up_skips_a_symlink_named_like_a_safety_copy_and_leaves_its_target_alone() {
    let store = Store::open().await;
    let outside = store.data_dir().join("precious.tar.gz");
    std::fs::write(&outside, b"keep me").unwrap();
    let link = format!("{}.{ARCHIVE}", stamped(PRE_PACK, days(400), "fffff1"));
    std::os::unix::fs::symlink(&outside, store.backups().join(&link)).unwrap();
    let dir_target = store.data_dir().join("precious-dir");
    std::fs::create_dir_all(&dir_target).unwrap();
    std::fs::write(dir_target.join("forge.db"), b"keep me").unwrap();
    let dir_link = stamped(PRE_RESTORE, days(400), "fffff2");
    std::os::unix::fs::symlink(&dir_target, store.backups().join(&dir_link)).unwrap();

    store.clean_up().await;

    assert_eq!(
        (
            store.remaining(),
            outside.is_file(),
            dir_target.join("forge.db").is_file()
        ),
        (names([&link, &dir_link]), true, true)
    );
}

#[tokio::test]
async fn prune_reports_how_many_copies_it_removed_and_the_bytes_they_held() {
    let store = Store::open().await;
    store.policy(SafetyCopyMode::KeepNewest).await;
    store.pre_pack(hours(1), "aaaaa1");
    store.pre_pack(hours(2), "aaaaa2");
    store.pre_pack(hours(3), "aaaaa3");
    store.pre_restore(hours(1), "ccccc1");
    store.pre_restore(hours(2), "ccccc2");
    store.pre_restore(hours(3), "ccccc3");

    let report = store.prune().await;

    assert_eq!(
        report,
        SafetyCopyReport {
            removed: 2,
            freed_bytes: (COPY_BYTES * 3) as u64,
            failures: Vec::new(),
            deferred_by_transfer: false,
        }
    );
}

#[tokio::test]
async fn never_mode_prune_removes_nothing_however_old_or_numerous() {
    let store = Store::open().await;
    store.policy(SafetyCopyMode::Never).await;
    for n in 0..5 {
        store.pre_pack(days(4000 + n), &format!("aaaaa{n}"));
    }
    let before = store.remaining();

    let report = store.prune().await;

    assert_eq!((report.removed, store.remaining()), (0, before));
}

#[tokio::test]
async fn delete_after_days_removes_expired_copies_but_keeps_the_newest_of_each_kind() {
    let store = Store::open().await;
    store.policy(SafetyCopyMode::DeleteAfterDays).await;
    let pack_fresh = store.pre_pack(days(1), "aaaaa1");
    store.pre_pack(days(60), "aaaaa2");
    let setup_newest = store.pre_setup(days(90), "bbbbb1");
    store.pre_setup(days(91), "bbbbb2");
    let update_fresh = store.pre_update(hours(1), "ddddd1");
    let update_recent = store.pre_update(days(29), "ddddd2");

    store.prune().await;

    assert_eq!(
        store.remaining(),
        names([&pack_fresh, &setup_newest, &update_fresh, &update_recent])
    );
}

#[tokio::test]
async fn changing_the_policy_saves_it_and_prunes_under_it_at_once() {
    let store = Store::open().await;
    store.policy(SafetyCopyMode::Never).await;
    let newest = store.pre_pack(hours(1), "aaaaa1");
    store.pre_pack(hours(2), "aaaaa2");
    let changed = SafetyCopySettings {
        mode: SafetyCopyMode::KeepNewest,
        keep_newest: 1,
        delete_after_days: DAYS,
    };

    change_safety_copy_settings(&store.repo, store.data_dir(), &store.exclusivity, changed)
        .await
        .unwrap();

    assert_eq!(
        (
            SafetyCopySettings::load(&store.repo).await.unwrap(),
            store.remaining()
        ),
        (changed, names([&newest]))
    );
}

#[tokio::test]
async fn clean_up_removes_every_unprotected_copy_even_in_never_mode() {
    let store = Store::open().await;
    store.policy(SafetyCopyMode::Never).await;
    store.pre_pack(hours(1), "aaaaa1");
    store.pre_setup(hours(1), "bbbbb1");
    store.pre_restore(hours(1), "ccccc1");
    store.pre_update(hours(1), "ddddd1");

    let report = store.clean_up().await;

    assert_eq!((report.removed, store.remaining()), (4, BTreeSet::new()));
}

#[tokio::test]
async fn clean_up_spares_every_pre_update_copy_while_an_update_is_in_flight() {
    let store = Store::open().await;
    let apply_state = ApplyState::path_in(store.data_dir());
    std::fs::create_dir_all(apply_state.parent().unwrap()).unwrap();
    std::fs::write(&apply_state, b"{}").unwrap();
    let update_new = store.pre_update(hours(1), "ddddd1");
    let update_old = store.pre_update(days(400), "ddddd2");
    store.pre_pack(hours(1), "aaaaa1");

    store.clean_up().await;

    assert_eq!(store.remaining(), names([&update_new, &update_old]));
}

#[tokio::test]
async fn clean_up_spares_every_pre_restore_copy_while_the_restore_journal_is_unreadable() {
    let store = Store::open().await;
    std::fs::write(store.data_dir().join(RESTORE_JOURNAL_FILE), b"{ torn").unwrap();
    let restore_new = store.pre_restore(hours(1), "ccccc1");
    let restore_old = store.pre_restore(days(400), "ccccc2");
    store.pre_setup(hours(1), "bbbbb1");

    store.clean_up().await;

    assert_eq!(store.remaining(), names([&restore_new, &restore_old]));
}

#[tokio::test]
async fn a_running_transfer_defers_every_sweep_and_deletes_nothing() {
    let store = Store::open().await;
    store.policy(SafetyCopyMode::KeepNewest).await;
    for n in 0..4 {
        store.pre_pack(days(400 + n), &format!("aaaaa{n}"));
    }
    let before = store.remaining();
    let _transfer = store.exclusivity.try_acquire().unwrap();
    let changed = SafetyCopySettings {
        mode: SafetyCopyMode::KeepNewest,
        keep_newest: 1,
        delete_after_days: DAYS,
    };

    let reports = [
        store.prune().await,
        store.clean_up().await,
        change_safety_copy_settings(&store.repo, store.data_dir(), &store.exclusivity, changed)
            .await
            .unwrap(),
    ];

    assert_eq!(
        (
            reports
                .iter()
                .map(|r| (r.deferred_by_transfer, r.removed))
                .collect::<Vec<_>>(),
            store.remaining()
        ),
        (vec![(true, 0), (true, 0), (true, 0)], before)
    );
}

#[tokio::test]
async fn a_policy_change_during_a_running_transfer_is_still_saved() {
    let store = Store::open().await;
    let _transfer = store.exclusivity.try_acquire().unwrap();
    let changed = SafetyCopySettings {
        mode: SafetyCopyMode::DeleteAfterDays,
        keep_newest: KEEP,
        delete_after_days: DAYS,
    };

    change_safety_copy_settings(&store.repo, store.data_dir(), &store.exclusivity, changed)
        .await
        .unwrap();

    assert_eq!(
        SafetyCopySettings::load(&store.repo).await.unwrap(),
        changed
    );
}

#[tokio::test]
async fn the_sweep_after_a_transfer_ends_proceeds_normally() {
    let store = Store::open().await;
    store.policy(SafetyCopyMode::KeepNewest).await;
    for n in 0..3 {
        store.pre_pack(hours(1 + n), &format!("aaaaa{n}"));
    }
    drop(store.exclusivity.try_acquire().unwrap());

    let report = store.prune().await;

    assert_eq!((report.deferred_by_transfer, report.removed), (false, 1));
}

#[tokio::test]
async fn usage_counts_only_safety_copies_and_the_bytes_inside_them() {
    let store = Store::open().await;
    store.pre_pack(hours(1), "aaaaa1");
    store.pre_restore(hours(1), "ccccc1");
    store.file(&format!(
        "{}.forgebundle",
        stamped(STEM_PREFIX, hours(1), "eeeee1")
    ));
    store.dir_copy("restore-staging");

    let usage = safety_copy_usage(store.data_dir()).await.unwrap();

    assert_eq!(
        usage,
        SafetyCopyUsage {
            count: 2,
            bytes: (COPY_BYTES * 3) as u64,
        }
    );
}

#[tokio::test]
async fn a_data_dir_without_a_backups_folder_has_nothing_to_sweep_or_count() {
    let store = Store::open().await;
    std::fs::remove_dir_all(store.backups()).unwrap();

    let outcome = (
        store.clean_up().await,
        safety_copy_usage(store.data_dir()).await.unwrap(),
    );

    assert_eq!(
        outcome,
        (SafetyCopyReport::default(), SafetyCopyUsage::default())
    );
}
