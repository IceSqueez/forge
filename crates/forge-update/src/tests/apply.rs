use std::ffi::OsString;
use std::path::Path;

#[cfg(unix)]
use super::harness::bundle_binary;
use super::harness::{FROM, Fixture, NEW_BUILD, OLD_BUILD, SCHEMA, Spawn, TO};
use crate::apply_state::ApplyPhase;
use crate::{
    ApplyFailure, ApplyMode, ChildExit, InstallLayout, SupervisorOutcome, UpdateReport,
    take_update_report,
};

fn not_applied_reason(fixture: &Fixture) -> ApplyFailure {
    match take_update_report(&fixture.data_dir, FROM).unwrap() {
        Some(UpdateReport::NotApplied { version, reason }) => {
            assert_eq!(version, TO);
            reason
        }
        other => panic!("expected a not-applied report, got {other:?}"),
    }
}

fn install_dir_names(fixture: &Fixture) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(&fixture.install_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

struct Entry<'a> {
    path: &'a str,
    body: &'a [u8],
    mode: u32,
}

fn archive(files: &[Entry<'_>], symlinks: &[(&str, &Path)]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut builder = tar::Builder::new(encoder);
    for entry in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(entry.body.len() as u64);
        header.set_mode(entry.mode);
        header.set_entry_type(tar::EntryType::Regular);
        builder
            .append_data(&mut header, entry.path, entry.body)
            .unwrap();
    }
    for (path, target) in symlinks {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        builder.append_link(&mut header, path, target).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

fn with_raw_entry(name: &[u8], body: &[u8]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut builder = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_size(NEW_BUILD.len() as u64);
    header.set_mode(0o755);
    builder
        .append_data(&mut header, "Forge.app/Contents/MacOS/forge", NEW_BUILD)
        .unwrap();
    let mut raw = tar::Header::new_gnu();
    raw.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name);
    raw.set_size(body.len() as u64);
    raw.set_mode(0o644);
    raw.set_entry_type(tar::EntryType::Regular);
    raw.set_cksum();
    builder.append(&raw, body).unwrap();
    builder.into_inner().unwrap().finish().unwrap()
}

#[cfg(unix)]
fn valid_bundle() -> Vec<u8> {
    archive(
        &[
            Entry {
                path: "Forge.app/Contents/MacOS/forge",
                body: NEW_BUILD,
                mode: 0o750,
            },
            Entry {
                path: "Forge.app/Contents/Info.plist",
                body: b"<plist/>",
                mode: 0o640,
            },
        ],
        &[("Forge.app/Contents/MacOS/forge-link", Path::new("forge"))],
    )
}

#[test]
fn a_parent_that_never_exits_leaves_the_install_untouched_without_relaunch() {
    let fixture = Fixture::file_layout(InstallLayout::LinuxAppImage, ApplyMode::Restart);
    let mut host = fixture.host([]).parent_never_exits();

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(outcome, SupervisorOutcome::NotApplied);
    assert_eq!(
        not_applied_reason(&fixture),
        ApplyFailure::ParentStillRunning
    );
    assert_eq!(fixture.running_build(), OLD_BUILD);
    assert!(host.launches().is_empty());
    assert!(!fixture.staged.exists());
    assert!(fixture.records_cleared());
}

#[test]
fn a_swap_failing_mid_chain_renames_everything_back() {
    let fixture = Fixture::file_layout(InstallLayout::WindowsPortable, ApplyMode::Restart);
    std::fs::remove_file(&fixture.staged).unwrap();
    let mut host = fixture.host([]);

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(outcome, SupervisorOutcome::NotApplied);
    assert!(matches!(
        not_applied_reason(&fixture),
        ApplyFailure::FileOperation { .. }
    ));
    assert_eq!(fixture.running_build(), OLD_BUILD);
    assert_eq!(install_dir_names(&fixture), ["forge.exe"]);
}

#[test]
fn a_failed_apply_relaunches_the_old_build_only_in_restart_mode() {
    for (mode, expected_launches) in [(ApplyMode::Restart, 1), (ApplyMode::OnQuit, 0)] {
        let fixture = Fixture::file_layout(InstallLayout::WindowsPortable, mode);
        std::fs::remove_file(&fixture.staged).unwrap();
        let mut host = fixture.host([]);

        fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

        let launches = host.launches();
        assert_eq!(launches.len(), expected_launches, "{mode:?}");
        if let Some(relaunch) = launches.first() {
            assert!(relaunch.args.is_empty());
            assert_eq!(relaunch.watched.as_deref(), Some(OLD_BUILD));
        }
    }
}

#[test]
fn a_windows_portable_swap_keeps_the_old_exe_as_the_backup() {
    let fixture = Fixture::file_layout(InstallLayout::WindowsPortable, ApplyMode::OnQuit);
    let mut host = fixture.host([]);

    fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(fixture.running_build(), NEW_BUILD);
    assert_eq!(std::fs::read(fixture.previous()).unwrap(), OLD_BUILD);
    assert!(!fixture.staged.exists());
}

#[cfg(unix)]
#[test]
fn an_appimage_swap_gives_the_new_build_the_old_mode_plus_execute() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::file_layout(InstallLayout::LinuxAppImage, ApplyMode::OnQuit);
    std::fs::set_permissions(&fixture.target, std::fs::Permissions::from_mode(0o640)).unwrap();
    std::fs::set_permissions(&fixture.staged, std::fs::Permissions::from_mode(0o600)).unwrap();
    let mut host = fixture.host([]);

    fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    let mode = std::fs::metadata(&fixture.target)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(fixture.running_build(), NEW_BUILD);
    assert_eq!(mode & 0o7777, 0o751);
}

#[cfg(unix)]
#[test]
fn a_bundle_swap_installs_the_archived_app_with_modes_and_symlinks() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::bundle(ApplyMode::OnQuit, &valid_bundle());
    let mut host = fixture.host([]);

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    let contents = fixture.target.join("Contents");
    let mode_of = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o7777;
    assert_eq!(outcome, SupervisorOutcome::AwaitingFirstStart);
    assert_eq!(fixture.running_build(), NEW_BUILD);
    assert_eq!(mode_of(&bundle_binary(&fixture.target)), 0o750);
    assert_eq!(mode_of(&contents.join("Info.plist")), 0o640);
    assert_eq!(
        std::fs::read_link(contents.join("MacOS").join("forge-link")).unwrap(),
        Path::new("forge")
    );
}

#[cfg(unix)]
#[test]
fn a_bundle_swap_keeps_the_old_bundle_and_cleans_its_scratch() {
    let fixture = Fixture::bundle(ApplyMode::OnQuit, &valid_bundle());
    let mut host = fixture.host([]);

    fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(
        std::fs::read(bundle_binary(&fixture.previous())).unwrap(),
        OLD_BUILD
    );
    assert_eq!(
        install_dir_names(&fixture),
        [".forge-previous-Forge.app", "Forge.app"]
    );
}

#[test]
fn an_archive_without_exactly_one_app_bundle_is_refused() {
    let none = archive(
        &[Entry {
            path: "Contents/MacOS/forge",
            body: NEW_BUILD,
            mode: 0o755,
        }],
        &[],
    );
    let two = archive(
        &[
            Entry {
                path: "Forge.app/Contents/MacOS/forge",
                body: NEW_BUILD,
                mode: 0o755,
            },
            Entry {
                path: "Other.app/Contents/MacOS/other",
                body: NEW_BUILD,
                mode: 0o755,
            },
        ],
        &[],
    );
    for (label, payload) in [("no bundle", none), ("two bundles", two)] {
        let fixture = Fixture::bundle(ApplyMode::Restart, &payload);
        let mut host = fixture.host([]);

        let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

        assert_eq!(outcome, SupervisorOutcome::NotApplied, "{label}");
        assert_eq!(
            not_applied_reason(&fixture),
            ApplyFailure::MalformedPayload,
            "{label}"
        );
        assert_eq!(fixture.running_build(), OLD_BUILD, "{label}");
        assert_eq!(install_dir_names(&fixture), ["Forge.app"], "{label}");
    }
}

#[test]
fn an_archive_with_a_parent_dir_entry_is_refused() {
    let fixture = Fixture::bundle(
        ApplyMode::Restart,
        &with_raw_entry(b"../escaped", b"payload"),
    );
    let mut host = fixture.host([]);

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(outcome, SupervisorOutcome::NotApplied);
    assert_eq!(not_applied_reason(&fixture), ApplyFailure::MalformedPayload);
    assert_eq!(fixture.running_build(), OLD_BUILD);
}

#[cfg(unix)]
#[test]
fn an_archive_writing_through_a_symlink_is_refused() {
    let outside = tempfile::tempdir().unwrap();
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut builder = tar::Builder::new(encoder);
    let mut binary = tar::Header::new_gnu();
    binary.set_size(NEW_BUILD.len() as u64);
    binary.set_mode(0o755);
    builder
        .append_data(&mut binary, "Forge.app/Contents/MacOS/forge", NEW_BUILD)
        .unwrap();
    let mut link = tar::Header::new_gnu();
    link.set_entry_type(tar::EntryType::Symlink);
    link.set_size(0);
    builder
        .append_link(&mut link, "Forge.app/escape", outside.path())
        .unwrap();
    let mut planted = tar::Header::new_gnu();
    planted.set_size(7);
    planted.set_mode(0o644);
    builder
        .append_data(&mut planted, "Forge.app/escape/planted", &b"payload"[..])
        .unwrap();
    let payload = builder.into_inner().unwrap().finish().unwrap();
    let fixture = Fixture::bundle(ApplyMode::Restart, &payload);
    let mut host = fixture.host([]);

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(outcome, SupervisorOutcome::NotApplied);
    assert!(!outside.path().join("planted").exists());
    assert_eq!(fixture.running_build(), OLD_BUILD);
}

#[test]
fn a_parent_dir_entry_never_lands_outside_the_extract_dir() {
    let fixture = Fixture::bundle(
        ApplyMode::OnQuit,
        &with_raw_entry(b"../escaped", b"payload"),
    );
    let mut host = fixture.host([]);

    fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert!(!fixture.install_dir.join("escaped").exists());
    assert!(!fixture.root.path().join("escaped").exists());
}

fn msi_installer(exit: ChildExit, target: &Path) -> Spawn {
    let target = target.to_path_buf();
    Spawn::child(move |_| {
        std::fs::write(&target, NEW_BUILD).unwrap();
        Some(exit)
    })
}

#[test]
fn msi_success_codes_install_and_launch_the_candidate() {
    for code in [0, 3010, 1641] {
        let fixture = Fixture::file_layout(InstallLayout::WindowsPerUserMsi, ApplyMode::Restart);
        let mut host = fixture.host([msi_installer(ChildExit::Code(code), &fixture.target)]);

        let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

        assert_eq!(outcome, SupervisorOutcome::Updated, "{code}");
        assert_eq!(fixture.running_build(), NEW_BUILD, "{code}");
        assert_eq!(host.launches().len(), 2, "{code}");
    }
}

#[test]
fn msi_installer_is_run_quietly_without_restart_on_the_staged_package() {
    let fixture = Fixture::file_layout(InstallLayout::WindowsPerUserMsi, ApplyMode::OnQuit);
    let mut host = fixture.host([msi_installer(ChildExit::Code(0), &fixture.target)]);

    fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    let installer = &host.launches()[0];
    assert_eq!(
        installer.program.file_name().unwrap(),
        OsString::from("msiexec.exe")
    );
    assert_eq!(
        installer.args,
        [
            OsString::from("/i"),
            fixture.staged.clone().into_os_string(),
            OsString::from("/qn"),
            OsString::from("/norestart"),
        ]
    );
}

#[test]
fn msi_failure_exits_are_not_applied_with_the_exit_code() {
    for (exit, expected) in [
        (ChildExit::Code(1603), Some(1603)),
        (ChildExit::Code(1), Some(1)),
        (ChildExit::Code(-1), Some(-1)),
        (ChildExit::Terminated, None),
    ] {
        let fixture = Fixture::file_layout(InstallLayout::WindowsPerUserMsi, ApplyMode::Restart);
        let mut host = fixture.host([Spawn::exits(exit)]);

        let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

        assert_eq!(outcome, SupervisorOutcome::NotApplied, "{exit:?}");
        assert_eq!(
            not_applied_reason(&fixture),
            ApplyFailure::InstallerFailed {
                exit_code: expected
            },
            "{exit:?}"
        );
        assert_eq!(fixture.running_build(), OLD_BUILD, "{exit:?}");
        assert!(!fixture.previous().exists(), "{exit:?}");
    }
}

#[test]
fn an_msi_installer_past_its_timeout_is_killed_and_not_applied() {
    let fixture = Fixture::file_layout(InstallLayout::WindowsPerUserMsi, ApplyMode::Restart);
    let mut host = fixture.host([Spawn::hangs()]);

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(outcome, SupervisorOutcome::NotApplied);
    assert_eq!(host.kills(), 1);
    assert_eq!(
        not_applied_reason(&fixture),
        ApplyFailure::InstallerTimedOut
    );
}

#[test]
fn an_msi_update_whose_candidate_crashes_restores_the_previous_exe() {
    let fixture = Fixture::file_layout(InstallLayout::WindowsPerUserMsi, ApplyMode::Restart);
    let mut host = fixture.host([
        msi_installer(ChildExit::Code(0), &fixture.target),
        Spawn::exits(ChildExit::Code(5)),
    ]);

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(outcome, SupervisorOutcome::RolledBack);
    assert_eq!(fixture.running_build(), OLD_BUILD);
}

#[test]
fn an_on_quit_apply_waits_for_the_first_start_without_launching() {
    let fixture = Fixture::file_layout(InstallLayout::LinuxAppImage, ApplyMode::OnQuit);
    let mut host = fixture.host([]);

    let outcome = fixture.supervise(&mut host, Some(SCHEMA)).unwrap();

    assert_eq!(outcome, SupervisorOutcome::AwaitingFirstStart);
    assert!(host.launches().is_empty());
    assert_eq!(fixture.phase(), Some(ApplyPhase::AwaitingFirstStart));
    assert_eq!(fixture.running_build(), NEW_BUILD);
}
