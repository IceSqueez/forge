#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use forge_overlay::BEHAVIOR_FILE;
use forge_overlay::{
    CONFIG_FILE, MARKUP_FILE, OVERRIDABLE_FILES, OverlayError, RESERVED_DIRECTORY, RUNTIME_ASSET,
    STYLE_FILE, read_overlay_source, write_overlay_source,
};
use tempfile::TempDir;

const IDENTITY: &str = "sub-alert-1";

const UNSAFE_IDENTITIES: &[(&str, &str)] = &[
    ("..", "parent traversal"),
    (".", "the current directory"),
    ("a/b", "a nested path"),
    ("/abs", "an absolute path"),
    ("a\\b", "a windows separator"),
    ("Up-Case", "an uppercase letter"),
    ("dot.name", "a dot segment"),
    ("with space", "a space"),
    ("", "an empty name"),
    ("-leading-dash", "a leading dash"),
    (RESERVED_DIRECTORY, "the reserved shared subtree"),
];

const NOT_HANDED_OVER: &[(&str, &str)] = &[
    (CONFIG_FILE, "generated configuration, never the user's"),
    (RUNTIME_ASSET, "the shared runtime the page loads"),
    ("../../etc/passwd", "an escape out of the overlay directory"),
    ("..", "parent traversal"),
    ("sub/index.html", "a nested path"),
    ("index.html.tmp", "a staging file"),
    (".index.html.tmp", "the staging file of a real page"),
    ("Index.html", "a case that is not the shipped name"),
    ("", "an empty name"),
    ("overlay.scss", "a file this build never generates"),
];

fn unborn_root(home: &TempDir) -> PathBuf {
    home.path().join("overlays")
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("the directory exists")
        .map(|entry| {
            entry
                .expect("a readable directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn a_written_page_file_reads_back_byte_for_byte_and_leaves_no_staging_file_behind() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    let body = "\u{feff}<div class=\"stage\">\r\n  \u{433}\u{43e}\u{43b}\u{43e}\u{432}\u{43d}\u{435} \u{1f389}\r\n</div>\n";

    for name in OVERRIDABLE_FILES {
        write_overlay_source(&root, IDENTITY, name, body)
            .expect("a page file lands in a root that does not exist yet");

        assert_eq!(
            read_overlay_source(&root, IDENTITY, name).unwrap(),
            Some(body.to_owned()),
            "{name} did not come back exactly as it was handed over"
        );
    }

    assert_eq!(
        names_in(&root.canonicalize().unwrap().join(IDENTITY)),
        {
            let mut expected: Vec<String> =
                OVERRIDABLE_FILES.iter().map(|n| (*n).to_owned()).collect();
            expected.sort();
            expected
        },
        "the overlay directory holds staging leftovers alongside the page"
    );
}

#[test]
fn rewriting_a_page_file_replaces_the_whole_body_rather_than_its_leading_bytes() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);

    write_overlay_source(&root, IDENTITY, STYLE_FILE, &"/* long */\n".repeat(64)).unwrap();
    write_overlay_source(&root, IDENTITY, STYLE_FILE, "#stage{}").unwrap();

    assert_eq!(
        read_overlay_source(&root, IDENTITY, STYLE_FILE).unwrap(),
        Some("#stage{}".to_owned()),
        "a shorter body left a tail of the previous one on disk"
    );
}

#[test]
fn a_page_that_was_never_written_reports_absence_instead_of_failing() {
    let home = TempDir::new().unwrap();

    let unborn = unborn_root(&home);
    assert_eq!(
        read_overlay_source(&unborn, IDENTITY, MARKUP_FILE).unwrap(),
        None,
        "a root that was never created must not read as a failure"
    );

    let root = home.path().join("with-root");
    fs::create_dir_all(&root).unwrap();
    assert_eq!(
        read_overlay_source(&root, IDENTITY, MARKUP_FILE).unwrap(),
        None,
        "an overlay that owns no directory yet must not read as a failure"
    );

    write_overlay_source(&root, IDENTITY, MARKUP_FILE, "<div/>").unwrap();
    assert_eq!(
        read_overlay_source(&root, IDENTITY, STYLE_FILE).unwrap(),
        None,
        "a page file the user never claimed must not read as a failure"
    );
}

#[test]
fn a_name_outside_the_overridable_set_is_refused_by_both_read_and_write() {
    for &(name, label) in NOT_HANDED_OVER {
        let home = TempDir::new().unwrap();
        let root = unborn_root(&home);

        let read_err = read_overlay_source(&root, IDENTITY, name)
            .expect_err("only the three generated page files can be handed to their owner");
        assert!(
            matches!(&read_err, OverlayError::NotOverridable(got) if got == name),
            "reading {label} ({name:?}) produced {read_err:?}"
        );

        let write_err = write_overlay_source(&root, IDENTITY, name, "payload")
            .expect_err("only the three generated page files can be handed to their owner");
        assert!(
            matches!(&write_err, OverlayError::NotOverridable(got) if got == name),
            "writing {label} ({name:?}) produced {write_err:?}"
        );
        assert!(
            !root.exists(),
            "writing {label} ({name:?}) touched the filesystem before refusing"
        );
    }
}

#[test]
fn an_identity_that_is_not_a_plain_directory_name_is_refused_before_anything_is_created() {
    for &(id, label) in UNSAFE_IDENTITIES {
        let home = TempDir::new().unwrap();
        let root = unborn_root(&home);

        let read_err = read_overlay_source(&root, id, MARKUP_FILE)
            .expect_err("an identity is a directory name and must be refused when it is not");
        assert!(
            matches!(&read_err, OverlayError::UnsafeIdentity(got) if got == id),
            "reading under {label} ({id:?}) produced {read_err:?}"
        );

        let write_err = write_overlay_source(&root, id, MARKUP_FILE, "payload")
            .expect_err("an identity is a directory name and must be refused when it is not");
        assert!(
            matches!(&write_err, OverlayError::UnsafeIdentity(got) if got == id),
            "writing under {label} ({id:?}) produced {write_err:?}"
        );
        assert!(
            !root.exists(),
            "writing under {label} ({id:?}) touched the filesystem before refusing"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_page_file_that_is_a_symbolic_link_is_refused_and_its_target_is_left_alone() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    let outside = home.path().join("victim.css");
    fs::write(&outside, "victim content").unwrap();

    write_overlay_source(&root, IDENTITY, MARKUP_FILE, "<div/>").unwrap();
    let directory = root.canonicalize().unwrap().join(IDENTITY);
    std::os::unix::fs::symlink(&outside, directory.join(STYLE_FILE)).unwrap();

    let read_result = read_overlay_source(&root, IDENTITY, STYLE_FILE);
    assert!(
        matches!(&read_result, Err(OverlayError::SymlinkedPath { .. })),
        "a linked page file must be refused, never followed: {read_result:?}"
    );

    let write_result = write_overlay_source(&root, IDENTITY, STYLE_FILE, "#stage{}");
    assert!(
        matches!(&write_result, Err(OverlayError::SymlinkedPath { .. })),
        "a linked page file must be refused, never written through: {write_result:?}"
    );
    assert_eq!(
        fs::read_to_string(&outside).unwrap(),
        "victim content",
        "the user body was written through the link and out of the overlay root"
    );
}

#[cfg(unix)]
#[test]
fn an_overlay_directory_that_is_a_symbolic_link_is_refused_by_both_read_and_write() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    fs::create_dir_all(&root).unwrap();
    let elsewhere = home.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(elsewhere.join(BEHAVIOR_FILE), "victim content").unwrap();
    std::os::unix::fs::symlink(&elsewhere, root.join(IDENTITY)).unwrap();

    let read_result = read_overlay_source(&root, IDENTITY, BEHAVIOR_FILE);
    assert!(
        matches!(&read_result, Err(OverlayError::SymlinkedPath { .. })),
        "a linked overlay directory must be refused, never read through: {read_result:?}"
    );

    let write_result = write_overlay_source(&root, IDENTITY, BEHAVIOR_FILE, "export {}");
    assert!(
        matches!(&write_result, Err(OverlayError::SymlinkedPath { .. })),
        "a linked overlay directory must be refused, never written through: {write_result:?}"
    );
    assert_eq!(
        fs::read_to_string(elsewhere.join(BEHAVIOR_FILE)).unwrap(),
        "victim content",
        "the write followed a link out of the overlay root"
    );
}
