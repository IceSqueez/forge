use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::system_info::IdentityScrubber;

const CRASH_FILE_PREFIX: &str = "crash-";
const CRASH_FILE_EXTENSION: &str = "json";
const HANDLED_DIR_NAME: &str = "handled";
const KEPT_HANDLED_CRASHES: usize = 20;
const SUMMARY_MAX_CHARS: usize = 80;
const DETAILS_MESSAGE_MAX_CHARS: usize = 1000;
const UNKNOWN: &str = "unknown";
const PANIC_MACHINERY_FRAMES: [&str; 8] = [
    "forge::crash_hook::record_first_crash",
    "forge::crash_hook::install",
    "<alloc::boxed::Box<dyn",
    "std::backtrace",
    "std::panicking::",
    "std::sys::backtrace::",
    "__rustc::",
    "core::panicking::",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrashRecord {
    pub message: String,
    pub location: Option<String>,
    pub thread: Option<String>,
    pub backtrace: String,
    pub forge_version: String,
    pub build_commit: String,
    pub occurred_at_utc: String,
    pub process_id: u32,
    pub system_info: Option<String>,
}

impl CrashRecord {
    pub fn scrubbed(self, scrubber: &IdentityScrubber) -> Self {
        Self {
            message: scrubber.scrub(&self.message),
            location: self.location.map(|location| scrubber.scrub(&location)),
            thread: self.thread.map(|thread| scrubber.scrub(&thread)),
            backtrace: scrubber.scrub(&self.backtrace),
            system_info: self.system_info.map(|info| scrubber.scrub(&info)),
            ..self
        }
    }

    pub fn summary(&self) -> String {
        capped(
            self.message.lines().next().unwrap_or_default().trim(),
            SUMMARY_MAX_CHARS,
        )
    }

    pub fn version_line(&self) -> String {
        format!("v{} ({})", self.forge_version, self.build_commit)
    }

    pub fn details_text(&self) -> String {
        self.details(&capped(&self.message, DETAILS_MESSAGE_MAX_CHARS))
    }

    fn details(&self, message: &str) -> String {
        [
            format!("Message: {message}"),
            format!("Location: {}", self.location.as_deref().unwrap_or(UNKNOWN)),
            format!("Thread: {}", self.thread.as_deref().unwrap_or(UNKNOWN)),
            format!("Time: {}", self.occurred_at_utc),
        ]
        .join("\n")
    }

    pub fn to_text(&self) -> String {
        let system_info = self.system_info.as_deref().unwrap_or(UNKNOWN);
        format!(
            "forge {}\n\n{}\n\nSystem info at crash time:\n{system_info}\n\nBacktrace:\n{}\n",
            self.version_line(),
            self.details(&self.message),
            self.backtrace
        )
    }
}

fn frame_symbol(line: &str) -> Option<&str> {
    let (index, symbol) = line.trim_start().split_once(": ")?;
    index
        .bytes()
        .all(|byte| byte.is_ascii_digit())
        .then_some(symbol)
        .filter(|_| !index.is_empty())
}

pub fn without_panic_machinery(backtrace: &str) -> String {
    let lines: Vec<&str> = backtrace.lines().collect();
    let first_kept_frame = lines.iter().position(|line| {
        frame_symbol(line).is_some_and(|symbol| {
            !PANIC_MACHINERY_FRAMES
                .iter()
                .any(|machinery| symbol.starts_with(machinery))
        })
    });
    match first_kept_frame {
        Some(start) => lines[start..].join("\n"),
        None => backtrace.to_owned(),
    }
}

pub fn capped(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max_chars).collect();
    cut.push_str("...");
    cut
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCrash {
    pub path: PathBuf,
    pub record: CrashRecord,
}

pub fn crash_file_name(unix_millis: u128, process_id: u32) -> String {
    format!("{CRASH_FILE_PREFIX}{unix_millis:020}-{process_id}.{CRASH_FILE_EXTENSION}")
}

pub fn is_crash_file_name(name: &str) -> bool {
    name.starts_with(CRASH_FILE_PREFIX)
        && Path::new(name)
            .extension()
            .is_some_and(|extension| extension == CRASH_FILE_EXTENSION)
}

pub fn write_crash(dir: &Path, file_name: &str, record: &CrashRecord) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(file_name);
    let bytes = serde_json::to_vec_pretty(record).map_err(io::Error::other)?;
    fs::write(&path, bytes)?;
    Ok(path)
}

fn crash_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter(|entry| is_crash_file_name(&entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
        .collect();
    files.sort();
    files
}

fn read_record(path: &Path) -> Option<CrashRecord> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn earlier_process_files(
    dir: &Path,
    current_process_id: u32,
) -> Vec<(PathBuf, Option<CrashRecord>)> {
    crash_files(dir)
        .into_iter()
        .map(|path| {
            let record = read_record(&path);
            (path, record)
        })
        .filter(|(_, record)| {
            record
                .as_ref()
                .is_none_or(|record| record.process_id != current_process_id)
        })
        .collect()
}

pub fn latest_pending(dir: &Path, current_process_id: u32) -> Option<PendingCrash> {
    earlier_process_files(dir, current_process_id)
        .into_iter()
        .rev()
        .find_map(|(path, record)| record.map(|record| PendingCrash { path, record }))
}

pub fn mark_handled(dir: &Path, current_process_id: u32) -> io::Result<()> {
    let handled_dir = dir.join(HANDLED_DIR_NAME);
    fs::create_dir_all(&handled_dir)?;
    for (path, _) in earlier_process_files(dir, current_process_id) {
        if let Some(name) = path.file_name() {
            fs::rename(&path, handled_dir.join(name))?;
        }
    }
    let handled = crash_files(&handled_dir);
    let excess = handled.len().saturating_sub(KEPT_HANDLED_CRASHES);
    for stale in handled.into_iter().take(excess) {
        fs::remove_file(stale)?;
    }
    Ok(())
}
