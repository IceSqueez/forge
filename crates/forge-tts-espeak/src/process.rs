use std::path::Path;
use std::process::{Output, Stdio};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Child;

use crate::error::EspeakError;

const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const ESPEAK_BINARY: &str = "espeak-ng";
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

pub(crate) fn check_espeak_version(binary: &Path) -> Result<(), EspeakError> {
    let status = std::process::Command::new(binary)
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

pub(crate) async fn list_voices_from_binary(binary: &Path) -> Result<String, EspeakError> {
    list_voices_via(binary, SUBPROCESS_TIMEOUT).await
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
        return Err(EspeakError::VoiceListingFailed(format!(
            "espeak-ng --voices exited with {}",
            output.status
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(crate) async fn run_synthesis(
    binary: &Path,
    invocation: &SynthesisInvocation<'_>,
) -> Result<Vec<u8>, EspeakError> {
    run_synthesis_via(binary, SUBPROCESS_TIMEOUT, invocation).await
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

    #[cfg(unix)]
    const GENEROUS_TIMEOUT: Duration = Duration::from_secs(60);

    const SHORT_TIMEOUT: Duration = Duration::from_millis(50);

    #[cfg(unix)]
    fn fake_espeak(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
        let path = dir.join("fake-espeak-ng");
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("printf '%s' \"$1\" > \"$0\" && chmod 755 \"$0\"")
            .arg(&path)
            .arg(format!("#!/bin/sh\n{body}\n"))
            .status()
            .expect("run /bin/sh");
        assert!(status.success(), "could not install {}", path.display());
        path
    }

    fn invocation(text: &str) -> SynthesisInvocation<'_> {
        SynthesisInvocation {
            voice_id: "en-us",
            text,
            rate_wpm: DEFAULT_RATE_WPM,
            pitch_0_99: DEFAULT_PITCH,
            ssml: false,
        }
    }

    #[cfg(target_os = "linux")]
    const BLOCKS_FOREVER: &str = "mkfifo \"$0.fifo\" && read -r _ < \"$0.fifo\"";

    #[cfg(target_os = "linux")]
    fn live_processes_mentioning(marker: &std::path::Path) -> usize {
        let needle = marker.to_string_lossy().into_owned();
        std::fs::read_dir("/proc")
            .expect("procfs")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().parse::<u32>().is_ok())
            .filter_map(|entry| std::fs::read(entry.path().join("cmdline")).ok())
            .filter(|cmdline| String::from_utf8_lossy(cmdline).contains(&needle))
            .count()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn synthesis_feeds_the_text_as_one_newline_terminated_line_and_returns_stdout() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let binary = fake_espeak(dir.path(), "cat");
        for (text, expected) in [
            ("hello", "hello\n"),
            ("hello\n", "hello\n"),
            ("", "\n"),
            ("привіт", "привіт\n"),
        ] {
            let out = run_synthesis_via(&binary, GENEROUS_TIMEOUT, &invocation(text))
                .await
                .unwrap_or_else(|e| panic!("{text:?}: {e:?}"));

            assert_eq!(String::from_utf8_lossy(&out), expected, "{text:?}");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn synthesis_passes_only_the_non_default_prosody_and_ssml_flags() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let binary = fake_espeak(dir.path(), "cat > /dev/null; printf '%s ' \"$@\"");
        for (rate_wpm, pitch_0_99, ssml, expected) in [
            (DEFAULT_RATE_WPM, DEFAULT_PITCH, false, "--stdout -v en-us "),
            (200, DEFAULT_PITCH, false, "--stdout -v en-us -s 200 "),
            (DEFAULT_RATE_WPM, 0, false, "--stdout -v en-us -p 0 "),
            (
                DEFAULT_RATE_WPM,
                DEFAULT_PITCH,
                true,
                "--stdout -v en-us -m ",
            ),
            (88, 99, true, "--stdout -v en-us -s 88 -p 99 -m "),
        ] {
            let call = SynthesisInvocation {
                rate_wpm,
                pitch_0_99,
                ssml,
                ..invocation("hi")
            };

            let out = run_synthesis_via(&binary, GENEROUS_TIMEOUT, &call)
                .await
                .expect("synthesis");

            assert_eq!(String::from_utf8_lossy(&out), expected);
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_failing_synthesis_reports_the_engines_stderr() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let binary = fake_espeak(
            dir.path(),
            "cat > /dev/null; echo unknown voice >&2; exit 1",
        );

        let result = run_synthesis_via(&binary, GENEROUS_TIMEOUT, &invocation("hi")).await;

        assert!(
            matches!(&result, Err(EspeakError::SubprocessFailed(stderr)) if stderr.contains("unknown voice")),
            "expected SubprocessFailed with stderr, got {result:?}"
        );
    }

    #[tokio::test]
    async fn a_binary_that_cannot_be_started_is_an_io_error_for_both_calls() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let missing = dir.path().join("no-such-espeak");

        let synthesis = run_synthesis_via(&missing, SHORT_TIMEOUT, &invocation("hi")).await;
        let listing = list_voices_via(&missing, SHORT_TIMEOUT).await;

        assert!(
            matches!(synthesis, Err(EspeakError::Io(_))),
            "{synthesis:?}"
        );
        assert!(matches!(listing, Err(EspeakError::Io(_))), "{listing:?}");
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn a_synthesis_that_never_finishes_times_out_and_is_killed() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let binary = fake_espeak(dir.path(), BLOCKS_FOREVER);

        let result = run_synthesis_via(&binary, SHORT_TIMEOUT, &invocation("hi")).await;

        assert!(
            matches!(result, Err(EspeakError::Timeout { ms: 50 })),
            "expected a timeout, got {result:?}"
        );
        assert_eq!(
            live_processes_mentioning(&binary),
            0,
            "the timed-out espeak-ng process is still alive"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_voice_listing_asks_for_voices_and_returns_stdout() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let binary = fake_espeak(
            dir.path(),
            "[ \"$1\" = --voices ] || exit 9; printf 'Pty Language Age/Gender VoiceName\\n 5  uk  --/M  Ukrainian\\n'",
        );

        let listing = list_voices_via(&binary, GENEROUS_TIMEOUT)
            .await
            .expect("listing");

        assert_eq!(
            listing,
            "Pty Language Age/Gender VoiceName\n 5  uk  --/M  Ukrainian\n"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_voice_listing_that_exits_with_failure_reports_the_listing_as_failed() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let binary = fake_espeak(dir.path(), "echo partial; exit 1");

        let result = list_voices_via(&binary, GENEROUS_TIMEOUT).await;

        assert!(
            matches!(&result, Err(EspeakError::VoiceListingFailed(detail)) if detail.contains("--voices")),
            "{result:?}"
        );
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn a_voice_listing_that_never_finishes_times_out_and_is_killed() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let binary = fake_espeak(dir.path(), BLOCKS_FOREVER);

        let result = list_voices_via(&binary, SHORT_TIMEOUT).await;

        assert!(
            matches!(result, Err(EspeakError::Timeout { ms: 50 })),
            "expected a timeout, got {result:?}"
        );
        tokio::time::timeout(GENEROUS_TIMEOUT, async {
            while live_processes_mentioning(&binary) > 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the timed-out espeak-ng process is still alive");
    }
}
