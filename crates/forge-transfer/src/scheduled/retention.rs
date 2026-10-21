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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::PathBuf;

    use time::{Date, Duration, Month, OffsetDateTime, UtcOffset};

    use super::{BackupFile, RetentionPolicy, expired};

    fn noon() -> OffsetDateTime {
        Date::from_calendar_date(2026, Month::October, 21)
            .unwrap()
            .with_hms(12, 0, 0)
            .unwrap()
            .assume_utc()
    }

    fn file(name: &str, created_at: OffsetDateTime) -> BackupFile {
        BackupFile {
            path: PathBuf::from(name),
            created_at,
        }
    }

    fn policy(keep_newest: u32, keep_daily_days: u32) -> RetentionPolicy {
        RetentionPolicy {
            keep_newest,
            keep_daily_days,
        }
    }

    fn names(paths: Vec<PathBuf>) -> Vec<String> {
        let mut names: Vec<String> = paths
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn newest_tier_keeps_the_newest_files_whatever_the_input_order() {
        let now = noon();
        let files = [
            file("b", now - Duration::hours(2)),
            file("d", now - Duration::hours(4)),
            file("a", now - Duration::hours(1)),
            file("c", now - Duration::hours(3)),
        ];

        let doomed = expired(&files, policy(2, 0), now, UtcOffset::UTC);

        assert_eq!(names(doomed), ["c", "d"]);
    }

    #[test]
    fn keeping_one_newest_without_a_daily_tier_expires_everything_else() {
        let now = noon();
        let files = [
            file("newest", now - Duration::minutes(1)),
            file("older", now - Duration::minutes(2)),
            file("oldest", now - Duration::days(400)),
        ];

        let doomed = expired(&files, policy(1, 0), now, UtcOffset::UTC);

        assert_eq!(names(doomed), ["older", "oldest"]);
    }

    #[test]
    fn a_policy_that_covers_every_file_expires_nothing() {
        let now = noon();
        let files = [
            file("a", now - Duration::hours(1)),
            file("b", now - Duration::hours(2)),
        ];

        assert!(expired(&files, policy(2, 0), now, UtcOffset::UTC).is_empty());
    }

    #[test]
    fn daily_tier_keeps_the_newest_file_of_each_day_inside_the_window() {
        let now = noon();
        let files = [
            file("today-noon", now - Duration::minutes(5)),
            file("today-morning", now - Duration::hours(4)),
            file("yesterday-evening", now - Duration::hours(16)),
            file("yesterday-morning", now - Duration::hours(26)),
            file("two-days-ago", now - Duration::hours(51)),
            file("five-days-ago", now - Duration::days(5)),
        ];

        let doomed = expired(&files, policy(1, 3), now, UtcOffset::UTC);

        assert_eq!(
            names(doomed),
            ["five-days-ago", "today-morning", "yesterday-morning"]
        );
    }

    #[test]
    fn daily_window_keeps_a_file_exactly_at_the_cutoff_and_expires_one_second_older() {
        let now = noon();
        let cutoff = now - Duration::days(2);
        let files = [
            file("newest", now),
            file("at-cutoff", cutoff),
            file("past-cutoff", cutoff - Duration::seconds(1)),
        ];

        let doomed = expired(&files, policy(1, 2), now, UtcOffset::UTC);

        assert_eq!(names(doomed), ["past-cutoff"]);
    }

    #[test]
    fn daily_tier_groups_files_by_the_local_day_not_the_utc_day() {
        let now = noon();
        let midnight_utc = Date::from_calendar_date(2026, Month::October, 20)
            .unwrap()
            .midnight()
            .assume_utc();
        let files = [
            file("newest", now),
            file("after-utc-midnight", midnight_utc + Duration::minutes(30)),
            file("before-utc-midnight", midnight_utc - Duration::minutes(30)),
        ];
        let west = UtcOffset::from_hms(-2, 0, 0).unwrap();

        let in_utc = expired(&files, policy(1, 3), now, UtcOffset::UTC);
        let in_west = expired(&files, policy(1, 3), now, west);

        assert_eq!(
            (names(in_utc), names(in_west)),
            (vec![], vec!["before-utc-midnight".to_owned()])
        );
    }

    #[test]
    fn zero_daily_days_disables_the_daily_tier() {
        let now = noon();
        let files = [
            file("newest", now),
            file("yesterday", now - Duration::days(1)),
        ];

        let doomed = expired(&files, policy(1, 0), now, UtcOffset::UTC);

        assert_eq!(names(doomed), ["yesterday"]);
    }
}
