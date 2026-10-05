#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use forge_events::{Event, EventPublisher};
use forge_registry::{RunContext, SubActionRunner};
use forge_runtime::sub_action_runners::{
    CoreFileDeleteRunner, CoreFileListRunner, CoreFileReadRunner, CoreFileWriteRunner,
};
use forge_types::{ArgStack, EventId, SubActionConfig, SubActionOutcome, Variant};

const TRAVERSAL: &str = "\"..\" is not allowed";
const ABSOLUTE: &str = "absolute paths are not allowed";
#[cfg(unix)]
const ESCAPE: &str = "leads outside the files folder";
const EMPTY: &str = "path is empty";
const READ_CAP_BYTES: usize = 1024 * 1024;
const SENTINEL: &str = "qa-sentinel";

static ASSETS_ROOT: OnceLock<PathBuf> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(0);

#[allow(unsafe_code)]
fn assets_root() -> &'static Path {
    ASSETS_ROOT.get_or_init(|| {
        let data_dir = tempfile::tempdir().unwrap().keep();
        // SAFETY: every test in this binary calls assets_root() before any other work, so all other test threads are blocked on this OnceLock and none reads the environment while it is written.
        unsafe {
            std::env::set_var("FORGE_DATA_DIR", &data_dir);
        }
        let root = data_dir.join("assets");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::canonicalize(root).unwrap()
    })
}

fn case_dir(label: &str) -> (PathBuf, String) {
    let rel = format!("{label}-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let abs = assets_root().join(&rel);
    std::fs::create_dir_all(&abs).unwrap();
    (abs, rel)
}

#[cfg(unix)]
fn outside_dir() -> PathBuf {
    tempfile::tempdir().unwrap().keep()
}

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

#[derive(Clone, Copy, Debug)]
enum Step {
    Read,
    Write,
    List,
    Delete,
}

const ALL_STEPS: [Step; 4] = [Step::Read, Step::Write, Step::List, Step::Delete];

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn whole_file() -> (&'static str, Variant) {
    ("read_as", text("Whole file"))
}

async fn run_with(
    step: Step,
    path: &str,
    stack: &ArgStack,
    extra: &[(&str, Variant)],
) -> (SubActionOutcome, Option<ArgStack>) {
    assets_root();
    let mut cfg = SubActionConfig::new();
    cfg.insert("path".to_owned(), text(path));
    match step {
        Step::Read => {
            cfg.insert("target_var".to_owned(), text("out"));
        }
        Step::Write => {
            cfg.insert("content".to_owned(), text("payload"));
        }
        Step::List | Step::Delete => {}
    }
    for (key, value) in extra {
        cfg.insert((*key).to_owned(), value.clone());
    }
    let ctx = RunContext::leaf(stack, 0, EventId::new(), &NullPublisher);
    let (telemetry, produced) = match step {
        Step::Read => CoreFileReadRunner.execute(&cfg, &ctx).await,
        Step::Write => CoreFileWriteRunner.execute(&cfg, &ctx).await,
        Step::List => CoreFileListRunner.execute(&cfg, &ctx).await,
        Step::Delete => CoreFileDeleteRunner.execute(&cfg, &ctx).await,
    };
    (telemetry.outcome, produced)
}

async fn run(step: Step, path: &str) -> (SubActionOutcome, Option<ArgStack>) {
    run_with(step, path, &ArgStack::new(), &[]).await
}

async fn failure_with(
    step: Step,
    path: &str,
    stack: &ArgStack,
    extra: &[(&str, Variant)],
) -> String {
    let (outcome, produced) = run_with(step, path, stack, extra).await;
    let SubActionOutcome::Failed(msg) = outcome else {
        panic!("{step:?} {path:?}: expected a failure, got {outcome:?}");
    };
    assert!(
        produced.is_none(),
        "{step:?} {path:?}: a failed step must not bind variables"
    );
    msg
}

async fn failure(step: Step, path: &str) -> String {
    failure_with(step, path, &ArgStack::new(), &[]).await
}

#[tokio::test]
async fn every_file_step_refuses_parent_traversal() {
    for path in ["..", "../escape.txt", "a/../b", "sub/../../x", "./.."] {
        for step in ALL_STEPS {
            let msg = failure(step, path).await;
            assert!(msg.contains(TRAVERSAL), "{step:?} {path:?}: {msg}");
        }
    }
    let data_dir = assets_root().parent().unwrap();
    assert!(!data_dir.join("escape.txt").exists());
    assert!(!data_dir.join("x").exists());
}

#[tokio::test]
async fn every_file_step_refuses_absolute_paths_in_any_os_syntax() {
    for path in [
        "/etc/passwd",
        "/",
        "\\abs",
        "\\\\server\\share",
        "//server/share",
        "C:\\x",
        "c:/x",
        "C:",
        "z:relative",
    ] {
        for step in ALL_STEPS {
            let msg = failure(step, path).await;
            assert!(msg.contains(ABSOLUTE), "{step:?} {path:?}: {msg}");
        }
    }
}

#[tokio::test]
async fn every_file_step_refuses_an_empty_or_whitespace_path() {
    for path in ["", " ", "\t\n"] {
        for step in ALL_STEPS {
            let msg = failure(step, path).await;
            assert!(msg.contains(EMPTY), "{step:?} {path:?}: {msg}");
        }
    }
}

#[tokio::test]
async fn interpolated_segments_go_through_the_same_checks() {
    for (value, expected) in [
        ("..", TRAVERSAL),
        ("sub/../..", TRAVERSAL),
        ("/etc", ABSOLUTE),
        ("C:", ABSOLUTE),
        ("", ABSOLUTE),
    ] {
        let stack = ArgStack::new().set("dir".to_owned(), text(value));
        for step in ALL_STEPS {
            let msg = failure_with(step, "%dir%/x.txt", &stack, &[]).await;
            assert!(msg.contains(expected), "{step:?} dir={value:?}: {msg}");
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn backslash_parent_is_a_literal_file_name_inside_the_root_on_unix() {
    let (abs, rel) = case_dir("literal");
    let path = format!("{rel}/..\\x");

    let (outcome, _) = run(Step::Write, &path).await;

    assert_eq!(outcome, SubActionOutcome::Success);
    assert_eq!(
        std::fs::read_to_string(abs.join("..\\x")).unwrap(),
        "payload"
    );
}

#[tokio::test]
async fn write_then_list_read_and_delete_share_the_files_folder() {
    let (abs, rel) = case_dir("shared");
    let file = format!("{rel}/notes/today.txt");

    let (written, _) = run_with(
        Step::Write,
        &file,
        &ArgStack::new(),
        &[("create_parent_dirs", Variant::Bool(true))],
    )
    .await;
    assert_eq!(written, SubActionOutcome::Success);
    assert!(abs.join("notes").join("today.txt").is_file());

    let (_, listed) = run_with(
        Step::List,
        &rel,
        &ArgStack::new(),
        &[("recursive", Variant::Bool(true))],
    )
    .await;
    assert_eq!(
        listed.unwrap().get("file.entries").cloned(),
        Some(Variant::Array(vec![text("notes/today.txt")]))
    );

    let (_, read) = run_with(Step::Read, &file, &ArgStack::new(), &[whole_file()]).await;
    assert_eq!(read.unwrap().get("out").cloned(), Some(text("payload")));

    let (deleted, _) = run(Step::Delete, &file).await;
    assert_eq!(deleted, SubActionOutcome::Success);
    assert!(!abs.join("notes").join("today.txt").exists());
}

#[tokio::test]
async fn an_existing_file_is_reachable_by_every_step() {
    let (abs, rel) = case_dir("existing");
    std::fs::write(abs.join("kept.txt"), "before").unwrap();
    let path = format!("{rel}/kept.txt");

    for (step, extra) in [
        (Step::Read, vec![]),
        (Step::Write, vec![("mode", text("append"))]),
        (Step::Write, vec![("mode", text("overwrite"))]),
        (Step::Delete, vec![]),
    ] {
        let (outcome, _) = run_with(step, &path, &ArgStack::new(), &extra).await;
        assert_eq!(outcome, SubActionOutcome::Success, "{step:?} {extra:?}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_to_an_outside_file_is_refused_by_every_step() {
    let outside = outside_dir();
    let secret = outside.join("secret.txt");
    std::fs::write(&secret, "outside-secret").unwrap();
    let (abs, rel) = case_dir("link-file");
    std::os::unix::fs::symlink(&secret, abs.join("to_file")).unwrap();
    let path = format!("{rel}/to_file");

    for (step, extra) in [
        (Step::Read, vec![]),
        (Step::List, vec![]),
        (Step::Delete, vec![]),
        (Step::Write, vec![("mode", text("overwrite"))]),
        (Step::Write, vec![("mode", text("append"))]),
    ] {
        let msg = failure_with(step, &path, &ArgStack::new(), &extra).await;
        assert!(msg.contains(ESCAPE), "{step:?} {extra:?}: {msg}");
    }
    assert_eq!(std::fs::read_to_string(&secret).unwrap(), "outside-secret");
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_to_an_outside_directory_is_refused_by_every_step() {
    let outside = outside_dir();
    std::fs::write(outside.join("secret.txt"), "outside-secret").unwrap();
    let (abs, rel) = case_dir("link-dir");
    std::os::unix::fs::symlink(&outside, abs.join("to_dir")).unwrap();

    for (step, path) in [
        (Step::Read, format!("{rel}/to_dir/secret.txt")),
        (Step::Write, format!("{rel}/to_dir/new.txt")),
        (Step::List, format!("{rel}/to_dir")),
        (Step::Delete, format!("{rel}/to_dir/secret.txt")),
    ] {
        let msg = failure(step, &path).await;
        assert!(msg.contains(ESCAPE), "{step:?} {path:?}: {msg}");
    }
    assert!(outside.join("secret.txt").exists());
    assert!(!outside.join("new.txt").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn broken_symlink_is_refused_and_writes_create_nothing_outside() {
    let outside = outside_dir();
    let target = outside.join("not-yet.txt");
    let (abs, rel) = case_dir("link-broken");
    std::os::unix::fs::symlink(&target, abs.join("broken")).unwrap();
    let path = format!("{rel}/broken");

    for (step, extra) in [
        (Step::Read, vec![]),
        (Step::List, vec![]),
        (Step::Delete, vec![]),
        (Step::Write, vec![("mode", text("overwrite"))]),
        (Step::Write, vec![("mode", text("append"))]),
        (Step::Write, vec![("mode", text("create_new"))]),
    ] {
        let msg = failure_with(step, &path, &ArgStack::new(), &extra).await;
        assert!(msg.contains(ESCAPE), "{step:?} {extra:?}: {msg}");
    }
    assert!(!target.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_that_stays_inside_the_root_is_followed() {
    let (abs, rel) = case_dir("link-inside");
    std::fs::write(abs.join("real.txt"), "inside").unwrap();
    std::os::unix::fs::symlink(abs.join("real.txt"), abs.join("alias.txt")).unwrap();

    let (outcome, produced) = run_with(
        Step::Read,
        &format!("{rel}/alias.txt"),
        &ArgStack::new(),
        &[whole_file()],
    )
    .await;

    assert_eq!(outcome, SubActionOutcome::Success);
    assert_eq!(produced.unwrap().get("out").cloned(), Some(text("inside")));
}

#[tokio::test]
async fn read_accepts_exactly_the_size_cap_and_refuses_one_byte_more() {
    let (abs, rel) = case_dir("size");
    std::fs::write(abs.join("at_cap.txt"), "a".repeat(READ_CAP_BYTES)).unwrap();
    std::fs::write(abs.join("over_cap.txt"), "a".repeat(READ_CAP_BYTES + 1)).unwrap();

    let (at_cap, produced) = run_with(
        Step::Read,
        &format!("{rel}/at_cap.txt"),
        &ArgStack::new(),
        &[whole_file()],
    )
    .await;
    assert_eq!(at_cap, SubActionOutcome::Success);
    assert!(matches!(
        produced.unwrap().get("out"),
        Some(Variant::String(s)) if s.len() == READ_CAP_BYTES
    ));

    let msg = failure(Step::Read, &format!("{rel}/over_cap.txt")).await;
    assert!(msg.contains("byte cap"), "{msg}");
}

#[tokio::test]
async fn read_of_a_directory_fails_as_not_a_file() {
    let (_, rel) = case_dir("dir-target");

    for path in [rel.clone(), ".".to_owned()] {
        let msg = failure(Step::Read, &path).await;
        assert!(msg.contains("path is not a file"), "{path:?}: {msg}");
    }
}

#[tokio::test]
async fn read_of_a_missing_file_names_the_files_folder() {
    let msg = failure(Step::Read, "never-written.txt").await;

    assert!(msg.contains("file not found in the files folder"), "{msg}");
}

#[tokio::test]
async fn read_of_non_utf8_content_fails_as_a_read_error() {
    let (abs, rel) = case_dir("binary");
    std::fs::write(abs.join("blob.bin"), [0xFF, 0xFE, 0x00, 0xC3]).unwrap();

    let msg = failure(Step::Read, &format!("{rel}/blob.bin")).await;

    assert!(msg.starts_with("read failed"), "{msg}");
}

#[tokio::test]
async fn failure_messages_never_contain_the_path_or_the_files_folder() {
    let (abs, rel) = case_dir(SENTINEL);
    std::fs::create_dir_all(abs.join("sub-dir")).unwrap();
    std::fs::write(abs.join("big.txt"), "a".repeat(READ_CAP_BYTES + 1)).unwrap();
    std::fs::write(abs.join("blob.bin"), [0xFF, 0xFE]).unwrap();
    std::fs::write(abs.join("exists.txt"), "x").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside_dir(), abs.join("escape")).unwrap();

    let mut rows = vec![
        (Step::Read, format!("{rel}/missing.txt"), vec![]),
        (Step::Read, format!("/{rel}/abs.txt"), vec![]),
        (Step::Read, format!("../{rel}"), vec![]),
        (Step::Read, format!("{rel}/sub-dir"), vec![]),
        (Step::Read, format!("{rel}/big.txt"), vec![]),
        (Step::Read, format!("{rel}/blob.bin"), vec![]),
        (Step::Write, format!("{rel}/no-dir/x.txt"), vec![]),
        (
            Step::Write,
            format!("{rel}/exists.txt"),
            vec![("mode", text("create_new"))],
        ),
        (Step::List, format!("{rel}/no-dir"), vec![]),
        (Step::Delete, format!("{rel}/missing.txt"), vec![]),
        (Step::Delete, format!("{rel}/sub-dir"), vec![]),
    ];
    if cfg!(unix) {
        rows.push((Step::Read, format!("{rel}/escape/x.txt"), vec![]));
    }

    let root = assets_root().to_string_lossy().into_owned();
    for (step, path, extra) in rows {
        let msg = failure_with(step, &path, &ArgStack::new(), &extra).await;
        assert!(!msg.contains(SENTINEL), "{step:?} {path:?} leaked: {msg}");
        assert!(!msg.contains(&root), "{step:?} {path:?} leaked: {msg}");
    }
}

async fn read_fixture(
    read_as: Option<&str>,
    target_var: &str,
    out_key: &str,
    contents: &str,
) -> (SubActionOutcome, Option<Variant>) {
    let (abs, rel) = case_dir("read");
    std::fs::write(abs.join("fixture.dat"), contents).unwrap();
    let mut extra = vec![("target_var", text(target_var))];
    if let Some(mode) = read_as {
        extra.push(("read_as", text(mode)));
    }
    let (outcome, produced) = run_with(
        Step::Read,
        &format!("{rel}/fixture.dat"),
        &ArgStack::new(),
        &extra,
    )
    .await;
    (outcome, produced.and_then(|s| s.get(out_key).cloned()))
}

fn strings(items: &[&str]) -> Variant {
    Variant::Array(items.iter().map(|s| text(s)).collect())
}

#[tokio::test]
async fn lines_mode_splits_on_newlines_stripping_trailing_carriage_returns() {
    let (outcome, value) =
        read_fixture(Some("Lines array"), "out", "out", "alpha\r\nbeta\ngamma").await;
    assert_eq!(outcome, SubActionOutcome::Success);
    assert_eq!(value, Some(strings(&["alpha", "beta", "gamma"])));
}

#[tokio::test]
async fn unrecognised_read_as_defaults_to_lines_array() {
    for read_as in [None, Some("something-else")] {
        let (outcome, value) = read_fixture(read_as, "out", "out", "x\ny").await;
        assert_eq!(outcome, SubActionOutcome::Success, "read_as={read_as:?}");
        assert_eq!(value, Some(strings(&["x", "y"])), "read_as={read_as:?}");
    }
}

#[tokio::test]
async fn whole_file_mode_returns_the_entire_content_as_one_string() {
    let (outcome, value) = read_fixture(Some("Whole file"), "out", "out", "line1\nline2\n").await;
    assert_eq!(outcome, SubActionOutcome::Success);
    assert_eq!(value, Some(text("line1\nline2\n")));
}

#[tokio::test]
async fn json_mode_parses_object_root_into_variant_object() {
    let (outcome, value) =
        read_fixture(Some("JSON"), "out", "out", r#"{"name":"forge","count":3}"#).await;
    assert_eq!(outcome, SubActionOutcome::Success);
    let Some(Variant::Object(map)) = value else {
        panic!("expected Variant::Object, got {value:?}");
    };
    assert_eq!(map.get("name"), Some(&text("forge")));
    assert_eq!(map.get("count"), Some(&Variant::Int(3)));
}

#[tokio::test]
async fn json_mode_failures_report_typed_message_and_produce_no_binding() {
    for (contents, needle) in [
        ("{not valid json", "invalid JSON"),
        ("null", "unsupported JSON value"),
    ] {
        let (outcome, value) = read_fixture(Some("JSON"), "out", "out", contents).await;
        assert!(
            matches!(&outcome, SubActionOutcome::Failed(m) if m.contains(needle)),
            "contents {contents:?} expected {needle:?}, got {outcome:?}",
        );
        assert!(value.is_none());
    }
}

#[tokio::test]
async fn target_var_name_is_sanitized_before_binding() {
    let (outcome, value) = read_fixture(Some("Whole file"), "  %result%  ", "result", "hi").await;
    assert_eq!(outcome, SubActionOutcome::Success);
    assert_eq!(value, Some(text("hi")));
}
