use std::process::Stdio;

use tokio::io::AsyncWriteExt;

use crate::error::EspeakError;

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

/// Maps a rate multiplier to words-per-minute for the `-s` flag.
///
/// eSpeak-NG default is 175 wpm. The result is clamped to the supported
/// range [50, 450]. Returns 175 for a multiplier of 1.0, in which case the
/// caller should omit the `-s` flag entirely.
pub fn rate_wpm_from_multiplier(rate_multiplier: f32) -> u32 {
    (175.0_f32 * rate_multiplier).round().clamp(50.0, 450.0) as u32
}

/// Maps semitone shift to the 0-99 range used by eSpeak-NG's `-p` flag.
///
/// The neutral value is 50. Returns 50 for 0.0 semitones, in which case the
/// caller should omit the `-p` flag entirely.
pub fn pitch_from_semitones(pitch_semitones: f32) -> u32 {
    (50.0_f32 + pitch_semitones * (50.0 / 12.0_f32))
        .round()
        .clamp(0.0, 99.0) as u32
}

pub(crate) async fn list_voices_from_binary() -> Result<String, EspeakError> {
    let output = tokio::process::Command::new("espeak-ng")
        .arg("--voices")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
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
        .spawn()
        .map_err(EspeakError::Io)?;

    if let Some(mut stdin) = child.stdin.take() {
        let mut bytes = text.as_bytes().to_vec();
        if bytes.last() != Some(&b'\n') {
            bytes.push(b'\n');
        }
        stdin.write_all(&bytes).await.map_err(EspeakError::Io)?;
    }

    let output = child.wait_with_output().await.map_err(EspeakError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(EspeakError::SubprocessFailed(stderr));
    }

    Ok(output.stdout)
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
