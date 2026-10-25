#![allow(clippy::unwrap_used, clippy::panic)]
mod harness;

use std::sync::{Arc, Mutex};

use forge_update::{
    ApplyMode, ChildExit, DatabaseRestore, InstallLayout, RollbackReason, StartAction,
    SupervisorOutcome, UpdateError, UpdateReport, pending_start_action, take_update_report,
};
use harness::{
    DB_MAIN, DB_SHM, DB_WAL, FROM, FakeClock, Fixture, LIVE_DB, NEW_BUILD, OLD_BUILD, SCHEMA,
    SNAPSHOT_DB, Spawn, TO, candidate_args, settings,
};

type ReasonCheck = fn(&RollbackReason) -> bool;
type CandidateFor = fn(&Fixture) -> Spawn;

fn restart_fixture() -> Fixture {
    Fixture::file_layout(InstallLayout::LinuxAppImage, ApplyMode::Restart)
}

fn unhealthy_cases() -> Vec<(&'static str, Spawn, ReasonCheck)> {
    vec![
        (
            "non-zero exit",
            Spawn::exits(ChildExit::Code(101)),
            |reason| {
                *reason
                    == RollbackReason::Crashed {
                        exit_code: Some(101),
                    }
            },
        ),
        ("signal", Spawn::exits(ChildExit::Terminated), |reason| {
            *reason == RollbackReason::Crashed { exit_code: None }
        }),
        ("health timeout", Spawn::hangs(), |reason| {
            *reason == RollbackReason::NoHealthSignal
        }),
        (
            "spawn failure",
            Spawn::Fails,
            |reason| matches!(reason, RollbackReason::LaunchFailed { detail } if !detail.is_empty()),
        ),
    ]
}

fn reported_database(fixture: &Fixture) -> DatabaseRestore {
    match take_update_report(&fixture.data_dir, FROM).unwrap() {
        Some(UpdateReport::RolledBack { database, .. }) => database,
        other => panic!("expected a rolled-back report, got {other:?}"),
    }
}

#[test]
fn healthy_signals_confirm_the_update() {
    let cases: Vec<(&str, CandidateFor)> = vec![
        ("marker", |fixture| {
            Spawn::reports_healthy_at(fixture.data_dir.clone(), 1)
        }),
        ("clean exit without marker", |_| {
            Spawn::exits(ChildExit::Code(0))
        }),
        ("crash right after the marker", |fixture| {
            let data_dir = fixture.data_dir.clone();
            Spawn::child(move |_| {
                forge_update::write_health_marker(&data_dir, TO).unwrap();
                Some(ChildExit::Code(1))
            })
        }),
    ];
    for (label, candidate) in cases {
        let fixture = restart_fixture();
        let mut host = fixture.host([candidate(&fixture)]);

        let outcome = fixture.supervise(&mut host, Some(SCHEMA + 1)).unwrap();

        assert_eq!(outcome, SupervisorOutcome::Updated, "{label}");
        assert_eq!(fixture.running_build(), NEW_BUILD, "{label}");
    }
}

#[test]
fn the_candidate_is_launched_on_the_new_build_with_the_candidate_flag() {
    let fixture = restart_fixture();
    let mut host = fixture.host([Spawn::reports_healthy_at(fixture.data_dir.clone(), 0)]);

    fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    let launches = host.launches();
    assert_eq!(launches.len(), 1);
    assert_eq!(launches[0].program, fixture.launch_executable());
    assert_eq!(launches[0].args, candidate_args());
    assert_eq!(launches[0].watched.as_deref(), Some(NEW_BUILD));
}

#[test]
fn a_confirmed_update_deletes_the_backup_and_keeps_the_snapshot() {
    let fixture = restart_fixture();
    let mut host = fixture.host([Spawn::reports_healthy_at(fixture.data_dir.clone(), 1)]);

    fixture.supervise(&mut host, Some(SCHEMA + 1)).unwrap();

    assert!(!fixture.previous().exists());
    assert_eq!(std::fs::read(&fixture.snapshot).unwrap(), SNAPSHOT_DB);
    assert_eq!(
        std::fs::read(fixture.data_dir.join(DB_MAIN)).unwrap(),
        LIVE_DB
    );
}

#[test]
fn a_confirmed_update_clears_the_records_and_leaves_no_outcome() {
    let fixture = restart_fixture();
    let mut host = fixture.host([Spawn::reports_healthy_at(fixture.data_dir.clone(), 1)]);

    fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert!(fixture.records_cleared());
    assert_eq!(take_update_report(&fixture.data_dir, TO).unwrap(), None);
}

#[test]
fn a_healthy_candidate_is_left_running() {
    let fixture = restart_fixture();
    let mut host = fixture.host([Spawn::reports_healthy_at(fixture.data_dir.clone(), 2)]);

    fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(host.kills(), 0);
}

#[test]
fn only_the_target_version_sees_an_updated_report_while_supervised() {
    let fixture = restart_fixture();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let data_dir = fixture.data_dir.clone();
    let sink = Arc::clone(&seen);
    let mut host = fixture.host([Spawn::child(move |_| {
        let mut sink = sink.lock().unwrap();
        sink.push(take_update_report(&data_dir, TO).unwrap());
        sink.push(take_update_report(&data_dir, FROM).unwrap());
        Some(ChildExit::Code(0))
    })]);

    fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(
        *seen.lock().unwrap(),
        [
            Some(UpdateReport::Updated {
                previous_version: FROM.to_owned(),
                version: TO.to_owned(),
            }),
            None,
        ]
    );
}

#[test]
fn every_unhealthy_candidate_restores_the_previous_build_byte_identical() {
    for (label, candidate, _) in unhealthy_cases() {
        let fixture = restart_fixture();
        let mut host = fixture.host([candidate]);

        let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

        assert_eq!(outcome, SupervisorOutcome::RolledBack, "{label}");
        assert_eq!(fixture.running_build(), OLD_BUILD, "{label}");
        assert!(!fixture.previous().exists(), "{label}");
    }
}

#[test]
fn every_unhealthy_candidate_reports_rolled_back_with_its_reason() {
    for (label, candidate, reason_matches) in unhealthy_cases() {
        let fixture = restart_fixture();
        let mut host = fixture.host([candidate]);

        fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

        match take_update_report(&fixture.data_dir, FROM).unwrap() {
            Some(UpdateReport::RolledBack {
                failed_version,
                restored_version,
                reason,
                ..
            }) => {
                assert_eq!(failed_version, TO, "{label}");
                assert_eq!(restored_version, FROM, "{label}");
                assert!(reason_matches(&reason), "{label}: {reason:?}");
            }
            other => panic!("{label}: {other:?}"),
        }
    }
}

#[test]
fn every_unhealthy_candidate_relaunches_the_old_build_without_the_candidate_flag() {
    for (label, candidate, _) in unhealthy_cases() {
        let fixture = restart_fixture();
        let mut host = fixture.host([candidate]);

        fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

        let launches = host.launches();
        assert_eq!(launches.len(), 2, "{label}");
        let relaunch = &launches[1];
        assert_eq!(relaunch.program, fixture.launch_executable(), "{label}");
        assert!(relaunch.args.is_empty(), "{label}");
        assert_eq!(relaunch.watched.as_deref(), Some(OLD_BUILD), "{label}");
    }
}

#[test]
fn every_unhealthy_candidate_clears_the_update_records() {
    for (label, candidate, _) in unhealthy_cases() {
        let fixture = restart_fixture();
        let mut host = fixture.host([candidate]);

        fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

        assert!(fixture.records_cleared(), "{label}");
        assert_eq!(
            pending_start_action(&fixture.data_dir, FROM).unwrap(),
            StartAction::Continue,
            "{label}"
        );
    }
}

#[test]
fn the_database_is_restored_from_the_snapshot_when_the_schema_moved_or_is_unknown() {
    for schema in [Some(SCHEMA + 1), None] {
        for (label, candidate, _) in unhealthy_cases() {
            let fixture = restart_fixture();
            let mut host = fixture.host([candidate]);

            fixture.supervise(&mut host, schema).unwrap();

            let main = std::fs::read(fixture.data_dir.join(DB_MAIN)).unwrap();
            assert_eq!(main, SNAPSHOT_DB, "{label} {schema:?}");
            assert!(
                !fixture.data_dir.join(DB_WAL).exists(),
                "{label} {schema:?}"
            );
            assert!(
                !fixture.data_dir.join(DB_SHM).exists(),
                "{label} {schema:?}"
            );
            assert_eq!(
                reported_database(&fixture),
                DatabaseRestore::Restored,
                "{label}"
            );
        }
    }
}

#[test]
fn the_database_is_untouched_when_the_schema_did_not_move() {
    for (label, candidate, _) in unhealthy_cases() {
        let fixture = restart_fixture();
        let mut host = fixture.host([candidate]);

        fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

        let main = std::fs::read(fixture.data_dir.join(DB_MAIN)).unwrap();
        assert_eq!(main, LIVE_DB, "{label}");
        assert!(fixture.data_dir.join(DB_WAL).exists(), "{label}");
        assert!(fixture.data_dir.join(DB_SHM).exists(), "{label}");
        assert_eq!(
            reported_database(&fixture),
            DatabaseRestore::Unchanged,
            "{label}"
        );
    }
}

#[test]
fn a_missing_snapshot_is_reported_and_leaves_the_live_database_intact() {
    let fixture = restart_fixture();
    std::fs::remove_file(&fixture.snapshot).unwrap();
    let mut host = fixture.host([Spawn::exits(ChildExit::Code(3))]);

    fixture.supervise(&mut host, Some(SCHEMA + 1)).unwrap();

    assert!(matches!(
        reported_database(&fixture),
        DatabaseRestore::Failed { .. }
    ));
    assert_eq!(
        std::fs::read(fixture.data_dir.join(DB_MAIN)).unwrap(),
        LIVE_DB
    );
    assert!(fixture.data_dir.join(DB_WAL).exists());
}

#[test]
fn a_candidate_still_running_at_the_deadline_is_killed() {
    let fixture = restart_fixture();
    let mut host = fixture.host([Spawn::hangs()]);

    fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(host.kills(), 1);
}

#[test]
fn the_rollback_fires_exactly_at_the_health_deadline() {
    let fixture = restart_fixture();
    let clock = FakeClock::new();
    let mut host = fixture.host([Spawn::hangs()]);

    fixture
        .supervise_with(&mut host, &clock, Some(SCHEMA))
        .unwrap();

    assert_eq!(clock.elapsed(), settings().health_timeout);
}

#[test]
fn a_marker_on_the_last_poll_before_the_deadline_confirms_the_update() {
    let fixture = restart_fixture();
    let last_poll = u32::try_from(
        (settings().health_timeout - settings().poll_interval).as_secs()
            / settings().poll_interval.as_secs(),
    )
    .unwrap();
    let mut host = fixture.host([Spawn::reports_healthy_at(
        fixture.data_dir.clone(),
        last_poll,
    )]);

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(outcome, SupervisorOutcome::Updated);
}

#[test]
fn a_candidate_that_ignores_the_kill_is_rolled_back_after_the_grace() {
    let fixture = restart_fixture();
    let clock = FakeClock::new();
    let mut host = fixture.host([Spawn::ignores_kill()]);

    let outcome = fixture
        .supervise_with(&mut host, &clock, Some(SCHEMA))
        .unwrap();

    assert_eq!(outcome, SupervisorOutcome::RolledBack);
    assert_eq!(
        clock.elapsed(),
        settings().health_timeout + settings().child_exit_grace
    );
}

#[test]
fn a_marker_for_another_version_does_not_count_as_healthy() {
    let fixture = restart_fixture();
    let data_dir = fixture.data_dir.clone();
    let mut host = fixture.host([Spawn::child(move |_| {
        forge_update::write_health_marker(&data_dir, FROM).unwrap();
        None
    })]);

    fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert!(matches!(
        take_update_report(&fixture.data_dir, FROM).unwrap(),
        Some(UpdateReport::RolledBack {
            reason: RollbackReason::NoHealthSignal,
            ..
        })
    ));
}

#[test]
fn a_marker_left_over_from_before_the_launch_does_not_count_as_healthy() {
    let fixture = restart_fixture();
    std::fs::write(
        forge_update::HealthMarker::path_in(&fixture.data_dir),
        serde_json::to_vec(&forge_update::HealthMarker {
            pid: 1,
            version: TO.to_owned(),
        })
        .unwrap(),
    )
    .unwrap();
    let mut host = fixture.host([Spawn::hangs()]);

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(outcome, SupervisorOutcome::RolledBack);
}

#[test]
fn a_rollback_without_a_backup_reports_failure_and_does_not_relaunch() {
    let fixture = restart_fixture();
    let previous = fixture.previous();
    let mut host = fixture.host([Spawn::child(move |_| {
        std::fs::remove_file(&previous).unwrap();
        Some(ChildExit::Code(101))
    })]);

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(outcome, SupervisorOutcome::RollbackFailed);
    assert_eq!(host.launches().len(), 1);
    assert!(matches!(
        take_update_report(&fixture.data_dir, FROM).unwrap(),
        Some(UpdateReport::RollbackFailed {
            reason: RollbackReason::Crashed {
                exit_code: Some(101)
            },
            ..
        })
    ));
}

#[test]
fn a_second_supervisor_is_refused_while_one_runs() {
    let fixture = restart_fixture();
    let seen = Arc::new(Mutex::new(None));
    let sink = Arc::clone(&seen);
    let data_dir = fixture.data_dir.clone();
    let mut host = fixture.host([Spawn::child(move |_| {
        let mut nested_host = harness::FakeHost::new([]);
        let unit = harness::database_unit();
        let result = forge_update::run_supervisor(forge_update::SupervisorContext {
            data_dir: &data_dir,
            database: &unit,
            settings: settings(),
            host: &mut nested_host,
            clock: &FakeClock::new(),
            schema: &harness::FixedSchema(None),
        });
        *sink.lock().unwrap() = Some(matches!(result, Err(UpdateError::SupervisorBusy)));
        Some(ChildExit::Code(0))
    })]);

    fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(*seen.lock().unwrap(), Some(true));
}

#[test]
fn a_regular_start_defers_to_a_running_supervisor() {
    let fixture = restart_fixture();
    let seen = Arc::new(Mutex::new(None));
    let sink = Arc::clone(&seen);
    let data_dir = fixture.data_dir.clone();
    let mut host = fixture.host([Spawn::child(move |_| {
        *sink.lock().unwrap() = Some(pending_start_action(&data_dir, TO).unwrap());
        Some(ChildExit::Code(0))
    })]);

    fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(*seen.lock().unwrap(), Some(StartAction::SupervisorRunning));
}

#[test]
fn a_finished_rollback_report_is_taken_once() {
    let fixture = restart_fixture();
    let mut host = fixture.host([Spawn::exits(ChildExit::Code(2))]);
    fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    let first = take_update_report(&fixture.data_dir, FROM).unwrap();
    let second = take_update_report(&fixture.data_dir, FROM).unwrap();

    assert!(matches!(first, Some(UpdateReport::RolledBack { .. })));
    assert_eq!(second, None);
}
