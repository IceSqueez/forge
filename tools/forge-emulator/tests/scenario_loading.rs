//! Scenario files: syntax rejection with file positions, shipped examples, round-trip, and the check CLI.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;

use forge_emulator::EmulatorError;
use forge_emulator::scenario::{Scenario, load_scenario, parse_scenario};

const EMULATOR: &str = env!("CARGO_BIN_EXE_forge-emulator");

fn scenarios_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios")
}

fn example_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(scenarios_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    files
}

const VALID: &str = r#"{
  "name": "n",
  "purpose": "p",
  "fixture": {},
  "steps": [
    { "do": { "forge_ready": { "within_ms": 1000 } } }
  ]
}"#;

#[test]
fn syntax_and_shape_errors_carry_file_line_and_column() {
    let cases = [
        (
            "unknown step",
            r#"{"name": "n", "purpose": "p", "fixture": {},
"steps": [{"do": {"wait_for": {}}}]}"#,
            (2, 28),
            "unknown variant `wait_for`, expected one of `forge_ready`, `twitch_subscribed`, `chat`, `crowd`, `session_reconnect`, `pause`, `run_action`, `set_global`",
        ),
        (
            "unknown field inside a step",
            r#"{"name": "n", "purpose": "p", "fixture": {},
"steps": [{"do": {"pause": {"ms": 5, "reason": "r",
  "jitter": 2}}}]}"#,
            (3, 10),
            "unknown field `jitter`, expected `ms` or `reason`",
        ),
        (
            "unknown expectation",
            r#"{"name": "n", "purpose": "p", "fixture": {},
"steps": [{"do": {"forge_ready": {"within_ms": 1}},
  "expect": [{"event_seen": {}}]}]}"#,
            (3, 26),
            "unknown variant `event_seen`, expected one of `event`, `event_absent`, `caused_by`, `twitch_subscription`, `twitch_no_unexpected_requests`, `twitch_request_count`, `log_line`",
        ),
        (
            "unknown top-level field",
            r#"{"name": "n", "purpose": "p", "fixture": {}, "steps": [],
"timeout": 5}"#,
            (2, 9),
            "unknown field `timeout`, expected one of `name`, `purpose`, `fixture`, `fakes`, `steps`",
        ),
        (
            "unknown fixture field is refused by the reused fixture schema",
            r#"{"name": "n", "purpose": "p",
"fixture": {"twich": {}}, "steps": []}"#,
            (2, 19),
            "unknown field `twich`, expected `twitch` or `chat_commands`",
        ),
        (
            "missing deadline",
            r#"{"name": "n", "purpose": "p", "fixture": {},
"steps": [{"do": {"forge_ready": {}}}]}"#,
            (2, 35),
            "missing field `within_ms`",
        ),
        (
            "repeated payload pointer",
            r#"{"name": "n", "purpose": "p", "fixture": {},
"steps": [{"do": {"forge_ready": {"within_ms": 1}}, "expect": [{"event": {"kind": "k", "within_ms": 1,
"payload": {"/a": {"equals": 1}, "/a": {"equals": 2}}}}]}]}"#,
            (3, 37),
            "duplicate key `/a`",
        ),
        (
            "unknown badge",
            r#"{"name": "n", "purpose": "p", "fixture": {}, "steps": [{"do": {"chat": {
"viewer": {"user_id": "1", "login": "a", "badges": ["founder"]}, "text": "t"}}}]}"#,
            (2, 61),
            "unknown variant `founder`, expected one of `broadcaster`, `moderator`, `vip`, `subscriber`",
        ),
        (
            "trailing comma",
            "{\"name\": \"n\",\n}",
            (2, 1),
            "trailing comma",
        ),
    ];
    let path = Path::new("cases/bug-1234.json");
    for (label, text, (line, column), reason) in cases {
        match parse_scenario(path, text) {
            Err(EmulatorError::ScenarioSyntax {
                path: reported,
                line: got_line,
                column: got_column,
                reason: got_reason,
            }) => {
                assert_eq!(reported, path, "case: {label}");
                assert_eq!((got_line, got_column), (line, column), "case: {label}");
                assert_eq!(got_reason, reason, "case: {label}");
            }
            other => panic!("case {label}: expected a syntax error, got {other:?}"),
        }
    }
}

#[test]
fn syntax_error_display_leads_with_path_line_and_column() {
    let error = parse_scenario(Path::new("s.json"), "{\n  \"nam\": 1\n}").unwrap_err();
    assert_eq!(
        error.to_string(),
        "s.json:2:7: unknown field `nam`, expected one of `name`, `purpose`, `fixture`, `fakes`, `steps`"
    );
}

#[test]
fn semantic_rejection_lists_every_problem_under_the_file_path() {
    let text = VALID
        .replace(r#""name": "n""#, r#""name": """#)
        .replace(r#""within_ms": 1000"#, r#""within_ms": 0"#);
    let error = parse_scenario(Path::new("cases/empty.json"), &text).unwrap_err();
    assert!(
        matches!(&error, EmulatorError::ScenarioInvalid { problems, .. } if problems.len() == 2),
        "got {error:?}"
    );
    assert_eq!(
        error.to_string(),
        "cases/empty.json: invalid scenario\n  name: must not be blank\n  steps[0].do.forge_ready.within_ms: must be between 1 and 300000, got 0"
    );
}

#[test]
fn byte_order_mark_and_crlf_line_endings_are_accepted() {
    let text = format!("\u{feff}{}", VALID.replace('\n', "\r\n"));
    let scenario = parse_scenario(Path::new("windows.json"), &text).unwrap();
    assert_eq!(scenario.name, "n");
}

#[test]
fn missing_file_is_unreadable_naming_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("absent.json");
    let error = load_scenario(&path).unwrap_err();
    assert!(
        matches!(&error, EmulatorError::ScenarioUnreadable { path: reported, .. } if reported == &path),
        "got {error:?}"
    );
}

#[test]
fn every_shipped_example_scenario_validates() {
    let files = example_files();
    assert!(
        !files.is_empty(),
        "no example scenarios under {:?}",
        scenarios_dir()
    );
    for file in files {
        if let Err(error) = load_scenario(&file) {
            panic!("{error}");
        }
    }
}

#[test]
fn example_scenarios_survive_a_serialize_and_reload_round_trip() {
    for file in example_files() {
        let loaded = load_scenario(&file).unwrap();
        let written = serde_json::to_string_pretty(&loaded).unwrap();
        let reloaded: Scenario = parse_scenario(&file, &written).unwrap();
        assert_eq!(reloaded, loaded, "{}", file.display());
    }
}

#[test]
fn check_command_exit_code_names_the_failure_class() {
    let dir = tempfile::tempdir().unwrap();
    let syntax = dir.path().join("syntax.json");
    std::fs::write(&syntax, "{").unwrap();
    let invalid = dir.path().join("invalid.json");
    std::fs::write(&invalid, VALID.replace(r#""name": "n""#, r#""name": """#)).unwrap();
    let valid = dir.path().join("valid.json");
    std::fs::write(&valid, VALID).unwrap();

    for (file, code, stream_fragment) in [
        (valid.clone(), 0, "ok, `n` with 1 step(s)"),
        (dir.path().join("absent.json"), 9, "cannot read scenario"),
        (syntax, 10, "syntax.json:1:1: EOF while parsing an object"),
        (invalid, 11, "name: must not be blank"),
    ] {
        let output = Command::new(EMULATOR)
            .args(["scenario", "check"])
            .arg(&file)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}: {stderr}",
            file.display()
        );
        let stream = if code == 0 { &stdout } else { &stderr };
        assert!(
            stream.contains(stream_fragment),
            "{}: expected {stream_fragment:?} in {stream:?}",
            file.display()
        );
    }
}

#[test]
fn run_command_refuses_a_bad_scenario_file_before_launching_anything() {
    let dir = tempfile::tempdir().unwrap();
    let invalid = dir.path().join("invalid.json");
    std::fs::write(&invalid, VALID.replace(r#""name": "n""#, r#""name": """#)).unwrap();
    let run_root = dir.path().join("run");

    for (file, code) in [(dir.path().join("absent.json"), 9), (invalid, 11)] {
        let output = Command::new(EMULATOR)
            .args(["scenario", "run"])
            .arg(&file)
            .arg("--forge")
            .arg(dir.path().join("no-forge-here"))
            .arg("--run-root")
            .arg(&run_root)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}: {}",
            file.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !run_root.exists(),
            "{}: a run root was prepared",
            file.display()
        );
    }
}
