use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use time::OffsetDateTime;

const LOG_PREFIX: &str = "forge.log";

/// Oldest first - the rolling appender's date suffix makes name order chronological.
pub fn files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_log_file(path))
        .collect();
    paths.sort();
    Ok(paths)
}

pub fn bundle(dir: &Path) -> Result<String, String> {
    let mut out = format!(
        "forge {}\nos: {}\narch: {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    );

    for path in files(dir)? {
        out.push_str(&format!("\n---- {} ----\n", path.display()));
        match fs::read_to_string(&path) {
            Ok(text) => out.push_str(&text),
            Err(e) => out.push_str(&format!("<unreadable: {e}>\n")),
        }
    }
    Ok(out)
}

pub fn clear(dir: &Path) -> Result<(), String> {
    let active = active_file_name();
    for path in files(dir)? {
        if path.file_name().and_then(|name| name.to_str()) == Some(active.as_str()) {
            // Windows refuses to unlink the file the log appender still holds open.
            OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn is_log_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(LOG_PREFIX))
}

fn active_file_name() -> String {
    let today = OffsetDateTime::now_utc().date();
    format!(
        "{LOG_PREFIX}.{:04}-{:02}-{:02}",
        today.year(),
        u8::from(today.month()),
        today.day(),
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct ScratchDir(PathBuf);

    impl ScratchDir {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "forge_qa_logs_{tag}_{}_{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            fs::remove_dir_all(&path).ok();
            fs::create_dir_all(&path).expect("create scratch dir");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, name: &str, contents: &[u8]) {
            fs::write(self.0.join(name), contents).expect("write fixture file");
        }

        fn absent_child(&self) -> PathBuf {
            self.0.join("no-such-subdir")
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

    fn file_names(dir: &Path) -> Vec<String> {
        files(dir)
            .expect("list log files")
            .into_iter()
            .map(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .expect("fixture names are utf-8")
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn files_lists_only_forge_log_regular_files_oldest_first() {
        let dir = ScratchDir::new("files");
        dir.write("forge.log.2026-08-01", b"newest");
        dir.write("other.txt", b"unrelated");
        dir.write("forge.log", b"undated");
        dir.write("app.log", b"a foreign log");
        dir.write("forge.log.2026-01-05", b"oldest");
        fs::create_dir(dir.path().join("forge.log.archive")).expect("create decoy directory");

        assert_eq!(
            file_names(dir.path()),
            vec![
                "forge.log".to_owned(),
                "forge.log.2026-01-05".to_owned(),
                "forge.log.2026-08-01".to_owned(),
            ],
        );
    }

    #[test]
    fn files_is_empty_for_a_missing_or_empty_directory() {
        let dir = ScratchDir::new("empty");

        for probe in [dir.path().to_owned(), dir.absent_child()] {
            assert_eq!(
                files(&probe).expect("listing must not fail"),
                Vec::<PathBuf>::new(),
                "probe {}",
                probe.display(),
            );
        }
    }

    #[test]
    fn bundle_attributes_each_log_file_to_its_name_oldest_first() {
        let dir = ScratchDir::new("bundle");
        dir.write("forge.log.2026-08-01", b"newest-marker");
        dir.write("forge.log.2026-01-05", b"oldest-marker");
        dir.write("other.txt", b"unrelated-marker");

        let out = bundle(dir.path()).expect("bundle");
        let at = |needle: &str| {
            out.find(needle)
                .unwrap_or_else(|| panic!("missing {needle}"))
        };

        assert!(
            at("forge.log.2026-01-05") < at("oldest-marker")
                && at("oldest-marker") < at("forge.log.2026-08-01")
                && at("forge.log.2026-08-01") < at("newest-marker"),
            "each file's name must precede its own contents, oldest first:\n{out}",
        );
        assert!(
            !out.contains("unrelated-marker"),
            "a non-log file must not be pulled into the bundle:\n{out}",
        );
    }

    #[test]
    fn bundle_marks_an_unreadable_file_inline_and_keeps_the_remaining_ones() {
        let dir = ScratchDir::new("unreadable");
        dir.write("forge.log.2026-01-05", &[0xff, 0xfe, 0x00, 0xff]);
        dir.write("forge.log.2026-08-01", b"still-included");

        let out = bundle(dir.path()).expect("an unreadable file must not fail the whole bundle");

        assert!(out.contains("<unreadable:"), "no inline marker:\n{out}");
        assert!(
            out.contains("still-included"),
            "the readable file must survive an unreadable neighbour:\n{out}",
        );
    }

    #[test]
    fn bundle_of_a_missing_directory_matches_the_bundle_of_an_empty_one() {
        let dir = ScratchDir::new("bundle_missing");

        assert_eq!(
            bundle(&dir.absent_child()).expect("a missing log directory is not an error"),
            bundle(dir.path()).expect("bundle of an empty directory"),
        );
    }

    #[test]
    fn clear_truncates_the_active_log_and_deletes_the_rotated_ones() {
        let dir = ScratchDir::new("clear_logs");
        let active = active_file_name();
        dir.write(&active, b"today's entries");
        dir.write("forge.log.2026-01-05", b"rotated");
        dir.write("forge.log", b"undated");

        clear(dir.path()).expect("clear");

        assert_eq!(
            fs::read_to_string(dir.path().join(&active)).expect(
                "the appender still holds the active log open - it must be truncated, not unlinked"
            ),
            "",
        );
        assert_eq!(file_names(dir.path()), vec![active]);
    }

    #[test]
    fn clear_leaves_files_outside_the_forge_log_family_untouched() {
        let dir = ScratchDir::new("clear_foreign");
        dir.write("app.log", b"a foreign log");
        dir.write("other.txt", b"unrelated");

        clear(dir.path()).expect("clear");

        for name in ["app.log", "other.txt"] {
            assert!(
                fs::read(dir.path().join(name)).is_ok(),
                "{name} was removed by clear",
            );
        }
    }

    #[test]
    fn clear_on_a_missing_directory_succeeds() {
        let dir = ScratchDir::new("clear_missing");

        assert!(clear(&dir.absent_child()).is_ok());
    }
}
