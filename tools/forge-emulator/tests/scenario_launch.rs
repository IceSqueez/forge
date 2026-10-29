#![allow(clippy::unwrap_used)]
#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::time::Duration;

use forge_emulator::EmulatorError;
use forge_emulator::harness::launch::DEFAULT_LOG_DIRECTIVES;
use forge_emulator::harness::run::{RunOptions, ScenarioVerdict, StepStatus, run_scenario};
use forge_emulator::harness::scenario::{Scenario, parse_scenario};
use tempfile::TempDir;
use tokio::time::timeout;

mod support;
use support::{EMULATOR, clear_desktop, elsewhere, stub_forge, wait_until_gone};

const DEADLINE: Duration = Duration::from_secs(60);

fn chat_scenario() -> Scenario {
    parse_scenario(
        Path::new("inline.json"),
        r#"{
          "name": "launch lifecycle",
          "purpose": "exercise the run lifecycle",
          "fixture": { "twitch": {}, "chat_commands": [{ "phrase": "!ping", "action_name": "Ping" }] },
          "fakes": { "twitch": {} },
          "steps": [
            { "do": { "forge_ready": { "within_ms": 60000 } } },
            { "do": { "twitch_subscribed": { "types": ["channel.chat.message"], "within_ms": 1000 } } }
          ]
        }"#,
    )
    .unwrap()
}

fn options(root: &TempDir, stub_body: &str) -> RunOptions {
    let script = root.path().join("stub-forge.sh");
    std::fs::write(&script, stub_body).unwrap();
    RunOptions {
        emulator: PathBuf::from(EMULATOR),
        forge: stub_forge(&script),
        run_root: root.path().join("run"),
        log_directives: DEFAULT_LOG_DIRECTIVES.to_owned(),
        guard: clear_desktop(),
        live: elsewhere(),
        max_attempts: 1,
        shutdown_grace: Duration::from_secs(5),
    }
}

async fn read_pid(file: &Path) -> u32 {
    loop {
        if let Some(pid) = std::fs::read_to_string(file)
            .ok()
            .and_then(|text| text.trim().parse().ok())
        {
            return pid;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn stop_while_forge_boots_interrupts_the_run_and_leaves_no_forge_running() {
    let root = tempfile::tempdir().unwrap();
    let pid_file = root.path().join("forge.pid");
    let stub = format!("echo $$ > '{}'\nexec sleep 600", pid_file.display());
    let options = options(&root, &stub);
    let booted = read_pid(&pid_file);

    let outcome = timeout(
        DEADLINE,
        run_scenario(&chat_scenario(), options, async {
            booted.await;
        }),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(outcome.verdict, ScenarioVerdict::Interrupted);
    let statuses: Vec<StepStatus> = outcome.steps.iter().map(|step| step.status).collect();
    assert_eq!(statuses, [StepStatus::Interrupted, StepStatus::NotRun]);
    let pid = read_pid(&pid_file).await;
    assert!(
        wait_until_gone(pid, DEADLINE).await,
        "forge stub {pid} survived the interrupted run"
    );
}

#[tokio::test]
async fn forge_exiting_during_boot_is_a_launch_error_not_a_scenario_outcome() {
    let root = tempfile::tempdir().unwrap();
    let options = options(&root, "echo 'boot failed' >&2\nexit 9");

    let failure = timeout(
        DEADLINE,
        run_scenario(&chat_scenario(), options, std::future::pending()),
    )
    .await
    .unwrap();

    assert!(
        matches!(&failure, Err(EmulatorError::ForgeExited { code: Some(9), stderr_tail, .. })
            if stderr_tail.contains("boot failed")),
        "got {:?}",
        failure.map(|outcome| outcome.verdict)
    );
}
