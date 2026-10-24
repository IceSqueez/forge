use std::process::{Output, Stdio};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Child;

use crate::error::EspeakError;

const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(10);

fn timeout_error() -> EspeakError {
    EspeakError::Timeout {
        ms: u64::try_from(SUBPROCESS_TIMEOUT.as_millis()).unwrap_or(u64::MAX),
    }
}

pub(crate) fn check_espeak_version() -> Result<(), EspeakError> {
    let status = std::process::Command::new("espeak-ng")
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
    let listing = tokio::process::Command::new("espeak-ng")
        .arg("--voices")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(SUBPROCESS_TIMEOUT, listing)
        .await
        .map_err(|_| timeout_error())?
        .map_err(EspeakError::Io)?;

    if !output.status.success() {
        return Err(EspeakError::BinaryNotFound);
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(crate) async fn run_synthesis(
    voice_id: &str,
    text: &str,
    rate_wpm: u32,
    pitch_0_99: u32,
    ssml: bool,
) -> Result<Vec<u8>, EspeakError> {
    let mut args: Vec<std::ffi::OsString> = vec!["--stdout".into(), "-v".into(), voice_id.into()];

    if rate_wpm != 175 {
        args.push("-s".into());
        args.push(rate_wpm.to_string().into());
    }

    if pitch_0_99 != 50 {
        args.push("-p".into());
        args.push(pitch_0_99.to_string().into());
    }

    if ssml {
        args.push("-m".into());
    }

    let mut child = tokio::process::Command::new("espeak-ng")
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(EspeakError::Io)?;

    let mut input = text.as_bytes().to_vec();
    if input.last() != Some(&b'\n') {
        input.push(b'\n');
    }

    let output = match tokio::time::timeout(SUBPROCESS_TIMEOUT, exchange(&mut child, input)).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            kill_and_reap(&mut child).await;
            return Err(EspeakError::Io(e));
        }
        Err(_) => {
            kill_and_reap(&mut child).await;
            return Err(timeout_error());
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
}
