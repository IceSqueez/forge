use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::game_guard::GameGuard;
use super::group::{self, Stop};
use super::live_paths::LivePaths;
use super::output::{CapturedOutput, OutputStream};
use super::spec::LaunchSpec;
use crate::EmulatorError;
use crate::harness::control::{ClientTimeouts, ControlClient, ControlEndpoint, EventStream};

const CONTROL_HOST: &str = "127.0.0.1";
const TAIL_LINES: usize = 20;
const FIRST_RETRY: Duration = Duration::from_millis(25);
const RETRY_CEILING: Duration = Duration::from_millis(250);
const ATTEMPT_CEILING: Duration = Duration::from_secs(2);
const OUTPUT_DRAIN_DEADLINE: Duration = Duration::from_secs(2);
const KILL_REAP_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeExit {
    pub status: String,
    pub code: Option<i32>,
    pub forced: bool,
}

pub struct ForgeProcess {
    child: Option<Child>,
    pid: u32,
    data_dir: PathBuf,
    output: CapturedOutput,
    readers: Vec<JoinHandle<()>>,
    reaped: Option<ForgeExit>,
}

impl ForgeProcess {
    pub async fn spawn(
        spec: &LaunchSpec,
        guard: &GameGuard,
        live: &LivePaths,
    ) -> Result<Self, EmulatorError> {
        spec.validate()?;
        live.refuse_overlap(spec.data_dir.path(), &spec.home)?;
        std::fs::create_dir_all(&spec.home).map_err(|e| EmulatorError::InvalidLaunch {
            reason: format!("scratch home {}: {e}", spec.home.display()),
        })?;
        guard.ensure_no_game().await?;

        let mut command = Command::new(&spec.forge.program);
        command
            .args(&spec.forge.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        spec.configure_environment(&mut command, std::env::vars_os());

        let spawn_failed = |reason: String| EmulatorError::ForgeSpawn { reason };
        let mut child = command
            .spawn()
            .map_err(|e| spawn_failed(format!("{}: {e}", spec.forge.program.display())))?;
        let Some(pid) = child.id() else {
            return Err(spawn_failed("exited before its pid was read".to_owned()));
        };
        let output = CapturedOutput::new();
        let mut readers = Vec::with_capacity(2);
        if let Some(stdout) = child.stdout.take() {
            readers.push(tokio::spawn(
                output.clone().capture(OutputStream::Stdout, stdout),
            ));
        }
        if let Some(stderr) = child.stderr.take() {
            readers.push(tokio::spawn(
                output.clone().capture(OutputStream::Stderr, stderr),
            ));
        }
        Ok(Self {
            child: Some(child),
            pid,
            data_dir: spec.data_dir.path().to_owned(),
            output,
            readers,
            reaped: None,
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn output(&self) -> &CapturedOutput {
        &self.output
    }

    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join(forge_paths::LOGS_DIR_NAME)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.child.as_mut().map(Child::try_wait), Some(Ok(None)))
    }

    pub async fn wait_ready(
        &mut self,
        port: u16,
        token: &str,
        timeout: Duration,
    ) -> Result<(ControlClient, EventStream), EmulatorError> {
        let endpoint = ControlEndpoint::loopback(CONTROL_HOST, port)?;
        let deadline = Instant::now() + timeout;
        let mut changes = self.output.subscribe();
        let mut retry = FIRST_RETRY;
        loop {
            if let Some(status) = self.exit_status()? {
                return Err(self.early_exit(status).await);
            }
            if let Some((stop, line)) = self.output.boot_stop() {
                return Err(stop.into_error(line));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(self.readiness_timeout(timeout));
            }
            match authenticated_client(&endpoint, token, remaining.min(ATTEMPT_CEILING)).await {
                Ok(connected) => return Ok(connected),
                Err(e) if is_not_listening_yet(&e) => {}
                Err(e) => return Err(e),
            }

            let wake = (Instant::now() + retry).min(deadline);
            let output = self.output.clone();
            let child = self.child_mut()?;
            loop {
                tokio::select! {
                    _ = child.wait() => break,
                    _ = tokio::time::sleep_until(wake) => break,
                    changed = changes.changed() => {
                        if changed.is_err() || output.has_boot_stop() {
                            break;
                        }
                    }
                }
            }
            retry = (retry * 2).min(RETRY_CEILING);
        }
    }

    pub async fn exited(&mut self) -> Result<ForgeExit, EmulatorError> {
        if let Some(exit) = &self.reaped {
            return Ok(exit.clone());
        }
        let status = self
            .child_mut()?
            .wait()
            .await
            .map_err(|e| teardown(e.to_string()))?;
        Ok(self.reap(status, false).await)
    }

    pub async fn shutdown(mut self, grace: Duration) -> Result<ForgeExit, EmulatorError> {
        if let Some(exit) = self.reaped.take() {
            return Ok(exit);
        }
        let pid = self.pid;
        let child = self.child_mut()?;
        let (status, forced) = match child.try_wait().map_err(|e| teardown(e.to_string()))? {
            Some(status) => (status, false),
            None => {
                group::signal(child, pid, Stop::Terminate);
                match tokio::time::timeout(grace, child.wait()).await {
                    Ok(status) => (status.map_err(|e| teardown(e.to_string()))?, false),
                    Err(_) => {
                        group::signal(child, pid, Stop::Kill);
                        let status = tokio::time::timeout(KILL_REAP_DEADLINE, child.wait())
                            .await
                            .map_err(|_| teardown("forge survived SIGKILL".to_owned()))?
                            .map_err(|e| teardown(e.to_string()))?;
                        (status, true)
                    }
                }
            }
        };
        Ok(self.reap(status, forced).await)
    }

    async fn reap(&mut self, status: ExitStatus, forced: bool) -> ForgeExit {
        let exit = ForgeExit {
            status: status.to_string(),
            code: status.code(),
            forced,
        };
        self.child = None;
        self.reaped = Some(exit.clone());
        self.drain_output().await;
        exit
    }

    fn child_mut(&mut self) -> Result<&mut Child, EmulatorError> {
        self.child
            .as_mut()
            .ok_or_else(|| teardown("forge was already reaped".to_owned()))
    }

    fn exit_status(&mut self) -> Result<Option<ExitStatus>, EmulatorError> {
        self.child_mut()?
            .try_wait()
            .map_err(|e| teardown(e.to_string()))
    }

    async fn early_exit(&mut self, status: ExitStatus) -> EmulatorError {
        self.reap(status, false).await;
        classify_exit(status, &self.output)
    }

    fn readiness_timeout(&self, waited: Duration) -> EmulatorError {
        EmulatorError::ReadinessTimeout {
            waited,
            stderr_tail: self
                .output
                .tail(OutputStream::Stderr, TAIL_LINES)
                .join("\n"),
        }
    }

    async fn drain_output(&mut self) {
        for mut reader in std::mem::take(&mut self.readers) {
            if tokio::time::timeout(OUTPUT_DRAIN_DEADLINE, &mut reader)
                .await
                .is_err()
            {
                reader.abort();
            }
        }
    }
}

impl Drop for ForgeProcess {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut()
            && matches!(child.try_wait(), Ok(None))
        {
            group::signal(child, self.pid, Stop::Kill);
        }
        for reader in &self.readers {
            reader.abort();
        }
    }
}

async fn authenticated_client(
    endpoint: &ControlEndpoint,
    token: &str,
    budget: Duration,
) -> Result<(ControlClient, EventStream), EmulatorError> {
    let timeouts = ClientTimeouts {
        connect: budget,
        request: budget,
    };
    let (client, events) = ControlClient::connect(endpoint, timeouts).await?;
    client.authenticate(token).await?;
    Ok((client, events))
}

fn is_not_listening_yet(error: &EmulatorError) -> bool {
    matches!(
        error,
        EmulatorError::Connect { .. }
            | EmulatorError::ConnectTimeout
            | EmulatorError::ConnectionClosed
            | EmulatorError::RequestTimeout { .. }
    )
}

fn classify_exit(status: ExitStatus, output: &CapturedOutput) -> EmulatorError {
    if status.code() == Some(1)
        && let Some((stop, line)) = output.boot_stop()
        && stop.is_startup_refusal()
    {
        return stop.into_error(line);
    }
    EmulatorError::ForgeExited {
        status: status.to_string(),
        code: status.code(),
        stderr_tail: output.tail(OutputStream::Stderr, TAIL_LINES).join("\n"),
        stdout_tail: output.tail(OutputStream::Stdout, TAIL_LINES).join("\n"),
    }
}

fn teardown(reason: String) -> EmulatorError {
    EmulatorError::ForgeTeardown { reason }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::process::ExitStatusExt;

    use super::*;

    const OVERRIDE_LINE: &str = "2026-10-28T10:00:00.000000Z ERROR platform endpoint override refused; exiting signal=startup_refused outcome=endpoint_override_refused variable=FORGE_TWITCH_API_BASE_URL error=endpoint override FORGE_TWITCH_API_BASE_URL refused: host is not 127.0.0.0/8, ::1 or localhost";
    const ALREADY_RUNNING_LINE: &str = "2026-10-28T10:00:00.000000Z ERROR another forge instance is already running for this data directory; exiting signal=startup_refused outcome=already_running";
    const BOOT_FAILED_LINE: &str = "2026-10-28T10:00:00.000000Z ERROR boot failed signal=readiness outcome=failed failure=pre_baseline";

    fn exit_code(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }

    fn output_with(line: &str) -> CapturedOutput {
        let output = CapturedOutput::new();
        output.push(OutputStream::Stdout, line);
        output.push(OutputStream::Stderr, "stderr detail");
        output
    }

    #[test]
    fn exit_one_after_an_override_refusal_names_the_refused_variable() {
        let classified = classify_exit(exit_code(1), &output_with(OVERRIDE_LINE));
        assert!(
            matches!(&classified, EmulatorError::EndpointOverrideRefused { variable, line }
                if variable == "FORGE_TWITCH_API_BASE_URL" && line == OVERRIDE_LINE),
            "got {classified:?}"
        );
    }

    #[test]
    fn exit_one_after_an_already_running_refusal_is_its_own_failure_class() {
        let classified = classify_exit(exit_code(1), &output_with(ALREADY_RUNNING_LINE));
        assert!(
            matches!(&classified, EmulatorError::AlreadyRunning { line } if line == ALREADY_RUNNING_LINE),
            "got {classified:?}"
        );
    }

    #[test]
    fn other_exits_are_early_exits_carrying_code_and_tails() {
        for (status, line) in [
            (exit_code(2), OVERRIDE_LINE),
            (ExitStatus::from_raw(9), ALREADY_RUNNING_LINE),
            (exit_code(1), BOOT_FAILED_LINE),
            (
                exit_code(1),
                "2026-10-28T10:00:00.000000Z ERROR platform endpoint override refused; exiting error=endpoint override FORGE_TWITCH_API_BASE_URL refused: malformed",
            ),
        ] {
            let classified = classify_exit(status, &output_with(line));
            assert!(
                matches!(&classified, EmulatorError::ForgeExited { code, stderr_tail, stdout_tail, .. }
                    if *code == status.code() && stderr_tail == "stderr detail" && stdout_tail == line),
                "{status}: got {classified:?}"
            );
        }
    }

    #[test]
    fn transient_connection_failures_are_retried_and_refusals_are_not() {
        for (error, retried) in [
            (
                EmulatorError::Connect {
                    reason: "refused".to_owned(),
                },
                true,
            ),
            (EmulatorError::ConnectTimeout, true),
            (EmulatorError::ConnectionClosed, true),
            (EmulatorError::RequestTimeout { request: "auth" }, true),
            (
                EmulatorError::AuthRefused {
                    message: "bad token".to_owned(),
                },
                false,
            ),
            (
                EmulatorError::UnexpectedResponse {
                    request: "auth",
                    reason: "shape".to_owned(),
                },
                false,
            ),
        ] {
            assert_eq!(is_not_listening_yet(&error), retried, "{error:?}");
        }
    }
}
