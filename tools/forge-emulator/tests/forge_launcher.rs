//! Launch retries against `/bin/sh` stub children and the real seeder. Never starts the real forge.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use forge_emulator::EmulatorError;
use forge_emulator::fixture::Fixture;
use forge_emulator::launch::{
    DEFAULT_LOG_DIRECTIVES, ForgeCommand, GameGuard, HyprlandProbe, LaunchOptions, LivePaths,
    launch_forge,
};
use tempfile::TempDir;
use tokio::time::timeout;

const EMULATOR: &str = env!("CARGO_BIN_EXE_forge-emulator");
const DEADLINE: Duration = Duration::from_secs(60);
const LOST_RACE: &str = "echo 'forge-desktop: server failed to start, leaving it off: could not bind to 127.0.0.1:40000: Address already in use (os error 98)' >&2";

fn options(root: &TempDir, stub_body: &str, max_attempts: u32) -> LaunchOptions {
    let script = root.path().join("stub-forge.sh");
    std::fs::write(&script, stub_body).unwrap();
    LaunchOptions {
        emulator: PathBuf::from(EMULATOR),
        forge: ForgeCommand {
            program: PathBuf::from("/bin/sh"),
            args: vec![script.into_os_string()],
        },
        run_root: root.path().join("run"),
        fixture: Fixture::chat_command_mvp(),
        endpoint_overrides: Vec::new(),
        log_directives: DEFAULT_LOG_DIRECTIVES.to_owned(),
        guard: GameGuard::probing(HyprlandProbe::command(
            "/bin/sh",
            vec!["-c".into(), "printf '[]'".into(), "sh".into()],
        )),
        live: LivePaths {
            data_dir: PathBuf::from("/nonexistent-live-user/.local/share/forge"),
            home: PathBuf::from("/nonexistent-live-user"),
        },
        ready_timeout: DEADLINE,
        max_attempts,
        shutdown_grace: Duration::from_secs(5),
    }
}

fn seeded(run_root: &Path, attempt: u32) -> bool {
    run_root
        .join(format!("attempt-{attempt}/data/forge.db"))
        .exists()
}

fn process_is_gone(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => true,
        Ok(stat) => stat
            .rsplit_once(") ")
            .is_some_and(|(_, rest)| rest.starts_with('Z')),
    }
}

#[tokio::test]
async fn lost_port_race_relaunches_on_a_freshly_seeded_directory() {
    let root = tempfile::tempdir().unwrap();
    let stub = format!(
        r#"run="$FORGE_DATA_DIR/../.."
if [ -e "$run/first.pid" ]; then echo 'second boot' >&2; exit 9; fi
echo $$ > "$run/first.pid"
{LOST_RACE}
exec sleep 600"#
    );
    let options = options(&root, &stub, 3);
    let failure = timeout(DEADLINE, launch_forge(&options)).await.unwrap();

    assert!(
        matches!(&failure, Err(EmulatorError::ForgeExited { code: Some(9), stderr_tail, .. })
            if stderr_tail.contains("second boot")),
        "got {:?}",
        failure.map(|launched| launched.attempts)
    );
    assert!(seeded(&options.run_root, 1) && seeded(&options.run_root, 2));
    assert!(!options.run_root.join("attempt-3").exists());
    let first: u32 = std::fs::read_to_string(options.run_root.join("first.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(process_is_gone(first), "first attempt's forge survived");
}

#[tokio::test]
async fn port_race_lost_on_every_attempt_stops_at_the_attempt_bound() {
    let root = tempfile::tempdir().unwrap();
    let options = options(&root, &format!("{LOST_RACE}\nexec sleep 600"), 2);
    let failure = timeout(DEADLINE, launch_forge(&options)).await.unwrap();

    assert!(
        matches!(failure, Err(EmulatorError::ServerPortTaken { attempts: 2 })),
        "got {:?}",
        failure.map(|launched| launched.attempts)
    );
    assert!(seeded(&options.run_root, 2));
    assert!(!options.run_root.join("attempt-3").exists());
}

#[tokio::test]
async fn zero_launch_attempts_is_refused_before_anything_is_seeded() {
    let root = tempfile::tempdir().unwrap();
    let options = options(&root, "exit 0", 0);
    let failure = launch_forge(&options).await;

    assert!(
        matches!(failure, Err(EmulatorError::InvalidLaunch { .. })),
        "got {:?}",
        failure.map(|launched| launched.attempts)
    );
    assert!(!options.run_root.exists());
}
