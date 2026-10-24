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
