use std::env::consts::EXE_SUFFIX;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use crate::{MODEL_FLAG, PiperEngine};

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const HELP_FLAG: &str = "--help";
const BUNDLED_STEM: &str = "piper";
const PATH_STEMS: [&str; 2] = ["piper-tts", "piper"];

fn executable_name(stem: &str) -> String {
    format!("{stem}{EXE_SUFFIX}")
}

fn candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join(executable_name(BUNDLED_STEM)));
    }
    if let Some(search_path) = std::env::var_os("PATH") {
        for stem in PATH_STEMS {
            let name = executable_name(stem);
            for dir in std::env::split_paths(&search_path) {
                candidates.push(dir.join(&name));
            }
        }
    }
    candidates
}

async fn is_piper_tts(path: &Path) -> bool {
    let probe = tokio::process::Command::new(path)
        .arg(HELP_FLAG)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(PROBE_TIMEOUT, probe).await {
        Ok(Ok(out)) => {
            let mut text = out.stdout;
            text.extend_from_slice(&out.stderr);
            String::from_utf8_lossy(&text).contains(MODEL_FLAG)
        }
        _ => false,
    }
}

impl PiperEngine {
    pub async fn locate() -> Option<PathBuf> {
        for candidate in candidates() {
            if !candidate.is_file() {
                continue;
            }
            if is_piper_tts(&candidate).await {
                return Some(candidate);
            }
            tracing::info!(path = %candidate.display(), "ignoring a binary that is not piper TTS");
        }
        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_candidate_is_a_platform_executable_named_piper() {
        let found = candidates();

        assert!(!found.is_empty(), "no candidate next to the running binary");
        for candidate in found {
            let name = candidate
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            assert!(
                [BUNDLED_STEM, "piper-tts"]
                    .iter()
                    .any(|stem| name == format!("{stem}{EXE_SUFFIX}")),
                "unexpected candidate {}",
                candidate.display()
            );
        }
    }

    #[cfg(unix)]
    fn install_script(path: &Path, body: &str) {
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("printf '%s' \"$1\" > \"$0\" && chmod 755 \"$0\"")
            .arg(path)
            .arg(body)
            .status()
            .expect("run /bin/sh");
        assert!(status.success(), "could not install {}", path.display());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_probe_accepts_only_a_binary_whose_help_mentions_the_model_flag() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let cases = [
            (
                "help on stdout",
                "#!/bin/sh\necho 'usage: piper --model FILE'\n",
                true,
            ),
            (
                "help on stderr",
                "#!/bin/sh\necho 'usage: piper --model FILE' >&2\n",
                true,
            ),
            (
                "other program",
                "#!/bin/sh\necho 'usage: piper [--json] FILE'\n",
                false,
            ),
            ("silent program", "#!/bin/sh\nexit 0\n", false),
        ];
        for (name, body, accepted) in cases {
            let path = dir.path().join(name.replace(' ', "-"));
            install_script(&path, body);

            assert_eq!(is_piper_tts(&path).await, accepted, "{name}");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_probe_rejects_a_file_that_is_not_executable() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("piper");
        std::fs::write(&path, b"#!/bin/sh\necho --model\n").expect("write");

        assert!(!is_piper_tts(&path).await);
    }
}
