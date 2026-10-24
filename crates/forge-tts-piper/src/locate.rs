use std::env::consts::EXE_SUFFIX;
use std::ffi::OsStr;
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

fn candidates_in(exe_dir: Option<&Path>, search_path: Option<&OsStr>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = exe_dir {
        candidates.push(dir.join(executable_name(BUNDLED_STEM)));
    }
    if let Some(search_path) = search_path {
        for stem in PATH_STEMS {
            let name = executable_name(stem);
            for dir in std::env::split_paths(search_path) {
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
        let exe = std::env::current_exe().ok();
        let search_path = std::env::var_os("PATH");
        Self::locate_in(
            exe.as_deref().and_then(Path::parent),
            search_path.as_deref(),
        )
        .await
    }

    pub async fn locate_in(exe_dir: Option<&Path>, search_path: Option<&OsStr>) -> Option<PathBuf> {
        for candidate in candidates_in(exe_dir, search_path) {
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
    fn candidates_put_the_bundled_binary_first_then_each_path_name_across_the_path() {
        let exe_dir = Path::new("bundle");
        let search_path =
            std::env::join_paths([Path::new("first"), Path::new("second")]).expect("join");
        let name = |dir: &str, stem: &str| Path::new(dir).join(format!("{stem}{EXE_SUFFIX}"));

        let found = candidates_in(Some(exe_dir), Some(search_path.as_os_str()));

        assert_eq!(
            found,
            vec![
                name("bundle", "piper"),
                name("first", "piper-tts"),
                name("second", "piper-tts"),
                name("first", "piper"),
                name("second", "piper"),
            ]
        );
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

    #[cfg(unix)]
    const PIPER_HELP: &str = "#!/bin/sh\necho 'usage: piper --model FILE'\n";

    #[cfg(unix)]
    fn piper_at(dir: &Path, file_name: &str) -> PathBuf {
        let path = dir.join(file_name);
        install_script(&path, PIPER_HELP);
        path
    }

    #[cfg(unix)]
    fn path_of(dirs: &[&Path]) -> std::ffi::OsString {
        std::env::join_paths(dirs).expect("join")
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_bundled_piper_wins_over_one_on_the_path() {
        let bundle = tempfile::tempdir().expect("tmpdir");
        let on_path = tempfile::tempdir().expect("tmpdir");
        let bundled = piper_at(bundle.path(), "piper");
        piper_at(on_path.path(), "piper-tts");

        let found =
            PiperEngine::locate_in(Some(bundle.path()), Some(&path_of(&[on_path.path()]))).await;

        assert_eq!(found, Some(bundled));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_path_binary_is_found_under_either_accepted_name() {
        for file_name in ["piper-tts", "piper"] {
            let bundle = tempfile::tempdir().expect("tmpdir");
            let on_path = tempfile::tempdir().expect("tmpdir");
            let expected = piper_at(on_path.path(), file_name);

            let found =
                PiperEngine::locate_in(Some(bundle.path()), Some(&path_of(&[on_path.path()])))
                    .await;

            assert_eq!(found, Some(expected), "{file_name}");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn binaries_under_other_names_are_never_picked() {
        let bundle = tempfile::tempdir().expect("tmpdir");
        let on_path = tempfile::tempdir().expect("tmpdir");
        for decoy in ["piper.sh", "piper-tts.bin", "piper_tts", "my-piper"] {
            piper_at(bundle.path(), decoy);
            piper_at(on_path.path(), decoy);
        }

        let found =
            PiperEngine::locate_in(Some(bundle.path()), Some(&path_of(&[on_path.path()]))).await;

        assert_eq!(found, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unusable_bundled_candidates_fall_through_to_the_path() {
        for (case, plant) in [
            ("directory named piper", "dir"),
            ("binary that is not piper", "impostor"),
        ] {
            let bundle = tempfile::tempdir().expect("tmpdir");
            let on_path = tempfile::tempdir().expect("tmpdir");
            let bundled = bundle.path().join("piper");
            if plant == "dir" {
                std::fs::create_dir(&bundled).expect("mkdir");
            } else {
                install_script(&bundled, "#!/bin/sh\necho 'usage: piper [--json] FILE'\n");
            }
            let expected = piper_at(on_path.path(), "piper");

            let found =
                PiperEngine::locate_in(Some(bundle.path()), Some(&path_of(&[on_path.path()])))
                    .await;

            assert_eq!(found, Some(expected), "{case}");
        }
    }

    #[tokio::test]
    async fn nothing_to_search_or_empty_dirs_locate_nothing() {
        let empty = tempfile::tempdir().expect("tmpdir");
        let search_path = std::env::join_paths([empty.path()]).expect("join");

        assert_eq!(PiperEngine::locate_in(None, None).await, None);
        assert_eq!(
            PiperEngine::locate_in(Some(empty.path()), Some(&search_path)).await,
            None
        );
    }
}
