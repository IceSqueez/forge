use std::path::Path;
use std::process::{Output, Stdio};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Child;

use crate::error::EspeakError;

const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(10);
const ESPEAK_BINARY: &str = "espeak-ng";
const DEFAULT_RATE_WPM: u32 = 175;
const DEFAULT_PITCH: u32 = 50;

pub(crate) struct SynthesisInvocation<'a> {
    pub voice_id: &'a str,
    pub text: &'a str,
    pub rate_wpm: u32,
    pub pitch_0_99: u32,
    pub ssml: bool,
}

fn timeout_error(timeout: Duration) -> EspeakError {
    EspeakError::Timeout {
        ms: u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
    }
}

pub(crate) fn check_espeak_version() -> Result<(), EspeakError> {
    let status = std::process::Command::new(ESPEAK_BINARY)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| EspeakError::BinaryNotFound)?;
    if status.success() {
        Ok(())
    } else {
        Err(EspeakError::BinaryNotFound)
    }
}

pub fn rate_wpm_from_multiplier(rate_multiplier: f32) -> u32 {
    (175.0_f32 * rate_multiplier).round().clamp(50.0, 450.0) as u32
}

pub fn pitch_from_semitones(pitch_semitones: f32) -> u32 {
    (50.0_f32 + pitch_semitones * (50.0 / 12.0_f32))
        .round()
        .clamp(0.0, 99.0) as u32
}

pub(crate) async fn list_voices_from_binary() -> Result<String, EspeakError> {
    list_voices_via(Path::new(ESPEAK_BINARY), SUBPROCESS_TIMEOUT).await
}

pub(crate) async fn list_voices_via(
    binary: &Path,
    timeout: Duration,
) -> Result<String, EspeakError> {
    let listing = tokio::process::Command::new(binary)
        .arg("--voices")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(timeout, listing)
        .await
        .map_err(|_| timeout_error(timeout))?
        .map_err(EspeakError::Io)?;
    if !output.status.success() {
        return Err(EspeakError::BinaryNotFound);
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(crate) async fn run_synthesis(
    invocation: &SynthesisInvocation<'_>,
) -> Result<Vec<u8>, EspeakError> {
    run_synthesis_via(Path::new(ESPEAK_BINARY), SUBPROCESS_TIMEOUT, invocation).await
}

pub(crate) async fn run_synthesis_via(
    binary: &Path,
    timeout: Duration,
    invocation: &SynthesisInvocation<'_>,
) -> Result<Vec<u8>, EspeakError> {
    let mut args: Vec<std::ffi::OsString> =
        vec!["--stdout".into(), "-v".into(), invocation.voice_id.into()];
    if invocation.rate_wpm != DEFAULT_RATE_WPM {
        args.push("-s".into());
        args.push(invocation.rate_wpm.to_string().into());
    }
    if invocation.pitch_0_99 != DEFAULT_PITCH {
        args.push("-p".into());
        args.push(invocation.pitch_0_99.to_string().into());
    }
    if invocation.ssml {
        args.push("-m".into());
    }

    let mut child = tokio::process::Command::new(binary)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(EspeakError::Io)?;

    let mut input = invocation.text.as_bytes().to_vec();
    if input.last() != Some(&b'\n') {
        input.push(b'\n');
    }

    let output = match tokio::time::timeout(timeout, exchange(&mut child, input)).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            kill_and_reap(&mut child).await;
            return Err(EspeakError::Io(e));
        }
        Err(_) => {
            kill_and_reap(&mut child).await;
            return Err(timeout_error(timeout));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(EspeakError::SubprocessFailed(stderr));
    }
    Ok(output.stdout)
}

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
        tracing::debug!(error = %e, "espeak-ng process already gone");
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn rate_wpm_maps_multiplier_with_clamping() {
        for (mult, expected) in [(1.0, 175), (0.5, 88), (2.0, 350), (0.25, 50), (4.0, 450)] {
            assert_eq!(rate_wpm_from_multiplier(mult), expected, "mult={mult}");
        }
    }

    #[test]
    fn pitch_maps_semitones_within_zero_to_ninety_nine() {
        for (semi, expected) in [(0.0, 50), (12.0, 99), (-12.0, 0), (6.0, 75), (-6.0, 25)] {
            assert_eq!(pitch_from_semitones(semi), expected, "semi={semi}");
        }
    }

    #[cfg(unix)]
    const LARGER_THAN_A_PIPE_BUFFER: usize = 1 << 20;

    #[cfg(unix)]
    fn piped_shell(script: &str) -> Child {
        tokio::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn /bin/sh")
    }

    #[cfg(unix)]
    fn patterned_input(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_exchange_survives_output_larger_than_a_pipe_buffer_before_input_is_read() {
        let input = patterned_input(LARGER_THAN_A_PIPE_BUFFER);
        let cases = [
            ("echoes stdin", "cat".to_owned(), input.clone()),
            (
                "floods stdout before reading stdin",
                format!("head -c {LARGER_THAN_A_PIPE_BUFFER} /dev/zero; cat > /dev/null"),
                vec![0_u8; LARGER_THAN_A_PIPE_BUFFER],
            ),
        ];
        for (name, script, expected_stdout) in cases {
            let mut child = piped_shell(&script);
            let exchanged =
                tokio::time::timeout(SUBPROCESS_TIMEOUT, exchange(&mut child, input.clone()))
                    .await
                    .unwrap_or_else(|_| panic!("{name}: the exchange deadlocked"))
                    .unwrap_or_else(|e| panic!("{name}: exchange failed with {e:?}"));

            assert!(
                exchanged.status.success(),
                "{name}: exit {}",
                exchanged.status
            );
            assert!(
                exchanged.stdout == expected_stdout,
                "{name}: stdout differs"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_espeak_that_exits_without_reading_its_input_still_reports_its_stderr() {
        let mut child = piped_shell("echo unknown voice >&2; exit 2");

        let exchanged = exchange(&mut child, patterned_input(LARGER_THAN_A_PIPE_BUFFER))
            .await
            .expect("a failed exit is an output, not an io error");

        assert_eq!(exchanged.status.code(), Some(2));
        assert!(
            String::from_utf8_lossy(&exchanged.stderr).contains("unknown voice"),
            "stderr was lost"
        );
    }
}
