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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corpus {
    pub text: String,
    /// Log bytes the budget could not carry; the newest bytes are the ones kept.
    pub elided_bytes: u64,
    pub files_kept: usize,
}

/// Fills `budget` from the newest file backwards, so a corpus far larger than the budget is never
/// held in memory whole.
pub fn corpus(dir: &Path, budget: usize) -> Result<Corpus, String> {
    let mut remaining = budget;
    let mut elided_bytes = 0u64;
    let mut blocks: Vec<String> = Vec::new();

    for path in files(dir)?.iter().rev() {
        let body = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) => format!("<unreadable: {e}>\n"),
        };
        // The name alone attributes the lines; the directory above it is the OS account's.
        let name = path.file_name().unwrap_or(path.as_os_str());
        let header = format!("\n---- {} ----\n", name.to_string_lossy());

        if remaining <= header.len() {
            elided_bytes += body.len() as u64;
            continue;
        }

        let room = remaining - header.len();
        if body.len() <= room {
            remaining -= header.len() + body.len();
            blocks.push(header + body.as_str());
        } else {
            let start = line_start_at_or_after(&body, body.len() - room);
            elided_bytes += start as u64;
            remaining = 0;
            blocks.push(header + &body[start..]);
        }
    }

    blocks.reverse();
    Ok(Corpus {
        files_kept: blocks.len(),
        text: blocks.concat(),
        elided_bytes,
    })
}

/// Keeps the tail whole-line and on a character boundary, so a truncated corpus never opens
/// mid-record or mid-codepoint.
pub fn line_start_at_or_after(text: &str, from: usize) -> usize {
    let mut idx = from.min(text.len());
    while idx < text.len() && !text.is_char_boundary(idx) {
        idx += 1;
    }
    match text[idx..].find('\n') {
        Some(offset) => idx + offset + 1,
        None => text.len(),
    }
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
    fn corpus_attributes_each_log_file_to_its_name_oldest_first() {
        let dir = ScratchDir::new("bundle");
        dir.write("forge.log.2026-08-01", b"newest-marker");
        dir.write("forge.log.2026-01-05", b"oldest-marker");
        dir.write("other.txt", b"unrelated-marker");

        let out = corpus(dir.path(), usize::MAX).expect("corpus").text;
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
            "a non-log file must not be pulled into the corpus:\n{out}",
        );
    }

    /// Why: R4 - no literal names in the bundle, and the export statement promises the reporter
    /// that file paths are withheld. The log directory sits under the OS account's data dir, so
    /// an absolute path in a section header publishes the account name to a public issue.
    #[test]
    fn corpus_names_each_file_without_disclosing_the_directory_holding_it() {
        let dir = ScratchDir::new("nova_the_broadcaster");
        dir.write("forge.log.2026-08-01", b"body\n");

        let out = corpus(dir.path(), usize::MAX).expect("corpus").text;

        assert!(
            out.contains("forge.log.2026-08-01"),
            "the file name still attributes its lines:\n{out}",
        );
        assert!(
            !out.contains("nova_the_broadcaster"),
            "the enclosing directory reached the corpus:\n{out}",
        );
    }

    #[test]
    fn corpus_marks_an_unreadable_file_inline_and_keeps_the_remaining_ones() {
        let dir = ScratchDir::new("unreadable");
        dir.write("forge.log.2026-01-05", &[0xff, 0xfe, 0x00, 0xff]);
        dir.write("forge.log.2026-08-01", b"still-included");

        let out = corpus(dir.path(), usize::MAX)
            .expect("an unreadable file must not fail the whole corpus")
            .text;

        assert!(out.contains("<unreadable:"), "no inline marker:\n{out}");
        assert!(
            out.contains("still-included"),
            "the readable file must survive an unreadable neighbour:\n{out}",
        );
    }

    #[test]
    fn corpus_of_a_missing_directory_matches_the_corpus_of_an_empty_one() {
        let dir = ScratchDir::new("bundle_missing");

        assert_eq!(
            corpus(&dir.absent_child(), usize::MAX)
                .expect("a missing log directory is not an error"),
            corpus(dir.path(), usize::MAX).expect("corpus of an empty directory"),
        );
    }

    /// The header framing is produced by `corpus` itself; measuring it from an unbudgeted run
    /// keeps the fixture from restating the production format.
    fn framing_len(dir: &ScratchDir, bodies: usize) -> usize {
        corpus(dir.path(), usize::MAX).expect("corpus").text.len() - bodies
    }

    #[test]
    fn corpus_fills_the_budget_from_the_newest_file_backwards() {
        let dir = ScratchDir::new("budget_newest");
        let old = "old-line\n";
        let new = "new-line\n";
        dir.write("forge.log.2026-01-05", old.as_bytes());
        dir.write("forge.log.2026-08-01", new.as_bytes());
        let framing = framing_len(&dir, old.len() + new.len());

        // Room for both headers and the newest body, one byte short of the oldest body.
        let out = corpus(dir.path(), framing + new.len()).expect("corpus");

        assert!(
            out.text.contains("new-line"),
            "newest body dropped:\n{out:?}"
        );
        assert!(
            !out.text.contains("old-line"),
            "the oldest body must be the one left out:\n{out:?}",
        );
        assert_eq!(out.files_kept, 1);
        assert_eq!(out.elided_bytes, old.len() as u64);
    }

    /// Why: the corpus is cut by byte budget, and a Cyrillic or emoji log line straddles the cut.
    /// Slicing at the raw offset would panic; the tail must resume at the next whole line.
    #[test]
    fn corpus_cuts_a_partial_file_on_a_line_start_past_a_multibyte_character() {
        let dir = ScratchDir::new("budget_multibyte");
        let body = "a\nПривіт\nz\n";
        dir.write("forge.log.2026-08-01", body.as_bytes());
        let framing = framing_len(&dir, body.len());

        // Leaves room for 12 of the 17 body bytes, so the cut lands inside `р`.
        let out = corpus(dir.path(), framing + 12).expect("corpus");

        assert!(out.text.ends_with("z\n"), "tail lost:\n{out:?}");
        assert!(
            !out.text.contains('П') && !out.text.contains('и'),
            "a partially-cut line survived:\n{out:?}",
        );
        assert_eq!(
            out.elided_bytes,
            (body.len() - "z\n".len()) as u64,
            "elided bytes must count everything before the surviving line",
        );
        assert_eq!(out.files_kept, 1);
    }

    #[test]
    fn corpus_of_a_budget_too_small_for_any_framing_keeps_no_file() {
        let dir = ScratchDir::new("budget_zero");
        dir.write("forge.log.2026-08-01", b"body\n");

        let out = corpus(dir.path(), 0).expect("corpus");

        assert_eq!(out.text, "");
        assert_eq!(out.files_kept, 0);
        assert_eq!(out.elided_bytes, 5);
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
