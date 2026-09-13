use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::data_dir::ForgeDataDir;
use super::report::SeedReport;
use super::spec::Fixture;
use crate::EmulatorError;

const SEED_SUBCOMMAND: &str = "seed";
const SEED_DEADLINE: Duration = Duration::from_secs(60);

/// Returns only after the seeder process has exited, so nothing it opened still holds the database.
pub async fn seed(
    emulator_program: &Path,
    data_dir: &Path,
    fixture: &Fixture,
) -> Result<SeedReport, EmulatorError> {
    fixture.validate()?;
    let data_dir = ForgeDataDir::fresh(data_dir)?;
    let failed = |reason: String| EmulatorError::SeederProcess { reason };
    let input = serde_json::to_vec(fixture).map_err(|e| failed(e.to_string()))?;

    // Why: forge resolves the credential key from its environment alone; a child process gets
    // exactly forge's environment without this process mutating its own.
    let mut command = Command::new(emulator_program);
    command
        .arg(SEED_SUBCOMMAND)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    data_dir.configure(&mut command);
    let mut child = command.spawn().map_err(|e| failed(e.to_string()))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| failed("stdin was not captured".to_owned()))?;
    let run = async move {
        stdin.write_all(&input).await?;
        drop(stdin);
        child.wait_with_output().await
    };
    let output = tokio::time::timeout(SEED_DEADLINE, run)
        .await
        .map_err(|_| failed(format!("no exit within {}s", SEED_DEADLINE.as_secs())))?
        .map_err(|e| failed(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(failed(format!("{}: {}", output.status, stderr.trim())));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| failed(format!("unreadable seed report: {e}")))
}
