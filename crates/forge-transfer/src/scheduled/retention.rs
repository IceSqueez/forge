use std::collections::HashSet;
use std::path::PathBuf;

use time::{Duration, OffsetDateTime, UtcOffset};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RetentionPolicy {
    pub keep_newest: u32,
    pub keep_daily_days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackupFile {
    pub path: PathBuf,
    pub created_at: OffsetDateTime,
}

pub(crate) fn expired(
    files: &[BackupFile],
    policy: RetentionPolicy,
    now: OffsetDateTime,
    offset: UtcOffset,
) -> Vec<PathBuf> {
    let mut newest_first: Vec<&BackupFile> = files.iter().collect();
    newest_first.sort_by_key(|file| std::cmp::Reverse(file.created_at));
    let daily_cutoff = now - Duration::days(i64::from(policy.keep_daily_days));
    let mut days_kept = HashSet::new();
    newest_first
        .into_iter()
        .enumerate()
        .filter(|(rank, file)| {
            let within_newest = *rank < policy.keep_newest as usize;
            let first_of_day = policy.keep_daily_days > 0
                && file.created_at >= daily_cutoff
                && days_kept.insert(file.created_at.to_offset(offset).date());
            !(within_newest || first_of_day)
        })
        .map(|(_, file)| file.path.clone())
        .collect()
}
