use std::path::Path;

use super::harness::Fixture;
use crate::apply_state::ApplyState;
use crate::journal::UpdateJournal;
use crate::{ApplyMode, InstallLayout, update_in_flight};

fn restart_fixture() -> Fixture {
    Fixture::file_layout(InstallLayout::LinuxAppImage, ApplyMode::Restart)
}

fn apply_state_only(bytes: &[u8]) -> tempfile::TempDir {
    let data = tempfile::tempdir().unwrap();
    std::fs::write(ApplyState::path_in(data.path()), bytes).unwrap();
    data
}

#[test]
fn a_data_dir_without_update_records_has_no_update_in_flight() {
    let data = tempfile::tempdir().unwrap();

    assert!(!update_in_flight(data.path()).unwrap());
}

#[test]
fn any_update_record_on_disk_means_an_update_is_in_flight_whatever_it_holds() {
    for (label, bytes) in [("empty", &b""[..]), ("garbage", &[0xff, 0x00, 0xfe][..])] {
        let data = apply_state_only(bytes);

        assert!(update_in_flight(data.path()).unwrap(), "{label}");
    }
}

#[test]
fn each_written_handoff_record_alone_keeps_the_update_in_flight() {
    for (label, cleared) in [
        ("apply state only", UpdateJournal::path_in as fn(&Path) -> _),
        ("journal only", ApplyState::path_in),
    ] {
        let fixture = restart_fixture();
        std::fs::remove_file(cleared(&fixture.data_dir)).unwrap();

        assert!(update_in_flight(&fixture.data_dir).unwrap(), "{label}");
    }
}

#[test]
fn a_full_handoff_plan_on_disk_is_an_update_in_flight() {
    let fixture = restart_fixture();

    assert!(update_in_flight(&fixture.data_dir).unwrap());
}

#[test]
fn a_directory_in_place_of_a_record_counts_as_in_flight() {
    let data = tempfile::tempdir().unwrap();
    std::fs::create_dir(UpdateJournal::path_in(data.path())).unwrap();

    assert!(update_in_flight(data.path()).unwrap());
}

#[cfg(unix)]
#[test]
fn a_dangling_symlink_in_place_of_a_record_counts_as_in_flight() {
    let data = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(
        data.path().join("missing-target"),
        ApplyState::path_in(data.path()),
    )
    .unwrap();

    assert!(update_in_flight(data.path()).unwrap());
}

#[cfg(unix)]
#[test]
fn a_data_dir_path_that_runs_through_a_file_is_an_error_not_a_verdict() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("not-a-dir");
    std::fs::write(&file, b"x").unwrap();

    assert!(update_in_flight(&file.join("data")).is_err());
}
