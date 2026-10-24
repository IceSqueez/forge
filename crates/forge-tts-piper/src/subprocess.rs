use std::ffi::OsString;
use std::process::{Output, Stdio};

use forge_tts_core::TtsError;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Child;

use crate::PiperEngine;

async fn read_all(pipe: Option<impl AsyncRead + Unpin>) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if let Some(mut pipe) = pipe {
        pipe.read_to_end(&mut bytes).await?;
    }
    Ok(bytes)
}

async fn exchange(child: &mut Child, input: Vec<u8>) -> std::io::Result<Output> {
    let stdin = child.stdin.take();
    let write = async move {
        if let Some(mut stdin) = stdin {
            stdin.write_all(&input).await?;
        }
        Ok::<(), std::io::Error>(())
    };
    let (written, stdout, stderr) = tokio::join!(
        write,
        read_all(child.stdout.take()),
        read_all(child.stderr.take())
    );
    let status = child.wait().await?;
    if status.success() {
        written?;
    }
    Ok(Output {
        status,
        stdout: stdout?,
        stderr: stderr?,
    })
}

async fn kill_and_reap(child: &mut Child) {
    if let Err(e) = child.kill().await {
        tracing::debug!(error = %e, "piper process already gone");
    }
}

impl PiperEngine {
    pub(crate) async fn run_piper(
        &self,
        args: Vec<OsString>,
        input: Vec<u8>,
    ) -> Result<Output, TtsError> {
        let mut child = tokio::process::Command::new(&self.piper_binary)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(TtsError::Io)?;

        let outcome = tokio::time::timeout(self.timeout, exchange(&mut child, input)).await;
        match outcome {
            Ok(Ok(output)) => Ok(output),
            Ok(Err(e)) => {
                kill_and_reap(&mut child).await;
                Err(TtsError::Io(e))
            }
            Err(_) => {
                kill_and_reap(&mut child).await;
                Err(TtsError::Timeout {
                    ms: u64::try_from(self.timeout.as_millis()).unwrap_or(u64::MAX),
                })
            }
        }
    }
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use super::*;

    const LARGER_THAN_A_PIPE_BUFFER: usize = 1 << 20;
    const GENEROUS_TIMEOUT: Duration = Duration::from_secs(60);
    const SHORT_TIMEOUT: Duration = Duration::from_millis(50);

    fn shell_engine(voices_dir: &Path, timeout: Duration) -> PiperEngine {
        PiperEngine::new(PathBuf::from("/bin/sh"), voices_dir.to_path_buf(), timeout)
            .expect("/bin/sh exists")
    }

    fn shell_args(script: &str, marker: &Path) -> Vec<OsString> {
        vec!["-c".into(), script.into(), marker.into()]
    }

    fn patterned_input(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[cfg(target_os = "linux")]
    fn live_processes_mentioning(marker: &Path) -> usize {
        let needle = marker.to_string_lossy().into_owned();
        std::fs::read_dir("/proc")
            .expect("procfs")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().parse::<u32>().is_ok())
            .filter_map(|entry| std::fs::read(entry.path().join("cmdline")).ok())
            .filter(|cmdline| String::from_utf8_lossy(cmdline).contains(&needle))
            .count()
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn a_piper_that_never_finishes_times_out_and_is_killed() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let marker = dir.path().join("hung-piper-marker");
        std::fs::write(&marker, b"").expect("marker");
        let engine = shell_engine(dir.path(), SHORT_TIMEOUT);

        let result = engine
            .run_piper(
                shell_args("exec tail -f \"$0\"", &marker),
                b"hello\n".to_vec(),
            )
            .await;

        assert!(
            matches!(result, Err(TtsError::Timeout { ms: 50 })),
            "expected a timeout, got {result:?}"
        );
        assert_eq!(
            live_processes_mentioning(&marker),
            0,
            "the timed-out piper process is still alive"
        );
    }

    #[tokio::test]
    async fn the_exchange_survives_output_larger_than_a_pipe_buffer_before_input_is_read() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let marker = dir.path().join("marker");
        let input = patterned_input(LARGER_THAN_A_PIPE_BUFFER);
        let cases = [
            ("echoes stdin", "cat".to_owned(), input.clone()),
            (
                "floods stdout before reading stdin",
                format!("head -c {LARGER_THAN_A_PIPE_BUFFER} /dev/zero; cat > /dev/null"),
                vec![0_u8; LARGER_THAN_A_PIPE_BUFFER],
            ),
        ];
        let engine = shell_engine(dir.path(), GENEROUS_TIMEOUT);
        for (name, script, expected_stdout) in cases {
            let output = engine
                .run_piper(shell_args(&script, &marker), input.clone())
                .await
                .unwrap_or_else(|e| panic!("{name}: exchange failed with {e:?}"));

            assert!(output.status.success(), "{name}: exit {}", output.status);
            assert!(output.stdout == expected_stdout, "{name}: stdout differs");
        }
    }

    #[tokio::test]
    async fn a_piper_that_exits_without_reading_its_input_still_reports_its_stderr() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let marker = dir.path().join("marker");
        let engine = shell_engine(dir.path(), GENEROUS_TIMEOUT);

        let output = engine
            .run_piper(
                shell_args("echo voice model is corrupt >&2; exit 3", &marker),
                patterned_input(LARGER_THAN_A_PIPE_BUFFER),
            )
            .await
            .expect("a failed exit is an output, not an io error");

        assert_eq!(output.status.code(), Some(3));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("voice model is corrupt"),
            "stderr was lost"
        );
    }

    #[tokio::test]
    async fn a_binary_that_cannot_be_started_is_an_io_error() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let engine = PiperEngine::new(
            dir.path().to_path_buf(),
            dir.path().to_path_buf(),
            SHORT_TIMEOUT,
        )
        .expect("the directory exists");

        let result = engine.run_piper(Vec::new(), b"hello\n".to_vec()).await;

        assert!(matches!(result, Err(TtsError::Io(_))), "got {result:?}");
    }
}
