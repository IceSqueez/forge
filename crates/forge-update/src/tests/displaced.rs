use std::path::{Path, PathBuf};
use std::time::Instant;

use super::apply::{msi_installer, valid_bundle};
use super::harness::{
    DB_MAIN, FROM, FakeClock, Fixture, LIVE_DB, NEW_BUILD, OLD_BUILD, SCHEMA, SNAPSHOT_DB, Spawn,
    TO, bundle_binary, candidate_args, set_writable,
};
use crate::apply_state::ApplyPhase;
use crate::{
    ApplyFailure, ApplyMode, ChildExit, InstallLayout, RollbackReason, StartAction,
    SupervisorOutcome, SupervisorSettings, UpdateReport, pending_start_action, take_update_report,
};

const OS_ERROR_MARK: &str = "os error";
const CRASH_CODE: i32 = 101;
const MSI_FATAL: i32 = 1603;

struct Displaced {
    fixture: Fixture,
    outcome: SupervisorOutcome,
    launches: Vec<super::harness::Launch>,
}

fn locked_dir(fixture: &Fixture) -> PathBuf {
    fixture.root.path().join("locked")
}

fn apply_displaced() -> Displaced {
    let mut fixture = Fixture::file_layout(InstallLayout::WindowsPortable, ApplyMode::Restart);
    let locked = locked_dir(&fixture);
    fixture.relocate_staged(&locked);
    set_writable(&locked, false);
    let install_dir = fixture.install_dir.clone();
    let clock = FakeClock::new().on_first_sleep(move || set_writable(&install_dir, false));
    let mut host = fixture.host([]).probing_the_lock_in(&fixture.data_dir);

    let outcome = fixture
        .supervise_with(&mut host, &clock, Some(SCHEMA))
        .unwrap();

    Displaced {
        fixture,
        outcome,
        launches: host.launches(),
    }
}

fn swapped_bundle() -> Fixture {
    let mut fixture = Fixture::bundle(ApplyMode::Restart, b"already unpacked");
    std::fs::rename(&fixture.target, fixture.previous()).unwrap();
    let binary = bundle_binary(&fixture.target);
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::write(&binary, NEW_BUILD).unwrap();
    fixture.enter_phase(ApplyPhase::AwaitingHealth);
    let locked = locked_dir(&fixture);
    fixture.relocate_previous(&locked);
    fixture
}

fn rollback_displaced(fixture: Fixture, schema: Option<u32>) -> Displaced {
    let locked = locked_dir(&fixture);
    set_writable(&locked, false);
    let install_dir = fixture.install_dir.clone();
    let clock = FakeClock::new().on_first_sleep(move || set_writable(&install_dir, false));
    let mut host = fixture
        .host([Spawn::exits(ChildExit::Code(CRASH_CODE))])
        .probing_the_lock_in(&fixture.data_dir);

    let outcome = fixture.supervise_with(&mut host, &clock, schema).unwrap();

    Displaced {
        fixture,
        outcome,
        launches: host.launches(),
    }
}

impl Displaced {
    fn unlock(&self) {
        set_writable(&self.fixture.install_dir, true);
        let locked = locked_dir(&self.fixture);
        if locked.exists() {
            set_writable(&locked, true);
        }
    }

    fn last_launch(&self) -> &super::harness::Launch {
        self.launches.last().unwrap()
    }
}

fn os_errors_in(detail: &str) -> usize {
    detail.matches(OS_ERROR_MARK).count()
}

fn file_operation_detail(data_dir: &Path) -> String {
    match take_update_report(data_dir, FROM).unwrap() {
        Some(UpdateReport::NotApplied {
            version,
            reason: ApplyFailure::FileOperation { detail },
        }) => {
            assert_eq!(version, TO);
            detail
        }
        other => panic!("expected a file-operation report, got {other:?}"),
    }
}

fn rollback_failed_detail(data_dir: &Path) -> String {
    match take_update_report(data_dir, FROM).unwrap() {
        Some(UpdateReport::RollbackFailed {
            failed_version,
            reason,
            detail,
        }) => {
            assert_eq!(failed_version, TO);
            assert_eq!(
                reason,
                RollbackReason::Crashed {
                    exit_code: Some(CRASH_CODE)
                }
            );
            detail
        }
        other => panic!("expected a rollback-failed report, got {other:?}"),
    }
}

#[test]
fn a_swap_that_cannot_be_undone_reports_both_errors() {
    let displaced = apply_displaced();
    displaced.unlock();

    let detail = file_operation_detail(&displaced.fixture.data_dir);

    assert_eq!(displaced.outcome, SupervisorOutcome::NotApplied);
    assert_eq!(os_errors_in(&detail), 2, "{detail}");
}

#[test]
fn a_swap_that_cannot_be_undone_keeps_the_records_for_the_next_start() {
    let displaced = apply_displaced();
    displaced.unlock();
    let fixture = &displaced.fixture;

    assert!(!fixture.target.exists());
    assert!(fixture.previous().exists());
    assert!(fixture.records_kept());
}

#[test]
fn a_swap_that_cannot_be_undone_launches_the_backup_with_the_lock_free() {
    let displaced = apply_displaced();
    displaced.unlock();
    let launch = displaced.last_launch();

    assert_eq!(displaced.launches.len(), 1);
    assert_eq!(launch.program, displaced.fixture.previous());
    assert!(launch.args.is_empty());
    assert_eq!(launch.lock_held, Some(false));
    assert_eq!(std::fs::read(&launch.program).unwrap(), OLD_BUILD);
}

#[test]
fn the_next_start_restores_the_target_a_failed_swap_displaced() {
    let displaced = apply_displaced();
    displaced.unlock();
    let fixture = &displaced.fixture;

    let action = pending_start_action(&fixture.data_dir, FROM).unwrap();

    assert_eq!(action, StartAction::Continue);
    assert_eq!(std::fs::read(&fixture.target).unwrap(), OLD_BUILD);
    assert!(!fixture.previous().exists());
    assert!(!fixture.staged.exists());
    assert!(fixture.records_cleared());
}

#[test]
fn the_next_start_keeps_the_report_of_what_displaced_the_target() {
    let displaced = apply_displaced();
    displaced.unlock();
    let fixture = &displaced.fixture;

    pending_start_action(&fixture.data_dir, FROM).unwrap();

    let detail = file_operation_detail(&fixture.data_dir);
    assert_eq!(os_errors_in(&detail), 2, "{detail}");
}

#[test]
fn a_rollback_that_cannot_be_undone_launches_the_backup_bundle_executable() {
    let displaced = rollback_displaced(swapped_bundle(), Some(SCHEMA));
    displaced.unlock();
    let fixture = &displaced.fixture;
    let launch = displaced.last_launch();

    assert_eq!(displaced.outcome, SupervisorOutcome::RollbackFailed);
    assert_eq!(displaced.launches[0].args, candidate_args());
    assert_eq!(displaced.launches.len(), 2);
    assert_eq!(launch.program, bundle_binary(&fixture.previous()));
    assert!(launch.args.is_empty());
    assert_eq!(launch.lock_held, Some(false));
    assert_eq!(std::fs::read(&launch.program).unwrap(), OLD_BUILD);
    assert!(fixture.records_kept());
}

#[test]
fn a_rollback_that_cannot_be_undone_reports_both_errors() {
    let displaced = rollback_displaced(swapped_bundle(), Some(SCHEMA));
    displaced.unlock();

    let detail = rollback_failed_detail(&displaced.fixture.data_dir);

    assert_eq!(os_errors_in(&detail), 2, "{detail}");
}

#[test]
fn a_rollback_that_cannot_be_undone_still_restores_a_migrated_database() {
    let displaced = rollback_displaced(swapped_bundle(), Some(SCHEMA + 1));
    displaced.unlock();
    let data_dir = &displaced.fixture.data_dir;

    assert_eq!(std::fs::read(data_dir.join(DB_MAIN)).unwrap(), SNAPSHOT_DB);
}

#[test]
fn a_rollback_that_cannot_be_undone_adds_a_failed_database_restore_to_the_detail() {
    let fixture = swapped_bundle();
    std::fs::remove_file(&fixture.snapshot).unwrap();

    let displaced = rollback_displaced(fixture, Some(SCHEMA + 1));
    displaced.unlock();
    let data_dir = &displaced.fixture.data_dir;

    let detail = rollback_failed_detail(data_dir);
    assert!(os_errors_in(&detail) >= 3, "{detail}");
    assert_eq!(std::fs::read(data_dir.join(DB_MAIN)).unwrap(), LIVE_DB);
}

#[test]
fn the_next_start_restores_the_bundle_a_failed_rollback_displaced() {
    let displaced = rollback_displaced(swapped_bundle(), Some(SCHEMA));
    displaced.unlock();
    let fixture = &displaced.fixture;

    let action = pending_start_action(&fixture.data_dir, FROM).unwrap();

    assert_eq!(action, StartAction::Continue);
    assert_eq!(
        std::fs::read(bundle_binary(&fixture.target)).unwrap(),
        OLD_BUILD
    );
    assert!(!fixture.previous().exists());
    assert!(fixture.records_cleared());
}

#[test]
fn an_msi_failure_whose_backup_cannot_return_reports_both_and_launches_the_backup() {
    let fixture = Fixture::file_layout(InstallLayout::WindowsPerUserMsi, ApplyMode::Restart);
    let target = fixture.target.clone();
    let install_dir = fixture.install_dir.clone();
    let mut host = fixture.host([Spawn::child(move |_| {
        std::fs::remove_file(&target).unwrap();
        set_writable(&install_dir, false);
        Some(ChildExit::Code(MSI_FATAL))
    })]);

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();
    set_writable(&fixture.install_dir, true);

    let launches = host.launches();
    let detail = file_operation_detail(&fixture.data_dir);
    assert_eq!(outcome, SupervisorOutcome::NotApplied);
    assert!(detail.contains(&MSI_FATAL.to_string()), "{detail}");
    assert_eq!(os_errors_in(&detail), 1, "{detail}");
    assert_eq!(launches.len(), 2);
    assert_eq!(launches[1].program, fixture.previous());
    assert!(fixture.records_kept());
}

#[test]
fn a_leftover_package_never_turns_a_finished_install_into_a_failure() {
    let mut msi = Fixture::file_layout(InstallLayout::WindowsPerUserMsi, ApplyMode::Restart);
    let mut bundle = Fixture::bundle(ApplyMode::Restart, &valid_bundle());
    for (label, fixture) in [("msi", &mut msi), ("bundle", &mut bundle)] {
        let locked = locked_dir(fixture);
        fixture.relocate_staged(&locked);
        set_writable(&locked, false);
        let installer = msi_installer(ChildExit::Code(0), &fixture.target);
        let mut host = match label {
            "msi" => fixture.host([installer]),
            _ => fixture.host([]),
        };

        let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();
        set_writable(&locked, true);

        assert_eq!(outcome, SupervisorOutcome::Updated, "{label}");
        assert!(fixture.staged.exists(), "{label}: the fault never fired");
        assert_eq!(
            take_update_report(&fixture.data_dir, TO).unwrap(),
            None,
            "{label}"
        );
    }
}

#[test]
fn an_interrupted_restore_retries_before_giving_up_and_keeps_the_records() {
    let fixture = Fixture::file_layout(InstallLayout::LinuxAppImage, ApplyMode::Restart);
    std::fs::rename(&fixture.target, fixture.previous()).unwrap();
    set_writable(&fixture.install_dir, false);
    let defaults = SupervisorSettings::default();
    let least_wait = defaults.file_retry_delay * (defaults.file_retry_attempts - 1);
    let started = Instant::now();

    let result = pending_start_action(&fixture.data_dir, FROM);
    let waited = started.elapsed();
    set_writable(&fixture.install_dir, true);

    assert!(result.is_err());
    assert!(waited >= least_wait, "gave up after {waited:?}");
    assert!(fixture.records_kept());
    assert!(fixture.previous().exists());
}
