use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::log_record::LogRecord;

const LOG_FILE_PREFIX: &str = "forge.log";
const TAIL_WINDOW_BYTES: u64 = 64 * 1024;

/// Follows forge's rotating log files, yielding only complete lines written after it started.
#[derive(Debug, Clone)]
pub(crate) struct LogTail {
    dir: PathBuf,
    consumed: BTreeMap<PathBuf, u64>,
}

impl LogTail {
    /// A line still being written when the tail starts is read in full once it ends.
    pub(crate) fn from_now(dir: &Path) -> Self {
        let consumed = log_files(dir)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|file| {
                let complete = complete_prefix_len(&file).ok()?;
                Some((file, complete))
            })
            .collect();
        Self {
            dir: dir.to_owned(),
            consumed,
        }
    }

    pub(crate) fn from_start(dir: &Path) -> Self {
        Self {
            dir: dir.to_owned(),
            consumed: BTreeMap::new(),
        }
    }

    pub(crate) fn files(&self) -> Vec<PathBuf> {
        self.consumed.keys().cloned().collect()
    }

    pub(crate) fn poll(&mut self) -> io::Result<Vec<LogRecord>> {
        let mut records = Vec::new();
        for file in log_files(&self.dir)? {
            let offset = self.consumed.get(&file).copied().unwrap_or(0);
            let (lines, consumed) = read_complete_lines(&file, offset)?;
            records.extend(
                lines
                    .iter()
                    .filter_map(|line| LogRecord::parse(file.clone(), line)),
            );
            self.consumed.insert(file, consumed);
        }
        Ok(records)
    }
}

/// The last `count` complete lines of the newest log file.
pub(crate) fn newest_lines(dir: &Path, count: usize) -> Vec<String> {
    let Some(newest) = log_files(dir)
        .ok()
        .and_then(|files| files.into_iter().last())
    else {
        return Vec::new();
    };
    let Ok(len) = std::fs::metadata(&newest).map(|meta| meta.len()) else {
        return Vec::new();
    };
    let start = len.saturating_sub(TAIL_WINDOW_BYTES);
    let Ok((mut lines, _)) = read_complete_lines(&newest, start) else {
        return Vec::new();
    };
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let skip = lines.len().saturating_sub(count);
    lines.split_off(skip)
}

/// Sorted by name, which the rotation's date suffix makes chronological.
fn log_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        let named_log = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(LOG_FILE_PREFIX));
        if named_log && entry.file_type()?.is_file() {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

fn complete_prefix_len(file: &Path) -> io::Result<u64> {
    let mut handle = File::open(file)?;
    let len = handle.metadata()?.len();
    let start = len.saturating_sub(TAIL_WINDOW_BYTES);
    handle.seek(SeekFrom::Start(start))?;
    let mut tail = Vec::new();
    handle.read_to_end(&mut tail)?;
    Ok(match tail.iter().rposition(|byte| *byte == b'\n') {
        Some(newline) => start + newline as u64 + 1,
        None => start,
    })
}

/// A file that shrank below `offset` was replaced, so it is read from its start.
fn read_complete_lines(file: &Path, offset: u64) -> io::Result<(Vec<String>, u64)> {
    let mut handle = match File::open(file) {
        Ok(handle) => handle,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), offset)),
        Err(e) => return Err(e),
    };
    let len = handle.metadata()?.len();
    let offset = if len < offset { 0 } else { offset };
    handle.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    handle.read_to_end(&mut bytes)?;
    let Some(last_newline) = bytes.iter().rposition(|byte| *byte == b'\n') else {
        return Ok((Vec::new(), offset));
    };
    let complete = &bytes[..last_newline];
    let lines = String::from_utf8_lossy(complete)
        .split('\n')
        .map(|line| line.trim_end_matches('\r').to_owned())
        .collect();
    Ok((lines, offset + last_newline as u64 + 1))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::io::Write;

    use super::*;

    const TODAY: &str = "forge.log.2026-09-13";
    const TOMORROW: &str = "forge.log.2026-09-14";

    fn line(reason: &str) -> String {
        format!("2026-09-13T10:00:00Z DEBUG forge::trigger: declined reason={reason}\n")
    }

    fn append(dir: &Path, file: &str, text: &str) {
        let mut handle = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(file))
            .unwrap();
        handle.write_all(text.as_bytes()).unwrap();
    }

    fn reasons(records: &[LogRecord]) -> Vec<String> {
        records
            .iter()
            .map(|record| record.fields["reason"].clone())
            .collect()
    }

    #[test]
    fn tail_from_now_reads_only_lines_appended_after_it_started() {
        let dir = tempfile::tempdir().unwrap();
        append(dir.path(), TODAY, &line("before"));
        let mut tail = LogTail::from_now(dir.path());
        append(dir.path(), TODAY, &line("after"));

        assert_eq!(reasons(&tail.poll().unwrap()), ["after"]);
        assert!(tail.poll().unwrap().is_empty());
    }

    #[test]
    fn a_line_unfinished_when_the_tail_starts_is_read_whole_once_it_ends() {
        let dir = tempfile::tempdir().unwrap();
        append(dir.path(), TODAY, &line("before"));
        let unfinished = line("straddling");
        let (head, rest) = unfinished.split_at(30);
        append(dir.path(), TODAY, head);
        let mut tail = LogTail::from_now(dir.path());

        assert!(tail.poll().unwrap().is_empty());
        append(dir.path(), TODAY, rest);
        assert_eq!(reasons(&tail.poll().unwrap()), ["straddling"]);
    }

    #[test]
    fn a_log_file_rotated_in_after_the_tail_started_is_read_from_its_start() {
        let dir = tempfile::tempdir().unwrap();
        append(dir.path(), TODAY, &line("old"));
        let mut tail = LogTail::from_now(dir.path());
        append(dir.path(), TOMORROW, &line("rotated"));
        append(dir.path(), "unrelated.txt", &line("ignored"));

        assert_eq!(reasons(&tail.poll().unwrap()), ["rotated"]);
    }

    #[test]
    fn missing_log_directory_yields_no_lines_rather_than_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut tail = LogTail::from_now(&dir.path().join("logs"));
        assert!(tail.poll().unwrap().is_empty());
    }

    #[test]
    fn newest_lines_are_the_last_complete_lines_of_the_newest_file() {
        let dir = tempfile::tempdir().unwrap();
        append(dir.path(), TODAY, "old day\n");
        append(dir.path(), TOMORROW, "one\ntwo\nthree\npartial");
        assert_eq!(newest_lines(dir.path(), 2), ["two", "three"]);
    }
}
