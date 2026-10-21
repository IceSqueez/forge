#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_storage::paths::CREDENTIALS_KEY_FILE_NAME;
use forge_storage::{
    DATABASE_FILE_NAME, DatabaseFileSchema, EXPECTED_SCHEMA_VERSION, StorageError, StoredDatabase,
    StoredSchema, database_unit_file_names,
};
use forge_transfer::{
    ARCHIVE_ROOT, INCLUDED_DIRS, RESTORE_JOURNAL_FILE, RESTORE_MARKER_FILE, RestoreDeferral,
    RestoreOutcome, RestoreRollback, StageRequest, StagedRestore, TransferError, UpdateActivity,
    apply_pending_restore, record_pending_restore, restore_pending, restore_staging_root,
    stage_full_restore,
};
use forge_update::{ApplyState, UpdateJournal};
use serde_json::{Value, json};
use tempfile::TempDir;

const STAGE_NAME: &str = "forge-backup-20261019-120000Z-abcdef";
const SNAPSHOT_REL: &str = "backups/forge-pre-restore-20261019-120000Z-abcdef";

const ORIGINAL: &[(&str, &[u8])] = &[
    ("forge.db", b"original database"),
    ("forge.db-wal", b"original write-ahead log"),
    ("credentials-key", b"original credentials key"),
    ("media/a.ogg", b"original media"),
    ("overlays/o.html", b"original overlay"),
];

const RESTORED: &[(&str, &[u8])] = &[
    ("forge.db", b"restored database"),
    ("credentials-key", b"restored credentials key"),
    ("media/b.ogg", b"restored media"),
    ("tts/voice.onnx", b"restored voice"),
];

const STAGED_ENTRIES: [&str; 3] = ["forge.db", "media", "tts"];

const UNTOUCHED: &[(&str, &[u8])] = &[
    ("logs/forge.log", b"log line"),
    ("crashes/crash.txt", b"crash dump"),
    ("reports/report.json", b"problem report"),
    ("backups/forge-backup-old.forgebundle", b"older bundle"),
    ("forge.lock", b""),
];

type Tree = BTreeMap<String, Vec<u8>>;
type CrashState = fn(&Data);
type PathIn = fn(&Path) -> PathBuf;

fn write_file(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn plant(root: &Path, files: &[(&str, &[u8])]) {
    for (path, bytes) in files {
        write_file(&root.join(path), bytes);
    }
}

fn files_under(root: &Path) -> Tree {
    fn walk(root: &Path, dir: &Path, out: &mut Tree) {
        for child in std::fs::read_dir(dir).unwrap() {
            let path = child.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(relative, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = Tree::new();
    if root.exists() {
        walk(root, root, &mut out);
    }
    out
}

fn tree(groups: &[&[(&str, &[u8])]]) -> Tree {
    groups
        .iter()
        .flat_map(|group| group.iter())
        .map(|(path, bytes)| ((*path).to_owned(), bytes.to_vec()))
        .collect()
}

fn under(prefix: &str, files: &[(&str, &[u8])]) -> Tree {
    files
        .iter()
        .map(|(path, bytes)| (format!("{prefix}/{path}"), bytes.to_vec()))
        .collect()
}

fn scope_entries() -> Vec<String> {
    database_unit_file_names()
        .into_iter()
        .chain(INCLUDED_DIRS.iter().map(|dir| (*dir).to_owned()))
        .collect()
}

struct Data {
    dir: TempDir,
}

impl Data {
    fn new() -> Self {
        let data = Self {
            dir: TempDir::new().unwrap(),
        };
        plant(data.path(), ORIGINAL);
        plant(data.path(), UNTOUCHED);
        data
    }

    fn with_update_state() -> Self {
        let data = Self::new();
        write_file(&ApplyState::path_in(data.path()), b"{}");
        data
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn staging_dir(&self) -> PathBuf {
        restore_staging_root(self.path()).join(STAGE_NAME)
    }

    fn payload(&self) -> PathBuf {
        self.staging_dir().join(ARCHIVE_ROOT)
    }

    fn snapshot_dir(&self) -> PathBuf {
        self.path().join(SNAPSHOT_REL)
    }

    fn key(&self) -> PathBuf {
        self.path().join(CREDENTIALS_KEY_FILE_NAME)
    }

    fn stage(&self, files: &[(&str, &[u8])]) {
        plant(&self.payload(), files);
    }

    fn mark_pending(&self) {
        self.mark_pending_raw(&json!({ "format": 1, "staging": STAGE_NAME }).to_string());
    }

    fn mark_pending_raw(&self, contents: &str) {
        write_file(&self.path().join(RESTORE_MARKER_FILE), contents.as_bytes());
    }

    fn journal(&self, phase: Value, original_key: bool, staged_key: bool) {
        let journal = json!({
            "format": 1,
            "phase": phase,
            "staging_dir": self.staging_dir(),
            "snapshot_dir": self.snapshot_dir(),
            "credentials_key": self.key(),
            "original_key": original_key,
            "staged_entries": STAGED_ENTRIES,
            "staged_key": staged_key,
        });
        write_file(
            &self.path().join(RESTORE_JOURNAL_FILE),
            journal.to_string().as_bytes(),
        );
    }

    fn snapshot_first(&self, count: usize) {
        std::fs::create_dir_all(self.snapshot_dir()).unwrap();
        for name in scope_entries().into_iter().take(count) {
            let live = self.path().join(&name);
            if live.exists() {
                std::fs::rename(&live, self.snapshot_dir().join(&name)).unwrap();
            }
        }
    }

    fn snapshot_key(&self) {
        std::fs::copy(
            self.key(),
            self.snapshot_dir().join(CREDENTIALS_KEY_FILE_NAME),
        )
        .unwrap();
    }

    fn snapshot_all(&self) {
        self.snapshot_first(scope_entries().len());
        self.snapshot_key();
    }

    fn place_first(&self, count: usize) {
        for name in STAGED_ENTRIES.into_iter().take(count) {
            std::fs::rename(self.payload().join(name), self.path().join(name)).unwrap();
        }
    }

    fn place_key(&self) {
        std::fs::copy(self.payload().join(CREDENTIALS_KEY_FILE_NAME), self.key()).unwrap();
    }

    fn unsnapshot_first(&self, count: usize) {
        for name in scope_entries().into_iter().take(count) {
            let kept = self.snapshot_dir().join(&name);
            if kept.exists() {
                std::fs::rename(&kept, self.path().join(&name)).unwrap();
            }
        }
    }

    fn apply(&self) -> RestoreOutcome {
        apply_pending_restore(self.path()).unwrap()
    }

    fn tree(&self) -> Tree {
        files_under(self.path())
    }
}

fn snapshotting() -> Value {
    json!({ "phase": "snapshotting_current" })
}

fn placing() -> Value {
    json!({ "phase": "placing_staged" })
}

fn committed() -> Value {
    json!({ "phase": "committed" })
}

fn rolling_back(reason: Value) -> Value {
    json!({ "phase": "rolling_back", "reason": reason })
}

fn failure_reason() -> Value {
    json!({ "kind": "failed", "error": "simulated placement failure" })
}

fn update_state() -> &'static [(&'static str, &'static [u8])] {
    &[("update-apply.json", b"{}")]
}

fn original_tree(with_update_state: bool) -> Tree {
    let extra: &[(&str, &[u8])] = if with_update_state {
        update_state()
    } else {
        &[]
    };
    tree(&[ORIGINAL, UNTOUCHED, extra])
}

fn snapshot_of_original() -> Tree {
    under(SNAPSHOT_REL, ORIGINAL)
}

fn restored_tree(with_update_state: bool) -> Tree {
    let mut expected = tree(&[RESTORED, UNTOUCHED, update_state()]);
    if !with_update_state {
        expected.remove("update-apply.json");
    }
    expected.extend(snapshot_of_original());
    expected
}

fn applied_at_snapshot(data: &Data) -> RestoreOutcome {
    RestoreOutcome::Applied {
        pre_restore_snapshot: data.snapshot_dir(),
    }
}

fn interrupted_restore(data: &Data) {
    data.stage(RESTORED);
    data.mark_pending();
}

#[test]
fn crash_while_snapshotting_rolls_back_to_the_original_tree_at_every_step() {
    let entries = scope_entries().len();
    let cases = (0..=entries)
        .map(|moved| (moved, false))
        .chain([(entries, true)]);
    for (moved, key_copied) in cases {
        let data = Data::with_update_state();
        interrupted_restore(&data);
        data.journal(snapshotting(), true, true);
        data.snapshot_first(moved);
        if key_copied {
            data.snapshot_key();
        }

        let outcome = data.apply();

        assert_eq!(
            outcome,
            RestoreOutcome::RolledBack {
                reason: RestoreRollback::Interrupted
            },
            "moved {moved}, key copied {key_copied}"
        );
        assert_eq!(
            data.tree(),
            original_tree(true),
            "moved {moved}, key copied {key_copied}"
        );
    }
}

#[test]
fn crash_while_placing_completes_into_the_restored_tree_at_every_step() {
    let cases = (0..=STAGED_ENTRIES.len())
        .map(|placed| (placed, false))
        .chain([(STAGED_ENTRIES.len(), true)]);
    for (placed, key_placed) in cases {
        let data = Data::with_update_state();
        interrupted_restore(&data);
        data.journal(placing(), true, true);
        data.snapshot_all();
        data.place_first(placed);
        if key_placed {
            data.place_key();
        }

        let outcome = data.apply();

        assert_eq!(
            outcome,
            applied_at_snapshot(&data),
            "placed {placed}, key placed {key_placed}"
        );
        assert_eq!(
            data.tree(),
            restored_tree(true),
            "placed {placed}, key placed {key_placed}"
        );
    }
}

#[test]
fn crash_after_commit_finishes_cleanup_into_the_restored_tree() {
    for (marker_left, staging_left) in [(true, true), (false, true), (false, false)] {
        let data = Data::with_update_state();
        interrupted_restore(&data);
        data.journal(committed(), true, true);
        data.snapshot_all();
        data.place_first(STAGED_ENTRIES.len());
        data.place_key();
        if !marker_left {
            std::fs::remove_file(data.path().join(RESTORE_MARKER_FILE)).unwrap();
        }
        if !staging_left {
            std::fs::remove_dir_all(data.staging_dir()).unwrap();
        }

        let outcome = data.apply();

        assert_eq!(
            outcome,
            applied_at_snapshot(&data),
            "marker {marker_left}, staging {staging_left}"
        );
        assert_eq!(
            data.tree(),
            restored_tree(true),
            "marker {marker_left}, staging {staging_left}"
        );
    }
}

#[test]
fn crash_while_rolling_back_returns_to_the_original_tree_at_every_step() {
    let entries = scope_entries().len();
    let still_placed = (0..=STAGED_ENTRIES.len()).map(|placed| (placed, true, 0));
    let partly_unsnapshotted = (0..=entries).map(|restored| (0, false, restored));
    for (placed, key_placed, restored) in still_placed.chain(partly_unsnapshotted) {
        let data = Data::with_update_state();
        interrupted_restore(&data);
        data.journal(rolling_back(failure_reason()), true, true);
        data.snapshot_all();
        data.place_first(placed);
        if key_placed {
            data.place_key();
        }
        data.unsnapshot_first(restored);

        let outcome = data.apply();

        let case = format!("placed {placed}, key placed {key_placed}, restored {restored}");
        assert_eq!(
            outcome,
            RestoreOutcome::RolledBack {
                reason: RestoreRollback::Failed {
                    error: "simulated placement failure".to_owned()
                }
            },
            "{case}"
        );
        assert_eq!(data.tree(), original_tree(true), "{case}");
    }
}

#[test]
fn crash_midway_through_the_rollback_staging_sweep_keeps_the_original_tree() {
    let data = Data::with_update_state();
    data.journal(rolling_back(failure_reason()), true, true);
    std::fs::create_dir_all(data.payload()).unwrap();

    data.apply();

    assert_eq!(data.tree(), original_tree(true));
}

#[test]
fn crash_after_the_rollback_staging_sweep_still_settles_the_restore() {
    let data = Data::with_update_state();
    data.journal(rolling_back(failure_reason()), true, true);

    let outcome = apply_pending_restore(data.path());

    assert!(
        matches!(outcome, Ok(RestoreOutcome::RolledBack { .. })),
        "{outcome:?}"
    );
}

#[test]
fn second_apply_after_a_resumed_restore_changes_nothing() {
    let phases: [(&str, CrashState); 4] = [
        ("snapshotting", |data| {
            data.journal(snapshotting(), true, true);
            data.snapshot_first(1);
        }),
        ("placing", |data| {
            data.journal(placing(), true, true);
            data.snapshot_all();
            data.place_first(1);
        }),
        ("committed", |data| {
            data.journal(committed(), true, true);
            data.snapshot_all();
            data.place_first(STAGED_ENTRIES.len());
            data.place_key();
        }),
        ("rolling back", |data| {
            data.journal(rolling_back(failure_reason()), true, true);
            data.snapshot_all();
            data.place_first(1);
        }),
    ];
    for (phase, crash_state) in phases {
        let data = Data::with_update_state();
        interrupted_restore(&data);
        crash_state(&data);
        data.apply();
        let settled = data.tree();

        let again = data.apply();

        assert_eq!(again, RestoreOutcome::NonePending, "{phase}");
        assert_eq!(data.tree(), settled, "{phase}");
    }
}

#[test]
fn pending_restore_applies_and_keeps_the_pre_restore_snapshot() {
    let data = Data::new();
    interrupted_restore(&data);

    let outcome = data.apply();

    let RestoreOutcome::Applied {
        pre_restore_snapshot,
    } = outcome
    else {
        panic!("expected Applied, got {outcome:?}");
    };
    let snapshot_rel = pre_restore_snapshot
        .strip_prefix(data.path())
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let mut expected = tree(&[RESTORED, UNTOUCHED]);
    expected.extend(under(&snapshot_rel, ORIGINAL));
    assert_eq!(data.tree(), expected);
}

#[test]
fn applying_a_keyless_backup_removes_the_live_credentials_key() {
    let data = Data::new();
    let keyless: Vec<_> = RESTORED
        .iter()
        .copied()
        .filter(|(path, _)| *path != CREDENTIALS_KEY_FILE_NAME)
        .collect();
    data.stage(&keyless);
    data.mark_pending();

    data.apply();

    assert!(!data.key().exists());
}

#[test]
fn rollback_returns_the_credentials_key_to_its_pre_restore_presence() {
    for (original_key, staged_key) in [(true, false), (false, true)] {
        let data = Data::with_update_state();
        if !original_key {
            std::fs::remove_file(data.key()).unwrap();
        }
        interrupted_restore(&data);
        data.journal(rolling_back(failure_reason()), original_key, staged_key);
        data.snapshot_first(scope_entries().len());
        if original_key {
            data.snapshot_key();
        }
        data.place_first(STAGED_ENTRIES.len());
        if staged_key {
            data.place_key();
        } else {
            std::fs::remove_file(data.key()).unwrap();
        }

        data.apply();

        let mut expected = original_tree(true);
        if !original_key {
            expected.remove(CREDENTIALS_KEY_FILE_NAME);
        }
        assert_eq!(data.tree(), expected, "original key {original_key}");
    }
}

#[test]
fn blocked_placement_rolls_back_with_the_blocking_path_as_reason() {
    let data = Data::with_update_state();
    let without_tts: Vec<_> = RESTORED
        .iter()
        .copied()
        .filter(|(path, _)| !path.starts_with("tts/"))
        .collect();
    data.stage(&without_tts);
    data.mark_pending();
    data.journal(placing(), true, true);
    data.snapshot_all();

    let outcome = data.apply();

    let RestoreOutcome::RolledBack {
        reason: RestoreRollback::Failed { error },
    } = outcome
    else {
        panic!("expected a failed rollback, got {outcome:?}");
    };
    assert!(error.contains("tts"), "{error}");
    assert_eq!(data.tree(), original_tree(true));
}

#[test]
fn update_in_flight_defers_the_restore_and_keeps_everything_in_place() {
    for update_file in [ApplyState::path_in as PathIn, UpdateJournal::path_in] {
        let data = Data::new();
        interrupted_restore(&data);
        write_file(&update_file(data.path()), b"{}");
        let before = data.tree();

        let outcome = data.apply();

        assert_eq!(
            outcome,
            RestoreOutcome::Deferred {
                reason: RestoreDeferral::UpdateInProgress
            }
        );
        assert_eq!(data.tree(), before);
    }
}

#[test]
fn unreadable_marker_abandons_the_restore_and_sweeps_staging() {
    for marker in [
        "not json".to_owned(),
        json!({ "format": 2, "staging": STAGE_NAME }).to_string(),
        json!({ "format": 1, "staging": "../escape" }).to_string(),
        json!({ "format": 1, "staging": format!("{STAGE_NAME}/forge") }).to_string(),
    ] {
        let data = Data::new();
        data.stage(RESTORED);
        data.mark_pending_raw(&marker);

        let outcome = data.apply();

        assert_eq!(
            outcome,
            RestoreOutcome::RolledBack {
                reason: RestoreRollback::MarkerUnreadable
            },
            "{marker}"
        );
        assert_eq!(data.tree(), original_tree(false), "{marker}");
    }
}

#[test]
fn staging_without_a_database_abandons_the_restore() {
    let stagings: [&[(&str, &[u8])]; 2] = [&[], &[("media/b.ogg", b"restored media")]];
    for staged in stagings {
        let data = Data::new();
        data.stage(staged);
        data.mark_pending();

        let outcome = data.apply();

        assert_eq!(
            outcome,
            RestoreOutcome::RolledBack {
                reason: RestoreRollback::StagingMissing
            }
        );
        assert_eq!(data.tree(), original_tree(false));
    }
}

#[test]
fn orphan_staging_without_a_marker_is_swept() {
    let data = Data::new();
    data.stage(RESTORED);

    let outcome = data.apply();

    assert_eq!(outcome, RestoreOutcome::NonePending);
    assert_eq!(data.tree(), original_tree(false));
}

struct FakeSchema;

impl DatabaseFileSchema for FakeSchema {
    fn applied_schema_version(&self, _: &Path) -> Result<Option<u32>, StorageError> {
        Ok(None)
    }

    fn inspect(&self, _: &Path) -> Result<Option<StoredDatabase>, StorageError> {
        Ok(Some(StoredDatabase {
            schema: StoredSchema::Current {
                version: EXPECTED_SCHEMA_VERSION,
            },
            holds_credentials: false,
        }))
    }
}

fn legacy_bundle(dir: &Path) -> PathBuf {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(b"restored database".len() as u64);
    header.set_mode(0o600);
    builder
        .append_data(
            &mut header,
            format!("{ARCHIVE_ROOT}/{DATABASE_FILE_NAME}"),
            &b"restored database"[..],
        )
        .unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&builder.into_inner().unwrap()).unwrap();
    let bundle = dir.join("legacy.tar.gz");
    std::fs::write(&bundle, gz.finish().unwrap()).unwrap();
    bundle
}

async fn staged_in(data_dir: &Path) -> StagedRestore {
    let bundles = TempDir::new().unwrap();
    stage_full_restore(StageRequest {
        bundle: legacy_bundle(bundles.path()),
        data_dir: data_dir.to_path_buf(),
        passphrase: None,
        database_schema: Arc::new(FakeSchema),
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn recording_a_restore_refuses_while_an_update_is_in_flight() {
    let cases: [(UpdateActivity, Option<PathIn>); 3] = [
        (UpdateActivity::Staged, None),
        (UpdateActivity::Idle, Some(ApplyState::path_in)),
        (UpdateActivity::Idle, Some(UpdateJournal::path_in)),
    ];
    for (activity, update_file) in cases {
        let data = Data::new();
        let staged = staged_in(data.path()).await;
        if let Some(update_file) = update_file {
            write_file(&update_file(data.path()), b"{}");
        }

        let recorded = record_pending_restore(data.path(), &staged, activity).await;

        assert!(
            matches!(recorded, Err(TransferError::UpdateInProgress)),
            "{activity:?}: {recorded:?}"
        );
        assert!(!restore_pending(data.path()).unwrap());
    }
}

#[tokio::test]
async fn recording_a_second_restore_is_refused() {
    let data = Data::new();
    let first = staged_in(data.path()).await;
    let second = staged_in(data.path()).await;
    record_pending_restore(data.path(), &first, UpdateActivity::Idle)
        .await
        .unwrap();
    let marker = std::fs::read(data.path().join(RESTORE_MARKER_FILE)).unwrap();

    let recorded = record_pending_restore(data.path(), &second, UpdateActivity::Idle).await;

    assert!(matches!(
        recorded,
        Err(TransferError::RestoreAlreadyPending)
    ));
    assert_eq!(
        std::fs::read(data.path().join(RESTORE_MARKER_FILE)).unwrap(),
        marker
    );
}

#[tokio::test]
async fn recording_a_restore_staged_in_another_data_dir_is_refused() {
    let data = Data::new();
    let elsewhere = TempDir::new().unwrap();
    let staged = staged_in(elsewhere.path()).await;

    let recorded = record_pending_restore(data.path(), &staged, UpdateActivity::Idle).await;

    assert!(matches!(recorded, Err(TransferError::ForeignStaging)));
}

#[tokio::test]
async fn staged_and_recorded_restore_applies_at_the_next_boot() {
    let data = Data::new();
    let staged = staged_in(data.path()).await;
    record_pending_restore(data.path(), &staged, UpdateActivity::Idle)
        .await
        .unwrap();

    let outcome = data.apply();

    assert!(matches!(outcome, RestoreOutcome::Applied { .. }));
    assert_eq!(
        std::fs::read(data.path().join(DATABASE_FILE_NAME)).unwrap(),
        b"restored database"
    );
}

#[test]
fn rollback_of_a_category_scoped_restore_returns_only_the_listed_entries() {
    let data = Data::with_update_state();
    data.stage(&[("forge.db", b"restored database")]);
    data.mark_pending();
    let journal = json!({
        "format": 1,
        "phase": placing(),
        "staging_dir": data.staging_dir(),
        "snapshot_dir": data.snapshot_dir(),
        "credentials_key": data.key(),
        "original_key": true,
        "staged_entries": ["forge.db", "media"],
        "staged_key": false,
        "categories": ["database", "media"],
    });
    write_file(
        &data.path().join(RESTORE_JOURNAL_FILE),
        journal.to_string().as_bytes(),
    );
    std::fs::create_dir_all(data.snapshot_dir()).unwrap();
    for name in ["forge.db", "forge.db-wal", "media"] {
        std::fs::rename(data.path().join(name), data.snapshot_dir().join(name)).unwrap();
    }
    data.snapshot_key();

    let outcome = data.apply();

    assert!(
        matches!(outcome, RestoreOutcome::RolledBack { .. }),
        "{outcome:?}"
    );
    assert_eq!(data.tree(), original_tree(true));
}
