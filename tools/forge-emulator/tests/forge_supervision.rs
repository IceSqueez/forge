//! Supervision against `/bin/sh` stub children. Nothing here ever starts the real forge binary.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::net::{Ipv4Addr, TcpListener as StdTcpListener};
use std::path::{Path, PathBuf};
use std::time::Duration;

use forge_emulator::EmulatorError;
use forge_emulator::fixture::ForgeDataDir;
use forge_emulator::launch::{
    DEFAULT_LOG_DIRECTIVES, ForgeCommand, ForgeProcess, HyprlandProbe, INHERITED_VARIABLES,
    LaunchSpec, LivePaths,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;

const DEADLINE: Duration = Duration::from_secs(10);
const TOKEN: &str = "fixture-bearer-token";
const BIND_FAILURE_LINE: &str = "forge-desktop: server failed to start, leaving it off: could not bind to 127.0.0.1:40000: Address already in use (os error 98)";
const OVERRIDE_REFUSAL_LINE: &str = "2026-09-13T10:00:00.000000Z ERROR platform endpoint override refused; exiting error=endpoint override FORGE_TWITCH_EVENTSUB_WS_URL refused: host is not 127.0.0.0/8, ::1 or localhost";

struct Stage {
    root: TempDir,
    data: PathBuf,
    home: PathBuf,
}

impl Stage {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        std::fs::create_dir(&data).unwrap();
        let home = root.path().join("home");
        Self { root, data, home }
    }

    fn file(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    /// Run through `/bin/sh` so no test ever execs a file another test thread may still hold open.
    fn stub(&self, body: &str) -> ForgeCommand {
        let script = self.file("stub-forge.sh");
        std::fs::write(&script, body).unwrap();
        ForgeCommand {
            program: PathBuf::from("/bin/sh"),
            args: vec![script.into_os_string()],
        }
    }

    fn spec(&self, forge: ForgeCommand) -> LaunchSpec {
        LaunchSpec {
            forge,
            data_dir: ForgeDataDir::fresh(&self.data).unwrap(),
            home: self.home.clone(),
            twitch_client_id: Some("fixtureclientid".to_owned()),
            endpoint_overrides: vec![(
                "FORGE_TWITCH_API_BASE_URL",
                "http://127.0.0.1:1".to_owned(),
            )],
            log_directives: DEFAULT_LOG_DIRECTIVES.to_owned(),
        }
    }

    async fn spawn(&self, body: &str) -> ForgeProcess {
        ForgeProcess::spawn(&self.spec(self.stub(body)), &clear_desktop(), &elsewhere())
            .await
            .unwrap()
    }
}

fn clear_desktop() -> HyprlandProbe {
    probe_script("printf '[]'")
}

fn probe_script(body: &str) -> HyprlandProbe {
    HyprlandProbe::command("/bin/sh", vec!["-c".into(), body.into(), "sh".into()])
}

fn elsewhere() -> LivePaths {
    LivePaths {
        data_dir: PathBuf::from("/nonexistent-live-user/.local/share/forge"),
        home: PathBuf::from("/nonexistent-live-user"),
    }
}

fn unused_port() -> u16 {
    StdTcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn process_is_gone(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => true,
        Ok(stat) => stat
            .rsplit_once(") ")
            .is_some_and(|(_, rest)| rest.starts_with('Z')),
    }
}

async fn wait_until_gone(pid: u32) {
    let polled = timeout(DEADLINE, async {
        while !process_is_gone(pid) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(polled.is_ok(), "process {pid} is still running");
}

async fn wait_for_line(process: &ForgeProcess, text: &str) {
    process
        .output()
        .wait_for(text, DEADLINE, |lines| {
            lines
                .iter()
                .any(|line| line.text.starts_with(text))
                .then_some(())
        })
        .await
        .unwrap();
}

/// Answers each connection's `auth` request, accepting only `TOKEN`, and reports every token seen.
fn serve_auth(listener: TcpListener) -> mpsc::UnboundedReceiver<String> {
    let (seen_tx, seen) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let seen_tx = seen_tx.clone();
            tokio::spawn(async move {
                let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
                    return;
                };
                while let Some(Ok(message)) = socket.next().await {
                    let Message::Text(text) = message else {
                        continue;
                    };
                    let request: Value = serde_json::from_str(text.as_str()).unwrap();
                    let token = request["token"].as_str().unwrap_or_default().to_owned();
                    let reply = if token == TOKEN {
                        json!({ "id": request["id"], "status": "ok" })
                    } else {
                        json!({ "id": request["id"], "status": "error",
                                "error": { "code": "auth_failed", "message": "bad token" } })
                    };
                    let _ = seen_tx.send(token);
                    if socket
                        .send(Message::Text(reply.to_string().into()))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            });
        }
    });
    seen
}

#[tokio::test]
async fn forge_exiting_before_readiness_reports_its_code_and_stderr_tail() {
    let stage = Stage::new();
    let mut process = stage.spawn("echo 'panicked at boot' >&2; exit 3").await;
    let failure = timeout(DEADLINE, process.wait_ready(unused_port(), TOKEN, DEADLINE))
        .await
        .unwrap();
    assert!(
        matches!(&failure, Err(EmulatorError::ForgeExited { code: Some(3), stderr_tail, .. })
            if stderr_tail.contains("panicked at boot")),
        "got {:?}",
        failure.map(drop)
    );
}

#[tokio::test]
async fn endpoint_override_refusal_exit_is_its_own_failure_class() {
    let stage = Stage::new();
    let mut process = stage
        .spawn(&format!("echo '{OVERRIDE_REFUSAL_LINE}'; exit 1"))
        .await;
    let failure = timeout(DEADLINE, process.wait_ready(unused_port(), TOKEN, DEADLINE))
        .await
        .unwrap();
    assert!(
        matches!(&failure, Err(EmulatorError::EndpointOverrideRefused { line }) if line == OVERRIDE_REFUSAL_LINE),
        "got {:?}",
        failure.map(drop)
    );
}

#[tokio::test]
async fn lost_server_port_race_is_reported_without_waiting_out_the_deadline() {
    let stage = Stage::new();
    let mut process = stage
        .spawn(&format!("echo '{BIND_FAILURE_LINE}' >&2; exec sleep 600"))
        .await;
    let failure = timeout(
        DEADLINE,
        process.wait_ready(unused_port(), TOKEN, Duration::from_secs(120)),
    )
    .await
    .expect("bind failure short-circuits readiness");
    assert!(
        matches!(failure, Err(EmulatorError::ServerPortTaken { .. })),
        "got {:?}",
        failure.map(drop)
    );
    process.shutdown(DEADLINE).await.unwrap();
}

#[tokio::test]
async fn forge_that_never_serves_times_out_at_the_readiness_deadline() {
    let stage = Stage::new();
    let mut process = stage.spawn("exec sleep 600").await;
    let readiness_deadline = Duration::from_millis(300);
    let failure = timeout(
        DEADLINE,
        process.wait_ready(unused_port(), TOKEN, readiness_deadline),
    )
    .await
    .unwrap();
    assert!(
        matches!(failure, Err(EmulatorError::ReadinessTimeout { waited, .. }) if waited == readiness_deadline),
        "got {:?}",
        failure.map(drop)
    );
    process.shutdown(DEADLINE).await.unwrap();
}

#[tokio::test]
async fn readiness_retries_until_the_control_server_authenticates() {
    let stage = Stage::new();
    let mut process = stage.spawn("echo booted; exec sleep 600").await;
    let port = unused_port();
    let output = process.output().clone();
    let waiting = tokio::spawn(async move {
        let ready = process.wait_ready(port, TOKEN, DEADLINE).await.map(drop);
        (process, ready)
    });
    output
        .wait_for("boot line", DEADLINE, |lines| {
            lines.iter().any(|line| line.text == "booted").then_some(())
        })
        .await
        .unwrap();
    let mut tokens = serve_auth(
        TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap(),
    );

    let (process, ready) = timeout(DEADLINE, waiting).await.unwrap().unwrap();
    assert!(ready.is_ok(), "got {ready:?}");
    assert_eq!(tokens.recv().await.as_deref(), Some(TOKEN));
    process.shutdown(DEADLINE).await.unwrap();
}

#[tokio::test]
async fn refused_authentication_ends_readiness_immediately() {
    let stage = Stage::new();
    let mut process = stage.spawn("exec sleep 600").await;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let _tokens = serve_auth(listener);
    let failure = timeout(
        DEADLINE,
        process.wait_ready(port, "stale-token", Duration::from_secs(120)),
    )
    .await
    .expect("an auth refusal is not retried");
    assert!(
        matches!(failure, Err(EmulatorError::AuthRefused { .. })),
        "got {:?}",
        failure.map(drop)
    );
    process.shutdown(DEADLINE).await.unwrap();
}

#[tokio::test]
async fn shutdown_terminates_forge_gracefully_and_reaps_it() {
    let stage = Stage::new();
    let process = stage.spawn("exec sleep 600").await;
    let pid = process.pid();
    let exit = process.shutdown(DEADLINE).await.unwrap();
    assert!(!exit.forced, "{exit:?}");
    assert!(process_is_gone(pid));
}

#[tokio::test]
async fn shutdown_kills_forge_that_ignores_terminate_once_grace_runs_out() {
    let stage = Stage::new();
    let process = stage
        .spawn("trap '' TERM; echo armed; exec sleep 600")
        .await;
    wait_for_line(&process, "armed").await;
    let pid = process.pid();
    let exit = timeout(DEADLINE, process.shutdown(Duration::from_millis(200)))
        .await
        .unwrap()
        .unwrap();
    assert!(exit.forced, "{exit:?}");
    assert!(process_is_gone(pid));
}

#[tokio::test]
async fn shutdown_also_stops_helpers_forge_spawned() {
    let stage = Stage::new();
    let process = stage.spawn("sleep 600 & echo \"helper $!\"; wait").await;
    wait_for_line(&process, "helper ").await;
    let helper: u32 = process
        .output()
        .lines()
        .iter()
        .find_map(|line| line.text.strip_prefix("helper ")?.parse().ok())
        .unwrap();
    process.shutdown(DEADLINE).await.unwrap();
    wait_until_gone(helper).await;
}

#[tokio::test]
async fn dropping_an_unreaped_forge_kills_it() {
    let stage = Stage::new();
    let process = stage.spawn("exec sleep 600").await;
    let pid = process.pid();
    drop(process);
    wait_until_gone(pid).await;
}

#[tokio::test]
async fn shutdown_after_forge_exited_on_its_own_returns_the_recorded_exit() {
    let stage = Stage::new();
    let mut process = stage.spawn("exit 4").await;
    let exited = timeout(DEADLINE, process.exited()).await.unwrap().unwrap();
    assert_eq!(exited.code, Some(4));
    let shut_down = process.shutdown(DEADLINE).await.unwrap();
    assert_eq!(shut_down, exited);
}

#[tokio::test]
async fn forge_environment_holds_only_allowlisted_and_launch_variables() {
    let stage = Stage::new();
    let dump = stage.file("env.txt");
    let mut process = stage.spawn(&format!("env > '{}'", dump.display())).await;
    timeout(DEADLINE, process.exited()).await.unwrap().unwrap();

    let dumped = std::fs::read_to_string(&dump).unwrap();
    let variables: Vec<(&str, &str)> = dumped
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(key, _)| key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .collect();
    let launch_set = [
        "FORGE_DATA_DIR",
        "HOME",
        "XDG_DATA_HOME",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
        "XDG_STATE_HOME",
        "LANG",
        "NO_COLOR",
        "RUST_BACKTRACE",
        "RUST_LOG",
        "FORGE_TWITCH_CLIENT_ID",
        "FORGE_TWITCH_API_BASE_URL",
    ];
    let shell_set = ["PWD", "OLDPWD", "SHLVL", "_"];
    let permitted: BTreeSet<&str> = INHERITED_VARIABLES
        .iter()
        .chain(&launch_set)
        .chain(&shell_set)
        .copied()
        .collect();
    let unexpected: Vec<&str> = variables
        .iter()
        .map(|(key, _)| *key)
        .filter(|key| !permitted.contains(key))
        .collect();
    assert!(unexpected.is_empty(), "leaked into forge: {unexpected:?}");

    let value = |name: &str| {
        variables
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, v)| *v)
    };
    assert_eq!(
        value("FORGE_DATA_DIR").map(Path::new),
        Some(stage.data.canonicalize().unwrap().as_path())
    );
    assert_eq!(value("HOME").map(Path::new), Some(stage.home.as_path()));
}

#[tokio::test]
async fn game_on_screen_or_unreadable_desktop_prevents_the_spawn() {
    for (probe, expect_game) in [
        (
            probe_script(r#"printf '[{"class":"steam_app_570","fullscreen":2}]'"#),
            true,
        ),
        (probe_script("echo 'no hyprland socket' >&2; exit 1"), false),
    ] {
        let stage = Stage::new();
        let marker = stage.file("spawned");
        let spec = stage.spec(stage.stub(&format!(": > '{}'", marker.display())));
        let refusal = ForgeProcess::spawn(&spec, &probe, &elsewhere()).await;
        match (&refusal, expect_game) {
            (Err(EmulatorError::GameInProgress { windows }), true) => {
                assert_eq!(windows, "steam_app_570");
            }
            (Err(EmulatorError::GameStateUnreadable { .. }), false) => {}
            _ => panic!("{probe:?}: got {:?}", refusal.map(|p| p.pid())),
        }
        assert!(!marker.exists(), "{probe:?}: stub forge ran");
    }
}

#[tokio::test]
async fn fixture_overlapping_the_live_data_directory_prevents_the_spawn() {
    let stage = Stage::new();
    let marker = stage.file("spawned");
    let spec = stage.spec(stage.stub(&format!(": > '{}'", marker.display())));
    let live = LivePaths {
        data_dir: stage.data.clone(),
        home: PathBuf::from("/nonexistent-live-user"),
    };
    let refusal = ForgeProcess::spawn(&spec, &clear_desktop(), &live).await;
    assert!(
        matches!(refusal, Err(EmulatorError::LiveDataDir { .. })),
        "got {:?}",
        refusal.map(|p| p.pid())
    );
    assert!(!marker.exists());
}
