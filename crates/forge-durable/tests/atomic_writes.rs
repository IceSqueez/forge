#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::fs;
use std::io::{self, Write as _};
use std::path::Path;
use std::sync::{Arc, Barrier};

use forge_durable::{
    copy_atomically, discard_leftovers_as_sole_writer, replace_atomically, write_atomically,
};
use tempfile::TempDir;

const TARGET: &str = "state.json";
const WRITERS: u8 = 8;
const PAYLOAD_LEN: usize = 256 * 1024;

fn names_in(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn observed_staged_name(target: &Path) -> String {
    let directory = target.parent().unwrap().to_owned();
    let mut staged = None;
    replace_atomically(target, |_| {
        staged = names_in(&directory)
            .into_iter()
            .find(|name| !target.ends_with(name));
        Ok(())
    })
    .unwrap();
    staged.expect("the fill runs while the staged sibling exists")
}

#[test]
fn a_write_creates_missing_directories_and_replaces_what_was_there() {
    let home = TempDir::new().unwrap();
    let target = home.path().join("deep").join("er").join(TARGET);

    write_atomically(&target, b"first").unwrap();
    write_atomically(&target, b"second").unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"second");
    assert_eq!(names_in(target.parent().unwrap()), [TARGET]);
}

#[test]
fn a_failed_fill_returns_its_error_and_leaves_only_the_previous_file() {
    let home = TempDir::new().unwrap();
    let target = home.path().join(TARGET);
    write_atomically(&target, b"previous").unwrap();

    let failure = replace_atomically(&target, |file| {
        file.write_all(b"half of the ne")?;
        Err(io::Error::from(io::ErrorKind::StorageFull))
    })
    .unwrap_err();

    assert_eq!(failure.kind(), io::ErrorKind::StorageFull);
    assert_eq!(fs::read(&target).unwrap(), b"previous");
    assert_eq!(names_in(home.path()), [TARGET]);
}

#[test]
fn a_failed_fill_on_a_new_target_leaves_an_empty_directory() {
    let home = TempDir::new().unwrap();

    replace_atomically(&home.path().join(TARGET), |_| {
        Err(io::Error::from(io::ErrorKind::Interrupted))
    })
    .unwrap_err();

    assert!(names_in(home.path()).is_empty());
}

#[test]
fn a_rename_refused_by_a_directory_on_the_target_leaves_no_staged_file() {
    let home = TempDir::new().unwrap();
    let target = home.path().join(TARGET);
    fs::create_dir(&target).unwrap();

    write_atomically(&target, b"body").unwrap_err();

    assert_eq!(names_in(home.path()), [TARGET]);
    assert!(target.is_dir());
}

#[test]
fn a_path_without_a_file_name_is_refused() {
    let home = TempDir::new().unwrap();
    let no_name = home.path().join("..");

    for failure in [
        write_atomically(&no_name, b"body").unwrap_err(),
        discard_leftovers_as_sole_writer(&no_name).unwrap_err(),
    ] {
        assert_eq!(failure.kind(), io::ErrorKind::InvalidInput);
    }
}

#[test]
fn copying_publishes_the_source_bytes_and_keeps_the_source() {
    let home = TempDir::new().unwrap();
    let source = home.path().join("source.db");
    let destination = home.path().join("copies").join("copy.db");
    fs::write(&source, b"\x00\xffbytes").unwrap();

    copy_atomically(&source, &destination).unwrap();

    assert_eq!(fs::read(&destination).unwrap(), b"\x00\xffbytes");
    assert_eq!(fs::read(&source).unwrap(), b"\x00\xffbytes");
}

#[test]
fn copying_a_missing_source_fails_and_leaves_the_destination_alone() {
    let home = TempDir::new().unwrap();
    let destination = home.path().join(TARGET);
    fs::write(&destination, b"previous").unwrap();

    let failure = copy_atomically(&home.path().join("absent"), &destination).unwrap_err();

    assert_eq!(failure.kind(), io::ErrorKind::NotFound);
    assert_eq!(fs::read(&destination).unwrap(), b"previous");
    assert_eq!(names_in(home.path()), [TARGET]);
}

#[test]
fn concurrent_writers_to_one_path_each_succeed_and_a_whole_payload_wins() {
    let home = TempDir::new().unwrap();
    let target = Arc::new(home.path().join(TARGET));
    let start = Arc::new(Barrier::new(usize::from(WRITERS)));

    let writers: Vec<_> = (0..WRITERS)
        .map(|writer| {
            let (target, start) = (Arc::clone(&target), Arc::clone(&start));
            std::thread::spawn(move || {
                start.wait();
                write_atomically(&target, &vec![writer; PAYLOAD_LEN])
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap().unwrap();
    }

    let landed = fs::read(target.as_ref()).unwrap();
    assert_eq!(landed.len(), PAYLOAD_LEN);
    assert!(landed.iter().all(|byte| *byte == landed[0]), "torn write");
    assert_eq!(names_in(home.path()), [TARGET]);
}

#[test]
fn a_fill_abandoned_by_a_crash_is_recognised_and_discarded_by_the_same_name() {
    let home = TempDir::new().unwrap();
    let target = home.path().join(TARGET);
    let staged = observed_staged_name(&target);
    fs::write(home.path().join(&staged), b"partial").unwrap();

    discard_leftovers_as_sole_writer(&target).unwrap();

    assert_eq!(names_in(home.path()), [TARGET]);
}

#[test]
fn discarding_leftovers_matches_only_this_files_exact_staged_names() {
    let home = TempDir::new().unwrap();
    let kept = [
        TARGET,
        ".state.json",
        ".state.json.tmp",
        ".state.json.123.tmp",
        ".state.json..7.tmp",
        ".state.json.123..tmp",
        ".state.json.x.7.tmp",
        ".state.json.123.x.tmp",
        ".state.json.123.7.8.tmp",
        ".state.json.123.7.tmpx",
        ".state.json.-1.7.tmp",
        ".state.json.bak.123.7.tmp",
        ".state.jsonx.123.7.tmp",
        ".xstate.json.123.7.tmp",
        "state.json.123.7.tmp",
        "..state.json.123.7.tmp",
        ".other.json.123.7.tmp",
    ];
    for name in kept {
        fs::write(home.path().join(name), b"keep").unwrap();
    }
    for leftover in [".state.json.123.7.tmp", ".state.json.0.0.tmp"] {
        fs::write(home.path().join(leftover), b"partial").unwrap();
    }

    discard_leftovers_as_sole_writer(&home.path().join(TARGET)).unwrap();

    let mut expected = kept.map(str::to_owned).to_vec();
    expected.sort();
    assert_eq!(names_in(home.path()), expected);
}

#[test]
fn discarding_leftovers_handles_a_name_outside_ascii() {
    let home = TempDir::new().unwrap();
    let target = home.path().join("сцена 1.json");
    fs::write(home.path().join(".сцена 1.json.9.3.tmp"), b"partial").unwrap();

    discard_leftovers_as_sole_writer(&target).unwrap();

    assert!(names_in(home.path()).is_empty());
}

#[test]
fn discarding_in_a_directory_that_does_not_exist_is_a_no_op() {
    let home = TempDir::new().unwrap();

    discard_leftovers_as_sole_writer(&home.path().join("absent").join(TARGET)).unwrap();

    assert!(names_in(home.path()).is_empty());
}

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::{PermissionsExt as _, symlink};

    use forge_durable::{create_private, sync_dir, sync_parent};

    use super::*;

    const OWNER_ONLY: u32 = 0o600;
    const PERMISSION_BITS: u32 = 0o777;

    fn mode_of(path: &Path) -> u32 {
        fs::symlink_metadata(path).unwrap().permissions().mode() & PERMISSION_BITS
    }

    #[test]
    fn a_published_file_is_owner_only_even_over_a_wider_file() {
        let home = TempDir::new().unwrap();
        let fresh = home.path().join("fresh");
        let widened = home.path().join("widened");
        fs::write(&widened, b"old").unwrap();
        fs::set_permissions(&widened, fs::Permissions::from_mode(0o644)).unwrap();

        for target in [&fresh, &widened] {
            write_atomically(target, b"body").unwrap();

            assert_eq!(mode_of(target), OWNER_ONLY, "{target:?}");
        }
    }

    #[test]
    fn the_staged_file_is_owner_only_while_it_is_being_filled() {
        let home = TempDir::new().unwrap();
        let target = home.path().join(TARGET);
        let mut staged_mode = None;

        replace_atomically(&target, |file| {
            staged_mode = Some(file.metadata()?.permissions().mode() & PERMISSION_BITS);
            Ok(())
        })
        .unwrap();

        assert_eq!(staged_mode, Some(OWNER_ONLY));
    }

    #[test]
    fn create_private_refuses_a_planted_link_and_leaves_its_target_alone() {
        let home = TempDir::new().unwrap();
        let victim = home.path().join("victim");
        fs::write(&victim, b"victim content").unwrap();
        let live_link = home.path().join("live");
        let dangling_link = home.path().join("dangling");
        symlink(&victim, &live_link).unwrap();
        symlink(home.path().join("absent"), &dangling_link).unwrap();

        for link in [&live_link, &dangling_link] {
            let refused = create_private(link).unwrap_err();

            assert_eq!(refused.kind(), io::ErrorKind::AlreadyExists, "{link:?}");
        }
        assert_eq!(fs::read(&victim).unwrap(), b"victim content");
        assert!(!home.path().join("absent").exists());
    }

    #[test]
    fn a_link_on_the_target_is_replaced_by_the_file_and_never_written_through() {
        let home = TempDir::new().unwrap();
        let victim = home.path().join("victim");
        fs::write(&victim, b"victim content").unwrap();
        let target = home.path().join(TARGET);
        symlink(&victim, &target).unwrap();

        write_atomically(&target, b"body").unwrap();

        assert_eq!(fs::read(&victim).unwrap(), b"victim content");
        assert!(fs::symlink_metadata(&target).unwrap().file_type().is_file());
    }

    #[test]
    fn discarding_a_leftover_that_is_a_link_removes_the_link_and_not_its_target() {
        let home = TempDir::new().unwrap();
        let victim = home.path().join("victim");
        fs::write(&victim, b"victim content").unwrap();
        symlink(&victim, home.path().join(".state.json.1.2.tmp")).unwrap();

        discard_leftovers_as_sole_writer(&home.path().join(TARGET)).unwrap();

        assert_eq!(names_in(home.path()), ["victim"]);
        assert_eq!(fs::read(&victim).unwrap(), b"victim content");
    }

    #[test]
    fn syncing_a_missing_directory_reports_it() {
        let home = TempDir::new().unwrap();
        let absent = home.path().join("absent");

        for failure in [
            sync_dir(&absent).unwrap_err(),
            sync_parent(&absent.join(TARGET)).unwrap_err(),
        ] {
            assert_eq!(failure.kind(), io::ErrorKind::NotFound);
        }
    }
}
