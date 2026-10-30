use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::system_info::IdentityScrubber;

const CRASH_FILE_PREFIX: &str = "crash-";
const CRASH_FILE_EXTENSION: &str = "json";
const PENDING_CRASH_EXTENSION: &str = "json.tmp";
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
    let pending = path.with_extension(PENDING_CRASH_EXTENSION);
    fs::write(&pending, bytes)?;
    fs::rename(&pending, &path)?;
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
    let mut first_failure = Ok(());
    for (path, _) in earlier_process_files(dir, current_process_id) {
        if let Some(name) = path.file_name() {
            let moved = fs::rename(&path, handled_dir.join(name));
            first_failure = first_failure.and(moved);
        }
    }
    let handled = crash_files(&handled_dir);
    let excess = handled.len().saturating_sub(KEPT_HANDLED_CRASHES);
    for stale in handled.into_iter().take(excess) {
        first_failure = first_failure.and(fs::remove_file(stale));
    }
    first_failure
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{
        CrashRecord, capped, crash_file_name, is_crash_file_name, latest_pending, mark_handled,
        without_panic_machinery, write_crash,
    };
    use crate::system_info::IdentityScrubber;

    const OTHER_PROCESS: u32 = 100;
    const CURRENT_PROCESS: u32 = 999;
    const KEPT_HANDLED: usize = 20;

    fn record(message: &str, process_id: u32) -> CrashRecord {
        CrashRecord {
            message: message.to_owned(),
            location: Some("crates/forge-desktop/src/root.rs:42:9".to_owned()),
            thread: Some("main".to_owned()),
            backtrace: "  10: forge::root::Root::render\n  11: main".to_owned(),
            forge_version: "0.6.2".to_owned(),
            build_commit: "abc1234".to_owned(),
            occurred_at_utc: "2026-10-16T10:00:00Z".to_owned(),
            process_id,
            system_info: Some("OS: Arch Linux".to_owned()),
        }
    }

    fn write(dir: &Path, unix_millis: u128, process_id: u32, message: &str) {
        write_crash(
            dir,
            &crash_file_name(unix_millis, process_id),
            &record(message, process_id),
        )
        .unwrap();
    }

    fn file_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_type().unwrap().is_file())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn scrubbed_redacts_identities_in_every_text_field() {
        let scrubber = IdentityScrubber::new(["/home/alice".to_owned(), "alicebox".to_owned()]);
        let crash = CrashRecord {
            message: "cannot open /home/alice/.local/share/forge/forge.db".to_owned(),
            location: Some("/home/alice/src/forge/root.rs:1:1".to_owned()),
            thread: Some("alicebox-worker".to_owned()),
            backtrace: "   0: forge::main\n             at /home/alice/src/forge/main.rs:9:5"
                .to_owned(),
            system_info: Some("Host: ALICEBOX".to_owned()),
            ..record("", OTHER_PROCESS)
        }
        .scrubbed(&scrubber);
        let serialized = serde_json::to_string(&crash).unwrap().to_lowercase();
        assert!(!serialized.contains("alice"), "{serialized}");
    }

    #[test]
    fn summary_is_the_trimmed_first_message_line() {
        let crash = record("  index out of bounds  \nsecond line", OTHER_PROCESS);
        assert_eq!(crash.summary(), "index out of bounds");
    }

    #[test]
    fn summary_caps_a_long_first_line() {
        let crash = record(&"x".repeat(200), OTHER_PROCESS);
        assert_eq!(crash.summary(), format!("{}...", "x".repeat(80)));
    }

    #[test]
    fn summary_of_an_empty_message_is_empty() {
        assert_eq!(record("", OTHER_PROCESS).summary(), "");
    }

    #[test]
    fn capped_keeps_text_at_the_limit_and_cuts_one_over() {
        assert_eq!(capped("abcd", 4), "abcd");
        assert_eq!(capped("abcde", 4), "abcd...");
    }

    #[test]
    fn capped_counts_characters_not_bytes() {
        assert_eq!(capped("ґґґґ", 4), "ґґґґ");
        assert_eq!(capped("ґґґґ🎙", 4), "ґґґґ...");
    }

    #[test]
    fn details_text_caps_the_message_while_to_text_keeps_it_whole() {
        let message = "m".repeat(1500);
        let crash = record(&message, OTHER_PROCESS);
        assert!(
            crash
                .details_text()
                .contains(&format!("Message: {}...\n", "m".repeat(1000)))
        );
        assert!(crash.to_text().contains(&format!("Message: {message}\n")));
    }

    #[test]
    fn details_text_reports_unknown_location_and_thread() {
        let crash = CrashRecord {
            location: None,
            thread: None,
            ..record("boom", OTHER_PROCESS)
        };
        let details = crash.details_text();
        assert!(details.contains("Location: unknown"), "{details}");
        assert!(details.contains("Thread: unknown"), "{details}");
    }

    #[test]
    fn to_text_carries_version_crash_time_system_info_and_backtrace() {
        let text = record("boom", OTHER_PROCESS).to_text();
        for expected in [
            "forge v0.6.2 (abc1234)",
            "Time: 2026-10-16T10:00:00Z",
            "OS: Arch Linux",
            "  10: forge::root::Root::render\n  11: main",
        ] {
            assert!(text.contains(expected), "missing {expected:?} in {text}");
        }
    }

    #[test]
    fn to_text_reports_unknown_when_no_system_info_was_captured() {
        let crash = CrashRecord {
            system_info: None,
            ..record("boom", OTHER_PROCESS)
        };
        assert!(
            crash
                .to_text()
                .contains("System info at crash time:\nunknown\n")
        );
    }

    const CAPTURED_BACKTRACE: &str = "   0: std::backtrace_rs::backtrace::libunwind::trace
             at /rustc/abc/library/std/src/../../backtrace/src/backtrace/libunwind.rs:116:5
   1: std::backtrace::Backtrace::force_capture
   2: forge::crash_hook::record_first_crash
   3: forge::crash_hook::install::{{closure}}
   4: <alloc::boxed::Box<dyn core::ops::function::Fn<(&std::panic::PanicHookInfo,)>+Output = ()> as core::ops::function::Fn<Args>>::call
   5: std::panicking::rust_panic_with_hook
   6: std::panicking::begin_panic_handler::{{closure}}
   7: std::sys::backtrace::__rust_end_short_backtrace
   8: __rustc::rust_begin_unwind
   9: core::panicking::panic_fmt
  10: forge::root::Root::render
             at ./crates/forge-desktop/src/root.rs:42:9
  11: std::panicking::try
  12: main";

    #[test]
    fn without_panic_machinery_starts_at_the_first_app_frame() {
        let trimmed = without_panic_machinery(CAPTURED_BACKTRACE);
        assert!(
            trimmed.starts_with("  10: forge::root::Root::render\n"),
            "{trimmed}"
        );
    }

    #[test]
    fn without_panic_machinery_keeps_std_frames_after_the_first_app_frame() {
        let trimmed = without_panic_machinery(CAPTURED_BACKTRACE);
        assert!(
            trimmed.ends_with("  11: std::panicking::try\n  12: main"),
            "{trimmed}"
        );
    }

    #[test]
    fn without_panic_machinery_returns_input_without_an_app_frame() {
        let machinery_only = "   0: std::panicking::begin_panic\n   1: core::panicking::panic_fmt";
        for backtrace in ["", "disabled backtrace", machinery_only] {
            assert_eq!(without_panic_machinery(backtrace), backtrace);
        }
    }

    #[test]
    fn crash_file_names_are_recognised() {
        assert!(is_crash_file_name(&crash_file_name(1_760_000_000_000, 42)));
    }

    #[test]
    fn crash_file_names_sort_in_time_order_across_digit_counts() {
        assert!(crash_file_name(9, 7) < crash_file_name(10, 7));
    }

    #[test]
    fn is_crash_file_name_rejects_other_files() {
        for name in [
            "crash-00000000000000000001-2.txt",
            "crash-00000000000000000001-2.json.tmp",
            "crash-00000000000000000001-2",
            "Crash-00000000000000000001-2.json",
            "report-00000000000000000001-2.json",
            "forge.json",
        ] {
            assert!(!is_crash_file_name(name), "accepted {name}");
        }
    }

    #[test]
    fn latest_pending_is_none_for_a_missing_directory() {
        let temp = tempfile::tempdir().unwrap();
        assert!(latest_pending(&temp.path().join("crashes"), CURRENT_PROCESS).is_none());
    }

    #[test]
    fn latest_pending_returns_the_newest_earlier_crash() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("crashes");
        write(&dir, 10, OTHER_PROCESS, "older");
        write(&dir, 9, OTHER_PROCESS + 1, "oldest");
        write(&dir, 11, OTHER_PROCESS + 2, "newest");
        let pending = latest_pending(&dir, CURRENT_PROCESS).unwrap();
        assert_eq!(pending.record, record("newest", OTHER_PROCESS + 2));
    }

    #[test]
    fn latest_pending_skips_crashes_of_the_running_process() {
        let temp = tempfile::tempdir().unwrap();
        write(temp.path(), 1, OTHER_PROCESS, "previous run");
        write(temp.path(), 2, CURRENT_PROCESS, "this run");
        let pending = latest_pending(temp.path(), CURRENT_PROCESS).unwrap();
        assert_eq!(pending.record.message, "previous run");
    }

    #[test]
    fn latest_pending_is_none_when_only_the_running_process_crashed() {
        let temp = tempfile::tempdir().unwrap();
        write(temp.path(), 1, CURRENT_PROCESS, "this run");
        assert!(latest_pending(temp.path(), CURRENT_PROCESS).is_none());
    }

    #[test]
    fn latest_pending_ignores_files_that_are_not_crash_files() {
        let temp = tempfile::tempdir().unwrap();
        write_crash(temp.path(), "notes.json", &record("stray", OTHER_PROCESS)).unwrap();
        assert!(latest_pending(temp.path(), CURRENT_PROCESS).is_none());
    }

    #[test]
    fn latest_pending_falls_back_past_an_unreadable_newer_crash() {
        let temp = tempfile::tempdir().unwrap();
        write(temp.path(), 1, OTHER_PROCESS, "readable");
        fs::write(
            temp.path().join(crash_file_name(2, OTHER_PROCESS)),
            "{ torn",
        )
        .unwrap();
        let pending = latest_pending(temp.path(), CURRENT_PROCESS).unwrap();
        assert_eq!(pending.record.message, "readable");
    }

    #[test]
    fn mark_handled_moves_earlier_crashes_and_leaves_the_running_process_crash() {
        let temp = tempfile::tempdir().unwrap();
        write(temp.path(), 1, OTHER_PROCESS, "previous run");
        fs::write(
            temp.path().join(crash_file_name(2, OTHER_PROCESS)),
            "{ torn",
        )
        .unwrap();
        write(temp.path(), 3, CURRENT_PROCESS, "this run");
        mark_handled(temp.path(), CURRENT_PROCESS).unwrap();
        assert_eq!(
            file_names(temp.path()),
            vec![crash_file_name(3, CURRENT_PROCESS)]
        );
        assert_eq!(
            file_names(&temp.path().join("handled")),
            vec![
                crash_file_name(1, OTHER_PROCESS),
                crash_file_name(2, OTHER_PROCESS)
            ]
        );
    }

    #[test]
    fn a_failed_write_leaves_the_crash_already_on_disk_whole() {
        let temp = tempfile::tempdir().unwrap();
        write(temp.path(), 1, OTHER_PROCESS, "the first record");
        let name = crash_file_name(1, OTHER_PROCESS);
        fs::create_dir(temp.path().join(format!("{name}.tmp"))).unwrap();

        let replaced = write_crash(temp.path(), &name, &record("replacement", OTHER_PROCESS));

        let kept = latest_pending(temp.path(), CURRENT_PROCESS).unwrap();
        assert_eq!(
            (replaced.is_err(), kept.record.message.as_str()),
            (true, "the first record")
        );
    }

    #[test]
    fn a_torn_write_is_never_offered_as_a_pending_crash() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path()
                .join(format!("{}.tmp", crash_file_name(1, OTHER_PROCESS))),
            "{ torn",
        )
        .unwrap();

        assert!(latest_pending(temp.path(), CURRENT_PROCESS).is_none());
    }

    #[test]
    fn a_crash_that_cannot_move_does_not_strand_the_ones_after_it() {
        let temp = tempfile::tempdir().unwrap();
        write(temp.path(), 1, OTHER_PROCESS, "blocked");
        write(temp.path(), 2, OTHER_PROCESS, "movable");
        let blocker = temp
            .path()
            .join("handled")
            .join(crash_file_name(1, OTHER_PROCESS));
        fs::create_dir_all(blocker.join("occupied")).unwrap();

        let outcome = mark_handled(temp.path(), CURRENT_PROCESS);

        assert_eq!(
            (
                outcome.is_err(),
                file_names(temp.path()),
                file_names(&temp.path().join("handled")),
            ),
            (
                true,
                vec![crash_file_name(1, OTHER_PROCESS)],
                vec![crash_file_name(2, OTHER_PROCESS)],
            )
        );
    }

    #[test]
    fn mark_handled_clears_the_pending_notice() {
        let temp = tempfile::tempdir().unwrap();
        write(temp.path(), 1, OTHER_PROCESS, "previous run");
        mark_handled(temp.path(), CURRENT_PROCESS).unwrap();
        assert!(latest_pending(temp.path(), CURRENT_PROCESS).is_none());
    }

    #[test]
    fn mark_handled_keeps_every_handled_crash_at_the_limit() {
        let temp = tempfile::tempdir().unwrap();
        let handled = temp.path().join("handled");
        for millis in 1..KEPT_HANDLED as u128 {
            write(&handled, millis, OTHER_PROCESS, "handled");
        }
        write(temp.path(), KEPT_HANDLED as u128, OTHER_PROCESS, "pending");
        mark_handled(temp.path(), CURRENT_PROCESS).unwrap();
        assert_eq!(file_names(&handled).len(), KEPT_HANDLED);
    }

    #[test]
    fn mark_handled_drops_the_oldest_handled_crash_one_over_the_limit() {
        let temp = tempfile::tempdir().unwrap();
        let handled = temp.path().join("handled");
        for millis in 1..=KEPT_HANDLED as u128 {
            write(&handled, millis, OTHER_PROCESS, "handled");
        }
        write(temp.path(), 100, OTHER_PROCESS, "pending");
        mark_handled(temp.path(), CURRENT_PROCESS).unwrap();
        let names = file_names(&handled);
        assert_eq!(names.len(), KEPT_HANDLED);
        assert!(!names.contains(&crash_file_name(1, OTHER_PROCESS)));
        assert!(names.contains(&crash_file_name(100, OTHER_PROCESS)));
    }
}
