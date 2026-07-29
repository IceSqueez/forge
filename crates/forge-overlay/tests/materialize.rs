#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use forge_overlay::{
    BEHAVIOR_FILE, CONFIG_FILE, MARKUP_FILE, OverlayConfig, OverlayError, OverlayInstance,
    OverlayKindRegistry, RESERVED_DIRECTORY, RUNTIME_ASSET, STYLE_FILE, ensure_shared_directory,
    materialize_overlay, register_builtin_kinds, remove_overlay_directory,
};
use forge_types::Variant;
use tempfile::TempDir;

const ALERT_KIND: &str = "overlay.alert";
const IDENTITY: &str = "sub-alert-1";
const IDENTITY_LEN_LIMIT: usize = 64;

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

fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

fn instance(source_overrides: &[&str]) -> OverlayInstance {
    OverlayInstance {
        id: IDENTITY.to_owned(),
        display_name: "Sub alert".to_owned(),
        kind_id: ALERT_KIND.to_owned(),
        config: OverlayConfig::new(),
        source_overrides: source_overrides.iter().map(|n| (*n).to_owned()).collect(),
        credential: None,
    }
}

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

fn sorted(values: &[String]) -> Vec<String> {
    let mut owned = values.to_vec();
    owned.sort();
    owned
}

fn generated_page() -> Vec<String> {
    sorted(&[
        MARKUP_FILE.to_owned(),
        STYLE_FILE.to_owned(),
        BEHAVIOR_FILE.to_owned(),
        CONFIG_FILE.to_owned(),
    ])
}

#[test]
fn materializing_writes_the_whole_page_into_the_identity_directory_and_leaves_no_staging_files() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);

    let report = materialize_overlay(&root, &instance(&[]), &registry())
        .expect("a fresh overlay materializes into a root that does not exist yet");

    assert_eq!(
        report.directory,
        root.canonicalize().unwrap().join(IDENTITY),
        "an overlay owns a directory named for its identity, not its display name"
    );
    assert_eq!(
        names_in(&report.directory),
        generated_page(),
        "the directory must hold exactly the page and its config, with no staging leftovers"
    );
    assert_eq!(sorted(&report.written), generated_page());
    assert!(report.preserved.is_empty());
    assert!(report.missing_overrides.is_empty());
}

#[test]
fn each_generated_file_receives_the_body_its_name_promises() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    let reg = registry();
    let assets = reg
        .get(ALERT_KIND)
        .expect("the alert kind ships in this build")
        .page_assets();

    let report = materialize_overlay(&root, &instance(&[]), &reg).expect("materialize");

    for (name, body) in [
        (MARKUP_FILE, assets.markup),
        (STYLE_FILE, assets.style),
        (BEHAVIOR_FILE, assets.behavior),
    ] {
        assert_eq!(
            fs::read_to_string(report.directory.join(name)).unwrap(),
            body,
            "{name} did not receive the asset it is named for"
        );
    }
}

#[test]
fn an_overridden_file_survives_every_regeneration_byte_for_byte() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    let reg = registry();

    let first = materialize_overlay(&root, &instance(&[]), &reg).expect("first materialize");
    let owned = first.directory.join(STYLE_FILE);
    let user_body = b"#stage { opacity: 0.5; }\n\xc2\xa0/* kept */\n";
    fs::write(&owned, user_body).unwrap();

    for headline in ["Changed once", "Changed twice"] {
        let mut claimed = instance(&[STYLE_FILE]);
        claimed.config.insert(
            forge_overlay::config::HEADLINE.to_owned(),
            Variant::String(headline.to_owned()),
        );

        let report = materialize_overlay(&root, &claimed, &reg).expect("regenerate");

        assert_eq!(
            fs::read(&owned).unwrap(),
            user_body,
            "regenerating after a config change rewrote a file the user owns"
        );
        assert_eq!(report.preserved, vec![STYLE_FILE.to_owned()]);
        assert!(
            !report.written.contains(&STYLE_FILE.to_owned()),
            "an overridden file must never be reported as written"
        );
        assert!(report.missing_overrides.is_empty());
    }
}

#[test]
fn the_config_document_is_rewritten_even_when_every_source_file_is_overridden() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    let reg = registry();

    let first = materialize_overlay(&root, &instance(&[]), &reg).expect("first materialize");
    fs::write(first.directory.join(CONFIG_FILE), "stale document").unwrap();
    for name in [MARKUP_FILE, STYLE_FILE, BEHAVIOR_FILE] {
        fs::write(first.directory.join(name), format!("user body for {name}")).unwrap();
    }

    let report = materialize_overlay(
        &root,
        &instance(&[MARKUP_FILE, STYLE_FILE, BEHAVIOR_FILE]),
        &reg,
    )
    .expect("regenerate with every source file owned by the user");

    assert_eq!(
        report.written,
        vec![CONFIG_FILE.to_owned()],
        "the config document is data and can never be user owned"
    );
    assert_ne!(
        fs::read_to_string(first.directory.join(CONFIG_FILE)).unwrap(),
        "stale document",
        "a fully overridden overlay still needs fresh configuration"
    );
    for name in [MARKUP_FILE, STYLE_FILE, BEHAVIOR_FILE] {
        assert_eq!(
            fs::read_to_string(first.directory.join(name)).unwrap(),
            format!("user body for {name}"),
            "{name} was rewritten despite being overridden"
        );
    }
}

#[test]
fn an_override_whose_file_is_gone_is_reported_and_never_recreated() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    let reg = registry();

    let first = materialize_overlay(&root, &instance(&[]), &reg).expect("first materialize");
    fs::remove_file(first.directory.join(BEHAVIOR_FILE)).unwrap();

    let report = materialize_overlay(&root, &instance(&[BEHAVIOR_FILE]), &reg).expect("regenerate");

    assert_eq!(report.missing_overrides, vec![BEHAVIOR_FILE.to_owned()]);
    assert_eq!(report.preserved, vec![BEHAVIOR_FILE.to_owned()]);
    assert!(
        !first.directory.join(BEHAVIOR_FILE).exists(),
        "a missing override must be surfaced, not silently filled in from the shipped asset"
    );
}

#[test]
fn override_names_outside_the_overridable_set_are_ignored() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);

    let report = materialize_overlay(
        &root,
        &instance(&[
            CONFIG_FILE,
            "overlay.CSS",
            "../../etc/passwd",
            "styles.css",
            "",
        ]),
        &registry(),
    )
    .expect("unrecognised override names must not fail materialization");

    assert!(
        report.preserved.is_empty(),
        "only the three generated source files can be claimed by the user"
    );
    assert_eq!(sorted(&report.written), generated_page());
}

#[test]
fn an_identity_that_is_not_a_plain_directory_name_is_rejected_before_anything_is_created() {
    let reg = registry();

    for &(id, label) in UNSAFE_IDENTITIES {
        let home = TempDir::new().unwrap();
        let root = unborn_root(&home);
        let mut unsafe_identity = instance(&[]);
        unsafe_identity.id = id.to_owned();

        let err = materialize_overlay(&root, &unsafe_identity, &reg)
            .expect_err("an identity is a directory name and must be refused when it is not");

        assert!(
            matches!(&err, OverlayError::UnsafeIdentity(got) if got == id),
            "{label} ({id:?}) produced {err:?}"
        );
        assert!(
            !root.exists(),
            "{label} ({id:?}) must leave the filesystem untouched"
        );
    }
}

#[test]
fn an_identity_at_the_length_limit_is_accepted_and_one_byte_past_it_is_not() {
    let reg = registry();
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);

    let mut at_limit = instance(&[]);
    at_limit.id = "a".repeat(IDENTITY_LEN_LIMIT);
    let report = materialize_overlay(&root, &at_limit, &reg)
        .expect("an identity exactly at the limit is still a legal directory name");
    assert_eq!(report.directory.file_name().unwrap(), at_limit.id.as_str());

    let mut past_limit = instance(&[]);
    past_limit.id = "a".repeat(IDENTITY_LEN_LIMIT + 1);
    let err = materialize_overlay(&root, &past_limit, &reg)
        .expect_err("one byte past the limit must be refused");
    assert!(
        matches!(&err, OverlayError::UnsafeIdentity(got) if got == &past_limit.id),
        "{err:?}"
    );
}

#[test]
fn ensuring_the_shared_directory_reclaims_the_runtime_from_hand_edits() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);

    let first = ensure_shared_directory(&root).expect("the shared subtree is created on demand");
    assert_eq!(
        fs::read_to_string(first.join(RUNTIME_ASSET)).unwrap(),
        forge_overlay::RUNTIME_SOURCE,
        "the shipped runtime lands on the first ensure"
    );

    fs::write(first.join(RUNTIME_ASSET), "hand-edited runtime").unwrap();
    let second = ensure_shared_directory(&root).expect("a second call succeeds");

    assert_eq!(second, first);
    assert_eq!(first, root.canonicalize().unwrap().join(RESERVED_DIRECTORY));
    assert_eq!(
        fs::read_to_string(first.join(RUNTIME_ASSET)).unwrap(),
        forge_overlay::RUNTIME_SOURCE,
        "the reserved subtree is generator territory - hand edits are reclaimed"
    );
}

#[test]
fn materializing_an_unregistered_kind_is_refused_before_a_directory_appears() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    let mut unshipped = instance(&[]);
    unshipped.kind_id = "overlay.vendor_unshipped".to_owned();

    let err = materialize_overlay(&root, &unshipped, &registry())
        .expect_err("a record whose kind this build lacks must not be materialized");

    assert!(
        matches!(&err, OverlayError::UnknownKind(id) if id == "overlay.vendor_unshipped"),
        "{err:?}"
    );
    assert!(!root.exists());
}

#[cfg(unix)]
#[test]
fn an_overlay_directory_that_is_a_symbolic_link_is_refused_without_writing_through_it() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    fs::create_dir_all(&root).unwrap();
    let elsewhere = home.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, root.join(IDENTITY)).unwrap();

    let err = materialize_overlay(&root, &instance(&[]), &registry())
        .expect_err("a linked overlay directory must be refused");

    assert!(
        matches!(&err, OverlayError::SymlinkedPath { .. }),
        "{err:?}"
    );
    assert!(
        names_in(&elsewhere).is_empty(),
        "the link target received generated files"
    );
}

#[cfg(unix)]
#[test]
fn a_generated_file_that_is_a_symbolic_link_is_refused_and_its_target_is_untouched() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    let reg = registry();
    let outside = home.path().join("victim.css");
    fs::write(&outside, "victim content").unwrap();

    let first = materialize_overlay(&root, &instance(&[]), &reg).expect("first materialize");
    fs::remove_file(first.directory.join(STYLE_FILE)).unwrap();
    std::os::unix::fs::symlink(&outside, first.directory.join(STYLE_FILE)).unwrap();

    let err = materialize_overlay(&root, &instance(&[]), &reg)
        .expect_err("a linked target file must be refused");

    assert!(
        matches!(&err, OverlayError::SymlinkedPath { .. }),
        "{err:?}"
    );
    assert_eq!(
        fs::read_to_string(&outside).unwrap(),
        "victim content",
        "the generated stylesheet was written through the link"
    );
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_on_the_staging_path_is_not_written_through() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    let reg = registry();
    let outside = home.path().join("victim.css");
    fs::write(&outside, "victim content").unwrap();

    let first = materialize_overlay(&root, &instance(&[]), &reg).expect("first materialize");
    let staged = first.directory.join(format!(".{STYLE_FILE}.tmp"));
    std::os::unix::fs::symlink(&outside, &staged).unwrap();

    let result = materialize_overlay(&root, &instance(&[]), &reg);

    assert_eq!(
        fs::read_to_string(&outside).unwrap(),
        "victim content",
        "the staging write followed a link out of the overlay root"
    );
    assert!(
        matches!(&result, Err(OverlayError::SymlinkedPath { .. })),
        "a linked staging path must be refused exactly like a linked target: {result:?}"
    );
}

#[test]
fn removing_an_overlay_takes_its_directory_and_nothing_else() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    let reg = registry();
    ensure_shared_directory(&root).unwrap();
    let doomed = materialize_overlay(&root, &instance(&[]), &reg).expect("the doomed overlay");
    let mut neighbour = instance(&[]);
    neighbour.id = "keep-me".to_owned();
    let kept = materialize_overlay(&root, &neighbour, &reg).expect("the neighbouring overlay");

    let removed = remove_overlay_directory(&root, IDENTITY).expect("the directory is removable");

    assert!(
        removed,
        "a directory that was there must be reported removed"
    );
    assert!(!doomed.directory.exists());
    assert_eq!(
        names_in(&kept.directory),
        generated_page(),
        "removing one overlay reached into its neighbour"
    );
    assert!(
        root.join(RESERVED_DIRECTORY).join(RUNTIME_ASSET).exists(),
        "removing one overlay took the shared runtime with it"
    );
}

#[test]
fn removing_reports_false_when_the_directory_or_the_whole_root_is_already_gone() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);

    assert!(
        !remove_overlay_directory(&root, IDENTITY).expect("a missing root is not an error"),
        "a root that was never created must report nothing to remove"
    );

    ensure_shared_directory(&root).unwrap();
    assert!(
        !remove_overlay_directory(&root, IDENTITY).expect("a missing directory is not an error"),
        "an overlay that owns no directory yet must report nothing to remove"
    );
}

#[test]
fn removing_refuses_an_identity_that_is_not_a_plain_directory_name() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    let shared = ensure_shared_directory(&root).unwrap();
    materialize_overlay(&root, &instance(&[]), &registry()).expect("a neighbour to protect");

    for &(id, label) in UNSAFE_IDENTITIES {
        let err = remove_overlay_directory(&root, id)
            .expect_err("an identity is a directory name and must be refused when it is not");

        assert!(
            matches!(&err, OverlayError::UnsafeIdentity(got) if got == id),
            "{label} ({id:?}) produced {err:?}"
        );
    }

    assert!(
        home.path().exists() && shared.join(RUNTIME_ASSET).exists(),
        "a refused identity still deleted something"
    );
    assert_eq!(
        names_in(&root.canonicalize().unwrap().join(IDENTITY)),
        generated_page(),
        "a refused identity reached the neighbouring overlay"
    );
}

#[cfg(unix)]
#[test]
fn removing_an_overlay_directory_that_is_a_symbolic_link_leaves_its_target_alone() {
    let home = TempDir::new().unwrap();
    let root = unborn_root(&home);
    fs::create_dir_all(&root).unwrap();
    let elsewhere = home.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(elsewhere.join("keep.txt"), "victim content").unwrap();
    std::os::unix::fs::symlink(&elsewhere, root.join(IDENTITY)).unwrap();

    let err = remove_overlay_directory(&root, IDENTITY)
        .expect_err("a linked overlay directory must be refused, never followed");

    assert!(
        matches!(&err, OverlayError::SymlinkedPath { .. }),
        "{err:?}"
    );
    assert_eq!(
        fs::read_to_string(elsewhere.join("keep.txt")).unwrap(),
        "victim content",
        "the removal followed a link out of the overlay root"
    );
}
