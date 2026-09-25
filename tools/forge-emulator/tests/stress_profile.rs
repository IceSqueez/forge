#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use forge_emulator::EmulatorError;
use forge_emulator::stress::{StressProfile, load_profile};
use serde_json::{Value, json};

fn base() -> Value {
    json!({
        "name": "n",
        "purpose": "p",
        "fixture": {
            "twitch": {},
            "queues": [{ "name": "Chat", "concurrency": 4 }],
            "overlays": [{ "display_name": "Chat Box", "kind_id": "overlay.chat" }],
            "chat_commands": [{ "phrase": "!hype", "action_name": "Hype" }],
            "event_triggers": [
                { "trigger_kind": "twitch.chat.message", "action_name": "Chat", "queue": "Chat" }
            ]
        },
        "pages": ["Chat Box"],
        "senders": 10,
        "stimuli": [
            { "kind": "chat", "weight": 9, "runs": ["Chat"] },
            { "kind": "command", "weight": 1, "text": "!hype", "runs": ["Chat", "Hype"] },
            { "kind": "follow", "per_minute": 6, "runs": ["Hype"] }
        ],
        "ramp": { "rates": [10, 20], "hold_secs": 5 },
        "burst": { "multiplier": 2.0, "secs": 5 },
        "recovery_secs": 5,
        "knee": { "watch": ["Chat"], "max_backlog_secs": 5.0, "min_achieved_share": 0.9 }
    })
}

fn problems(profile: Value) -> Vec<String> {
    serde_json::from_value::<StressProfile>(profile)
        .unwrap()
        .problems()
}

fn with(mutate: impl FnOnce(&mut Value)) -> Value {
    let mut profile = base();
    mutate(&mut profile);
    profile
}

#[test]
fn the_base_profile_has_no_problems() {
    assert_eq!(problems(base()), Vec::<String>::new());
}

#[test]
fn each_unrunnable_profile_is_refused_naming_what_is_wrong() {
    let cases: Vec<(&str, Value, &str)> = vec![
        (
            "no senders",
            with(|p| p["senders"] = json!(0)),
            "senders must be between 1",
        ),
        (
            "empty ramp",
            with(|p| p["ramp"]["rates"] = json!([])),
            "ramp.rates is empty",
        ),
        (
            "zero rate",
            with(|p| p["ramp"]["rates"] = json!([10, 0])),
            "ramp.rates[1]",
        ),
        (
            "rate over the ceiling",
            with(|p| p["ramp"]["rates"] = json!([100_001])),
            "ramp.rates[0]",
        ),
        (
            "zero hold",
            with(|p| p["ramp"]["hold_secs"] = json!(0)),
            "ramp.hold_secs",
        ),
        (
            "burst below the last step",
            with(|p| p["burst"]["multiplier"] = json!(0.5)),
            "burst.multiplier",
        ),
        (
            "no twitch account",
            with(|p| p["fixture"]["twitch"] = Value::Null),
            "fixture.twitch is required",
        ),
        (
            "page not seeded",
            with(|p| p["pages"] = json!(["Alert Box"])),
            "pages names `Alert Box`",
        ),
        (
            "knee watches nothing",
            with(|p| p["knee"]["watch"] = json!([])),
            "knee.watch names no action",
        ),
        (
            "knee watches a ghost",
            with(|p| p["knee"]["watch"] = json!(["Ghost"])),
            "knee.watch names `Ghost`",
        ),
        (
            "share over one",
            with(|p| p["knee"]["min_achieved_share"] = json!(1.5)),
            "min_achieved_share",
        ),
        (
            "run of an unseeded action",
            with(|p| p["stimuli"][0]["runs"] = json!(["Ghost"])),
            "runs `Ghost`",
        ),
        (
            "stimulus that runs nothing",
            with(|p| p["stimuli"][0]["runs"] = json!([])),
            "runs no action",
        ),
        (
            "both weight and rate",
            with(|p| p["stimuli"][2]["weight"] = json!(1)),
            "exactly one of weight or per_minute",
        ),
        (
            "neither weight nor rate",
            with(|p| p["stimuli"][2]["per_minute"] = Value::Null),
            "exactly one of weight or per_minute",
        ),
        (
            "zero weight",
            with(|p| p["stimuli"][1]["weight"] = json!(0)),
            "is zero",
        ),
        (
            "command without phrase",
            with(|p| p["stimuli"][1]["text"] = Value::Null),
            "needs the command phrase",
        ),
        (
            "repeated kind",
            with(|p| p["stimuli"][2]["kind"] = json!("chat")),
            "repeats a kind",
        ),
        (
            "nothing weighted",
            with(|p| {
                p["stimuli"] = json!([{ "kind": "follow", "per_minute": 6, "runs": ["Hype"] }]);
            }),
            "no stimulus has a weight",
        ),
        (
            "fixture refuses its queue",
            with(|p| p["fixture"]["event_triggers"][0]["queue"] = json!("Alerts")),
            "runs on queue `Alerts`, which the fixture does not declare",
        ),
    ];
    for (label, profile, expected) in cases {
        let found = problems(profile);
        assert!(
            found.iter().any(|problem| problem.contains(expected)),
            "{label}: expected a problem containing {expected:?}, got {found:?}"
        );
    }
}

#[test]
fn an_unknown_field_is_rejected_rather_than_ignored() {
    let profile = with(|p| p["ramp"]["hold_sec"] = json!(5));
    assert!(serde_json::from_value::<StressProfile>(profile).is_err());
}

fn shipped_profiles() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("stress");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    files
}

#[test]
fn every_shipped_profile_loads_without_problems() {
    let files = shipped_profiles();
    assert!(!files.is_empty(), "stress/ holds no profile");
    for file in files {
        if let Err(e) = load_profile(&file) {
            panic!("{}: {e}", file.display());
        }
    }
}

#[test]
fn an_invalid_profile_file_reports_every_problem_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.json");
    let profile = with(|p| {
        p["senders"] = json!(0);
        p["ramp"]["rates"] = json!([]);
    });
    std::fs::write(&path, profile.to_string()).unwrap();

    match load_profile(&path) {
        Err(EmulatorError::StressProfileInvalid { problems, .. }) => {
            assert_eq!(problems.len(), 2, "{problems:?}");
        }
        other => panic!(
            "expected an invalid profile, got {:?}",
            other.map(|p| p.name)
        ),
    }
}
