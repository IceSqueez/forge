use std::path::{Path, PathBuf};

use super::harness::{FROM, Fixture, NEW_BUILD, OLD_BUILD, SCHEMA, Spawn, TO, set_writable};
use crate::apply::files::failed_build_path;
use crate::apply_state::ApplyPhase;
use crate::leftovers::remember_leftovers;
use crate::supervisor_lock::SupervisorLock;
use crate::{ApplyMode, InstallLayout, StartAction, SupervisorOutcome, pending_start_action};

const RECORD_FILE: &str = "update-leftovers.json";
const LEFTOVER_BYTES: &[u8] = b"leftover";

fn record_path(data_dir: &Path) -> PathBuf {
    data_dir.join(RECORD_FILE)
}

fn restart_fixture() -> Fixture {
    Fixture::file_layout(InstallLayout::LinuxAppImage, ApplyMode::Restart)
}

fn leftover_in(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("old-build");
    std::fs::write(&path, LEFTOVER_BYTES).unwrap();
    path
}

fn confirmed_with_a_locked_previous_build() -> (Fixture, PathBuf) {
    let mut fixture = restart_fixture();
    std::fs::rename(&fixture.target, fixture.previous()).unwrap();
    std::fs::write(&fixture.target, NEW_BUILD).unwrap();
    fixture.enter_phase(ApplyPhase::AwaitingHealth);
    let locked = fixture.root.path().join("locked");
    fixture.relocate_previous(&locked);
    set_writable(&locked, false);
    let mut host = fixture.host([Spawn::reports_healthy_at(fixture.data_dir.clone(), 1)]);

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(outcome, SupervisorOutcome::Updated);
    (fixture, locked)
}

fn failed_build_of(fixture: &Fixture) -> PathBuf {
    failed_build_path(&fixture.target).unwrap()
}

#[test]
fn a_previous_build_that_cannot_be_removed_is_recorded_after_a_confirmed_update() {
    let (fixture, locked) = confirmed_with_a_locked_previous_build();
    set_writable(&locked, true);

    assert!(fixture.previous().exists());
    assert!(record_path(&fixture.data_dir).exists());
    assert!(fixture.records_cleared());
}

#[test]
fn the_next_start_removes_a_recorded_previous_build_and_its_record() {
    let (fixture, locked) = confirmed_with_a_locked_previous_build();
    set_writable(&locked, true);

    let action = pending_start_action(&fixture.data_dir, TO).unwrap();

    assert_eq!(action, StartAction::Continue);
    assert!(!fixture.previous().exists());
    assert!(!record_path(&fixture.data_dir).exists());
}

#[test]
fn a_start_keeps_recording_only_the_leftovers_it_could_not_remove() {
    let root = tempfile::tempdir().unwrap();
    let locked = root.path().join("locked");
    let stuck = leftover_in(&locked);
    let removable = leftover_in(&root.path().join("free"));
    remember_leftovers(root.path(), [stuck.clone(), removable.clone()]).unwrap();
    set_writable(&locked, false);

    pending_start_action(root.path(), FROM).unwrap();
    set_writable(&locked, true);
    let after_the_blocked_start = (stuck.exists(), removable.exists());
    std::fs::write(&removable, LEFTOVER_BYTES).unwrap();
    pending_start_action(root.path(), FROM).unwrap();

    assert_eq!(after_the_blocked_start, (true, false));
    assert!(!stuck.exists());
    assert!(removable.exists(), "a removed path must leave the record");
}

#[test]
fn no_leftover_is_touched_while_a_journal_awaits_supervision() {
    for (label, phase, running) in [
        ("first start", Some(ApplyPhase::AwaitingFirstStart), FROM),
        ("stale update on the target version", None, TO),
    ] {
        let mut fixture = restart_fixture();
        if let Some(phase) = phase {
            fixture.enter_phase(phase);
        }
        let leftover = leftover_in(&fixture.root.path().join("free"));
        remember_leftovers(&fixture.data_dir, [leftover.clone()]).unwrap();

        let action = pending_start_action(&fixture.data_dir, running).unwrap();

        assert_eq!(action, StartAction::HandOff, "{label}");
        assert!(leftover.exists(), "{label}");
        assert!(record_path(&fixture.data_dir).exists(), "{label}");
    }
}

#[test]
fn no_leftover_is_touched_while_a_supervisor_runs() {
    let root = tempfile::tempdir().unwrap();
    let leftover = leftover_in(&root.path().join("free"));
    remember_leftovers(root.path(), [leftover.clone()]).unwrap();
    let _lock = SupervisorLock::try_acquire(root.path()).unwrap().unwrap();

    let action = pending_start_action(root.path(), FROM).unwrap();

    assert_eq!(action, StartAction::SupervisorRunning);
    assert!(leftover.exists());
}

#[test]
fn a_corrupt_record_does_not_block_a_start() {
    for (label, record) in [
        ("not json", b"\xff\x00{".as_slice()),
        ("empty", b"".as_slice()),
        ("wrong shape", br#"{"format":1}"#.as_slice()),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(record_path(root.path()), record).unwrap();

        let action = pending_start_action(root.path(), FROM).unwrap();

        assert_eq!(action, StartAction::Continue, "{label}");
    }
}

#[test]
fn a_record_of_an_unknown_format_deletes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let named = leftover_in(&root.path().join("free"));
    let record = serde_json::json!({ "format": 2, "paths": [named] });
    std::fs::write(record_path(root.path()), record.to_string()).unwrap();

    let action = pending_start_action(root.path(), FROM).unwrap();

    assert_eq!(action, StartAction::Continue);
    assert!(named.exists());
}

#[test]
fn a_new_leftover_survives_a_corrupt_record_and_is_cleaned_on_the_next_start() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(record_path(root.path()), b"not json").unwrap();
    let leftover = leftover_in(&root.path().join("free"));

    remember_leftovers(root.path(), [leftover.clone()]).unwrap();
    pending_start_action(root.path(), FROM).unwrap();

    assert!(!leftover.exists());
}

#[test]
fn the_failed_build_of_an_interrupted_update_goes_only_once_a_target_exists() {
    for (label, target_present, previous_present, failed_kept) in [
        ("target in place", true, false, false),
        ("target restored from the backup", false, true, false),
        ("no build but the failed one", false, false, true),
    ] {
        let fixture = restart_fixture();
        let failed = failed_build_of(&fixture);
        std::fs::write(&failed, NEW_BUILD).unwrap();
        if previous_present {
            std::fs::rename(&fixture.target, fixture.previous()).unwrap();
        } else if !target_present {
            std::fs::remove_file(&fixture.target).unwrap();
        }

        let action = pending_start_action(&fixture.data_dir, FROM).unwrap();

        assert_eq!(action, StartAction::Continue, "{label}");
        assert_eq!(failed.exists(), failed_kept, "{label}");
        assert_eq!(fixture.target.exists(), !failed_kept, "{label}");
        assert!(fixture.records_cleared(), "{label}");
    }
}

#[test]
fn a_failed_build_that_cannot_be_removed_keeps_the_records_and_the_next_start_retries() {
    let fixture = restart_fixture();
    let failed = failed_build_of(&fixture);
    std::fs::create_dir(&failed).unwrap();
    std::fs::write(failed.join("forge"), NEW_BUILD).unwrap();
    set_writable(&failed, false);

    let blocked = pending_start_action(&fixture.data_dir, FROM);
    let records_kept = fixture.records_kept();
    set_writable(&failed, true);
    let retried = pending_start_action(&fixture.data_dir, FROM).unwrap();

    assert!(blocked.is_err());
    assert!(records_kept);
    assert_eq!(retried, StartAction::Continue);
    assert!(!failed.exists());
    assert!(fixture.records_cleared());
    assert_eq!(std::fs::read(&fixture.target).unwrap(), OLD_BUILD);
}
