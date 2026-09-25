use std::path::{Path, PathBuf};

use serde::Serialize;

/// One reading of a Linux process from `/proc`; every field is `None` when unreadable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct ProcessReading {
    /// utime + stime of the whole process, in clock ticks.
    pub cpu_ticks: Option<u64>,
    /// utime + stime of the thread whose id is the pid: the one gpui renders on.
    pub main_thread_ticks: Option<u64>,
    pub rss_kib: Option<u64>,
    pub anonymous_kib: Option<u64>,
    pub fds: Option<u64>,
    pub threads: Option<u64>,
}

pub struct ProcessProbe {
    root: PathBuf,
}

impl ProcessProbe {
    pub fn new(pid: u32) -> Self {
        Self {
            root: PathBuf::from(format!("/proc/{pid}")),
        }
    }

    pub fn read(&self) -> ProcessReading {
        let pid_dir = self.root.file_name().map(PathBuf::from).unwrap_or_default();
        ProcessReading {
            cpu_ticks: read(&self.root.join("stat"))
                .as_deref()
                .and_then(stat_cpu_ticks),
            main_thread_ticks: read(&self.root.join("task").join(&pid_dir).join("stat"))
                .as_deref()
                .and_then(stat_cpu_ticks),
            rss_kib: read(&self.root.join("smaps_rollup"))
                .as_deref()
                .and_then(|text| kib_field(text, "Rss:")),
            anonymous_kib: read(&self.root.join("smaps_rollup"))
                .as_deref()
                .and_then(|text| kib_field(text, "Anonymous:")),
            fds: std::fs::read_dir(self.root.join("fd"))
                .ok()
                .map(|entries| entries.count() as u64),
            threads: read(&self.root.join("status"))
                .as_deref()
                .and_then(|text| plain_field(text, "Threads:")),
        }
    }
}

fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Fields 14 and 15 of `/proc/<pid>/stat`, counted after the parenthesised command name, which
/// may itself hold spaces and parentheses.
pub fn stat_cpu_ticks(stat: &str) -> Option<u64> {
    let after_name = &stat[stat.rfind(')')? + 1..];
    let mut fields = after_name.split_whitespace();
    let utime: u64 = fields.nth(11)?.parse().ok()?;
    let stime: u64 = fields.next()?.parse().ok()?;
    Some(utime + stime)
}

pub fn kib_field(text: &str, name: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix(name))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse().ok())
}

fn plain_field(text: &str, name: &str) -> Option<u64> {
    kib_field(text, name)
}

/// Clock ticks per second; Linux has reported 100 on every mainstream architecture for decades
/// and `/proc` exposes no way to ask without libc.
pub const CLOCK_TICKS_PER_SEC: f64 = 100.0;

/// Percent of one core between two tick readings `elapsed_ms` apart.
pub fn cpu_percent(before: Option<u64>, after: Option<u64>, elapsed_ms: u64) -> Option<f64> {
    let (before, after) = (before?, after?);
    if elapsed_ms == 0 || after < before {
        return None;
    }
    let seconds = (after - before) as f64 / CLOCK_TICKS_PER_SEC;
    Some(seconds * 100_000.0 / elapsed_ms as f64)
}

/// The database file with its write-ahead log and shared-memory index.
pub fn database_bytes(data_dir: &Path) -> Option<u64> {
    let base = data_dir.join("forge.db");
    let main = std::fs::metadata(&base).ok()?.len();
    let side = ["forge.db-wal", "forge.db-shm"]
        .iter()
        .filter_map(|name| std::fs::metadata(data_dir.join(name)).ok())
        .map(|meta| meta.len())
        .sum::<u64>();
    Some(main + side)
}

/// The machine's one-minute load average, to judge contention from other processes.
pub fn load_average() -> Option<f64> {
    read(Path::new("/proc/loadavg"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn cpu_ticks_are_counted_after_a_command_name_holding_spaces_and_parentheses() {
        let stat = "4242 (forge (main) x) S 1 2 3 4 5 6 7 8 9 10 250 50 0 0 20 0 90";
        assert_eq!(stat_cpu_ticks(stat), Some(300));
    }

    #[test]
    fn truncated_or_garbled_stat_lines_yield_no_ticks() {
        for stat in [
            "",
            "4242 forge S 1",
            "4242 (forge) S 1 2 3",
            "4242 (forge) S 1 2 3 4 5 6 7 8 9 10 x 50",
        ] {
            assert_eq!(stat_cpu_ticks(stat), None, "{stat:?}");
        }
    }

    #[test]
    fn smaps_rollup_fields_are_read_by_exact_name() {
        let rollup = "Rss:              265812 kB\nPss_Anon:          79000 kB\nAnonymous:         81044 kB\n";
        assert_eq!(kib_field(rollup, "Rss:"), Some(265_812));
        assert_eq!(kib_field(rollup, "Anonymous:"), Some(81_044));
        assert_eq!(kib_field(rollup, "Swap:"), None);
    }

    #[test]
    fn one_second_of_ticks_over_one_second_is_one_full_core() {
        let ticks = CLOCK_TICKS_PER_SEC as u64;
        assert_eq!(cpu_percent(Some(0), Some(ticks), 1_000), Some(100.0));
        assert_eq!(cpu_percent(Some(0), Some(ticks * 3), 1_000), Some(300.0));
    }

    #[test]
    fn cpu_percent_is_unknown_without_two_ordered_readings_over_time() {
        for (before, after, elapsed) in [
            (None, Some(5), 1_000),
            (Some(5), None, 1_000),
            (Some(9), Some(5), 1_000),
            (Some(5), Some(9), 0),
        ] {
            assert_eq!(
                cpu_percent(before, after, elapsed),
                None,
                "{before:?} {after:?} {elapsed}"
            );
        }
    }
}
