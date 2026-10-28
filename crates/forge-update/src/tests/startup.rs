use std::ffi::OsString;
use std::path::PathBuf;

use super::harness::{FROM, Fixture, NEW_BUILD, OLD_BUILD, SCHEMA, Spawn, TO, candidate_args};
use crate::apply_state::{ApplyPhase, ApplyState};
use crate::journal::UpdateJournal;
use crate::{
    ApplyFailure, ApplyMode, CANDIDATE_FLAG, ChildExit, HealthMarker, InstallLayout, LaunchRole,
    SUPERVISOR_FLAG, StartAction, SupervisorOutcome, UpdateReport, launch_role,
    pending_start_action, take_update_report, write_health_marker,
};

fn on_quit_applied() -> Fixture {
    let fixture = Fixture::file_layout(InstallLayout::LinuxAppImage, ApplyMode::OnQuit);
    let mut host = fixture.host([]);
    fixture.supervise(&mut host, Some(SCHEMA)).unwrap();
    fixture
}

fn stale_at(phase: ApplyPhase) -> Fixture {
    let fixture = Fixture::file_layout(InstallLayout::LinuxAppImage, ApplyMode::Restart);
    set_phase(&fixture, phase);
    fixture
}

fn set_phase(fixture: &Fixture, phase: ApplyPhase) {
    let mut state = fixture.state.clone();
    state.phase = phase;
    state.write(&fixture.data_dir).unwrap();
}

#[test]
fn the_first_start_after_an_on_quit_apply_hands_off() {
    let fixture = on_quit_applied();

    let action = pending_start_action(&fixture.data_dir, TO).unwrap();

    assert_eq!(action, StartAction::HandOff);
}

#[test]
fn the_handed_off_supervisor_waits_for_the_starter_then_supervises_the_candidate() {
    let fixture = on_quit_applied();
    pending_start_action(&fixture.data_dir, TO).unwrap();
    let mut host = fixture.host([Spawn::reports_healthy_at(fixture.data_dir.clone(), 1)]);

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(outcome, SupervisorOutcome::Updated);
    assert_eq!(host.parent_waits(), 1);
    assert_eq!(host.launches()[0].args, candidate_args());
    assert!(!fixture.previous().exists());
    assert!(fixture.records_cleared());
}

#[test]
fn a_crash_on_the_first_start_after_an_on_quit_apply_rolls_back() {
    let fixture = on_quit_applied();
    pending_start_action(&fixture.data_dir, TO).unwrap();
    let mut host = fixture.host([Spawn::exits(ChildExit::Code(134))]);

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(outcome, SupervisorOutcome::RolledBack);
    assert_eq!(fixture.running_build(), OLD_BUILD);
    assert_eq!(host.launches().last().unwrap().args, Vec::<OsString>::new());
}

#[test]
fn a_handed_off_supervisor_whose_starter_never_exits_launches_nothing() {
    let fixture = on_quit_applied();
    let mut host = fixture.host([]).parent_never_exits();

    let result = fixture.supervise(&mut host, Some(SCHEMA));

    assert!(matches!(
        result,
        Err(crate::UpdateError::ParentStillRunning)
    ));
    assert!(host.launches().is_empty());
    assert_eq!(fixture.running_build(), NEW_BUILD);
}

#[test]
fn a_stale_update_on_the_target_version_hands_off_for_supervision() {
    for phase in [
        ApplyPhase::HandedOff,
        ApplyPhase::Applying,
        ApplyPhase::AwaitingHealth,
    ] {
        let fixture = stale_at(phase);

        let action = pending_start_action(&fixture.data_dir, TO).unwrap();

        assert_eq!(action, StartAction::HandOff, "{phase:?}");
        assert_eq!(
            fixture.phase(),
            Some(ApplyPhase::AwaitingFirstStart),
            "{phase:?}"
        );
    }
}

#[test]
fn a_stale_update_on_another_version_is_reported_interrupted() {
    let fixture = stale_at(ApplyPhase::Applying);

    let action = pending_start_action(&fixture.data_dir, FROM).unwrap();

    assert_eq!(action, StartAction::Continue);
    assert_eq!(
        take_update_report(&fixture.data_dir, FROM).unwrap(),
        Some(UpdateReport::NotApplied {
            version: TO.to_owned(),
            reason: ApplyFailure::Interrupted,
        })
    );
}

#[test]
fn an_interrupted_update_drops_the_staged_file_and_the_records() {
    let fixture = stale_at(ApplyPhase::Applying);

    pending_start_action(&fixture.data_dir, FROM).unwrap();

    assert!(!fixture.staged.exists());
    assert!(fixture.records_cleared());
}

#[test]
fn an_interrupted_swap_puts_the_previous_build_back() {
    let fixture = stale_at(ApplyPhase::Applying);
    std::fs::rename(&fixture.target, fixture.previous()).unwrap();

    pending_start_action(&fixture.data_dir, FROM).unwrap();

    assert_eq!(fixture.running_build(), OLD_BUILD);
    assert!(!fixture.previous().exists());
}

#[test]
fn half_written_records_are_cleared_as_debris() {
    for orphan in ["journal", "state"] {
        let fixture = Fixture::file_layout(InstallLayout::LinuxAppImage, ApplyMode::Restart);
        match orphan {
            "journal" => ApplyState::clear(&fixture.data_dir).unwrap(),
            _ => UpdateJournal::clear_blocking(&fixture.data_dir).unwrap(),
        }

        let action = pending_start_action(&fixture.data_dir, FROM).unwrap();

        assert_eq!(action, StartAction::Continue, "{orphan}");
        assert!(fixture.records_cleared(), "{orphan}");
        assert_eq!(fixture.running_build(), OLD_BUILD, "{orphan}");
    }
}

#[test]
fn a_supervisor_with_half_written_records_has_nothing_to_do() {
    let fixture = Fixture::file_layout(InstallLayout::LinuxAppImage, ApplyMode::Restart);
    ApplyState::clear(&fixture.data_dir).unwrap();
    let mut host = fixture.host([]);

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(outcome, SupervisorOutcome::NothingPending);
    assert!(host.launches().is_empty());
    assert!(fixture.records_cleared());
}

#[test]
fn a_start_without_records_continues() {
    let data = tempfile::tempdir().unwrap();

    let action = pending_start_action(data.path(), TO).unwrap();

    assert_eq!(action, StartAction::Continue);
}

#[test]
fn the_supervisor_lock_is_released_when_the_supervisor_finishes() {
    let fixture = on_quit_applied();

    let action = pending_start_action(&fixture.data_dir, TO).unwrap();

    assert_ne!(action, StartAction::SupervisorRunning);
}

#[test]
fn the_health_marker_is_written_only_while_health_is_awaited() {
    for (phase, expected) in [
        (None, false),
        (Some(ApplyPhase::HandedOff), false),
        (Some(ApplyPhase::Applying), false),
        (Some(ApplyPhase::AwaitingFirstStart), false),
        (Some(ApplyPhase::AwaitingHealth), true),
    ] {
        let fixture = Fixture::file_layout(InstallLayout::LinuxAppImage, ApplyMode::Restart);
        match phase {
            Some(phase) => set_phase(&fixture, phase),
            None => ApplyState::clear(&fixture.data_dir).unwrap(),
        }

        let written = write_health_marker(&fixture.data_dir, TO).unwrap();

        assert_eq!(written, expected, "{phase:?}");
        assert_eq!(
            HealthMarker::path_in(&fixture.data_dir).exists(),
            expected,
            "{phase:?}"
        );
    }
}

#[test]
fn a_written_health_marker_names_the_running_version_and_process() {
    let fixture = Fixture::file_layout(InstallLayout::LinuxAppImage, ApplyMode::Restart);
    set_phase(&fixture, ApplyPhase::AwaitingHealth);

    write_health_marker(&fixture.data_dir, TO).unwrap();

    assert_eq!(
        crate::read_health_marker(&fixture.data_dir).unwrap(),
        Some(HealthMarker {
            pid: std::process::id(),
            version: TO.to_owned(),
        })
    );
}

#[test]
fn a_regular_start_without_a_supervisor_gets_no_updated_report() {
    let fixture = Fixture::file_layout(InstallLayout::LinuxAppImage, ApplyMode::Restart);
    set_phase(&fixture, ApplyPhase::AwaitingHealth);

    assert_eq!(take_update_report(&fixture.data_dir, TO).unwrap(), None);
}

#[test]
fn launch_role_reads_only_the_first_argument() {
    let data_dir = PathBuf::from("/data/Forge ü");
    let cases: Vec<(Vec<OsString>, LaunchRole)> = vec![
        (
            vec![SUPERVISOR_FLAG.into(), data_dir.clone().into()],
            LaunchRole::Supervisor {
                data_dir: data_dir.clone(),
            },
        ),
        (vec![CANDIDATE_FLAG.into()], LaunchRole::Candidate),
        (
            vec![CANDIDATE_FLAG.into(), SUPERVISOR_FLAG.into()],
            LaunchRole::Candidate,
        ),
        (vec![], LaunchRole::Regular),
        (
            vec!["--verbose".into(), CANDIDATE_FLAG.into()],
            LaunchRole::Regular,
        ),
        (
            vec!["--verbose".into(), SUPERVISOR_FLAG.into(), data_dir.into()],
            LaunchRole::Regular,
        ),
    ];
    for (args, expected) in cases {
        assert_eq!(launch_role(args.clone()), expected, "{args:?}");
    }
}

#[test]
fn launch_role_treats_unknown_or_incomplete_flags_as_regular() {
    for args in [
        vec![OsString::from(SUPERVISOR_FLAG)],
        vec![OsString::from(format!("{CANDIDATE_FLAG}=1"))],
        vec![OsString::from("--forge-update")],
        vec![OsString::from("")],
    ] {
        assert_eq!(launch_role(args.clone()), LaunchRole::Regular, "{args:?}");
    }
}
