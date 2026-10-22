use std::collections::{HashMap, HashSet};
use std::ffi::OsString;

use time::{Duration, OffsetDateTime};

use crate::safety_copies::inventory::SafetyCopy;
use crate::safety_copies::kind::SafetyCopyKind;
use crate::safety_copies::settings::SafetyRetention;

#[derive(Debug, Clone, Default)]
pub(crate) struct Protection {
    pub names: HashSet<OsString>,
    pub kinds: HashSet<SafetyCopyKind>,
}

impl Protection {
    fn covers(&self, copy: &SafetyCopy) -> bool {
        self.kinds.contains(&copy.kind)
            || copy
                .path
                .file_name()
                .is_some_and(|name| self.names.contains(name))
    }
}

pub(crate) fn doomed(
    copies: Vec<SafetyCopy>,
    retention: SafetyRetention,
    now: OffsetDateTime,
    protection: &Protection,
) -> Vec<SafetyCopy> {
    let mut by_kind: HashMap<SafetyCopyKind, Vec<SafetyCopy>> = HashMap::new();
    for copy in copies {
        by_kind.entry(copy.kind).or_default().push(copy);
    }
    by_kind
        .into_values()
        .flat_map(|group| expired_in_kind(group, retention, now))
        .filter(|copy| !protection.covers(copy))
        .collect()
}

pub(crate) fn everything_unprotected(
    copies: Vec<SafetyCopy>,
    protection: &Protection,
) -> Vec<SafetyCopy> {
    copies
        .into_iter()
        .filter(|copy| !protection.covers(copy))
        .collect()
}

fn expired_in_kind(
    mut group: Vec<SafetyCopy>,
    retention: SafetyRetention,
    now: OffsetDateTime,
) -> Vec<SafetyCopy> {
    group.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| a.path.cmp(&b.path))
    });
    match retention {
        SafetyRetention::Never => Vec::new(),
        SafetyRetention::KeepNewest(keep) => group.into_iter().skip(keep as usize).collect(),
        SafetyRetention::DeleteAfterDays(days) => {
            let cutoff = now - Duration::days(i64::from(days));
            group
                .into_iter()
                .enumerate()
                .filter(|(rank, copy)| *rank > 0 && copy.created_at < cutoff)
                .map(|(_, copy)| copy)
                .collect()
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeSet;
    use std::ffi::OsString;
    use std::path::PathBuf;

    use time::{Date, Duration, Month, OffsetDateTime, PrimitiveDateTime, Time};

    use super::{Protection, doomed, everything_unprotected};
    use crate::safety_copies::inventory::SafetyCopy;
    use crate::safety_copies::kind::SafetyCopyKind;
    use crate::safety_copies::settings::SafetyRetention;

    const KEEP: u32 = 2;
    const DAYS: u32 = 30;

    fn now() -> OffsetDateTime {
        PrimitiveDateTime::new(
            Date::from_calendar_date(2026, Month::October, 22).unwrap(),
            Time::from_hms(12, 0, 0).unwrap(),
        )
        .assume_utc()
    }

    fn copy(name: &str, kind: SafetyCopyKind, age: Duration) -> SafetyCopy {
        SafetyCopy {
            path: PathBuf::from("backups").join(name),
            kind,
            created_at: now() - age,
            bytes: 1,
        }
    }

    fn names(copies: Vec<SafetyCopy>) -> BTreeSet<String> {
        copies
            .into_iter()
            .map(|c| c.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn cutoff_age() -> Duration {
        Duration::days(i64::from(DAYS))
    }

    #[test]
    fn keep_newest_condemns_only_the_copies_beyond_the_newest_n_of_each_kind() {
        let copies = vec![
            copy("pack-a", SafetyCopyKind::PrePack, Duration::hours(1)),
            copy("pack-b", SafetyCopyKind::PrePack, Duration::hours(2)),
            copy("pack-c", SafetyCopyKind::PrePack, Duration::hours(3)),
            copy("pack-d", SafetyCopyKind::PrePack, Duration::hours(4)),
            copy("setup-a", SafetyCopyKind::PreSetup, Duration::days(400)),
            copy("setup-b", SafetyCopyKind::PreSetup, Duration::days(500)),
        ];

        let condemned = doomed(
            copies,
            SafetyRetention::KeepNewest(KEEP),
            now(),
            &Protection::default(),
        );

        assert_eq!(names(condemned), set(&["pack-c", "pack-d"]));
    }

    #[test]
    fn keep_newest_ranks_by_the_stamp_not_by_the_listing_order() {
        let copies = vec![
            copy("oldest", SafetyCopyKind::PreUpdate, Duration::hours(9)),
            copy("newest", SafetyCopyKind::PreUpdate, Duration::hours(1)),
            copy("middle", SafetyCopyKind::PreUpdate, Duration::hours(5)),
        ];

        let condemned = doomed(
            copies,
            SafetyRetention::KeepNewest(KEEP),
            now(),
            &Protection::default(),
        );

        assert_eq!(names(condemned), set(&["oldest"]));
    }

    #[test]
    fn keep_newest_at_exactly_n_copies_condemns_nothing() {
        let copies = vec![
            copy("a", SafetyCopyKind::PrePack, Duration::hours(1)),
            copy("b", SafetyCopyKind::PrePack, Duration::hours(2)),
        ];

        let condemned = doomed(
            copies,
            SafetyRetention::KeepNewest(KEEP),
            now(),
            &Protection::default(),
        );

        assert!(condemned.is_empty());
    }

    #[test]
    fn delete_after_days_condemns_one_second_past_the_cutoff_but_keeps_the_cutoff_itself() {
        let one_second = Duration::seconds(1);
        let copies = vec![
            copy("fresh", SafetyCopyKind::PrePack, Duration::hours(1)),
            copy("at-cutoff", SafetyCopyKind::PrePack, cutoff_age()),
            copy(
                "just-inside",
                SafetyCopyKind::PrePack,
                cutoff_age() - one_second,
            ),
            copy(
                "just-past",
                SafetyCopyKind::PrePack,
                cutoff_age() + one_second,
            ),
        ];

        let condemned = doomed(
            copies,
            SafetyRetention::DeleteAfterDays(DAYS),
            now(),
            &Protection::default(),
        );

        assert_eq!(names(condemned), set(&["just-past"]));
    }

    #[test]
    fn delete_after_days_keeps_the_newest_copy_of_each_kind_even_when_it_has_expired() {
        let copies = vec![
            copy(
                "restore-new",
                SafetyCopyKind::PreRestore,
                Duration::days(90),
            ),
            copy(
                "restore-old",
                SafetyCopyKind::PreRestore,
                Duration::days(91),
            ),
            copy(
                "update-only",
                SafetyCopyKind::PreUpdate,
                Duration::days(365),
            ),
        ];

        let condemned = doomed(
            copies,
            SafetyRetention::DeleteAfterDays(DAYS),
            now(),
            &Protection::default(),
        );

        assert_eq!(names(condemned), set(&["restore-old"]));
    }

    #[test]
    fn never_condemns_nothing_however_old_or_numerous() {
        let copies = (0..10)
            .map(|day| {
                copy(
                    &format!("pack-{day}"),
                    SafetyCopyKind::PrePack,
                    Duration::days(1000 + day),
                )
            })
            .collect();

        let condemned = doomed(
            copies,
            SafetyRetention::Never,
            now(),
            &Protection::default(),
        );

        assert!(condemned.is_empty());
    }

    #[test]
    fn a_protected_name_or_kind_survives_a_policy_that_would_condemn_it() {
        let protection = Protection {
            names: [OsString::from("restore-referenced")].into_iter().collect(),
            kinds: [SafetyCopyKind::PreUpdate].into_iter().collect(),
        };
        let copies = vec![
            copy(
                "restore-new",
                SafetyCopyKind::PreRestore,
                Duration::hours(1),
            ),
            copy(
                "restore-referenced",
                SafetyCopyKind::PreRestore,
                Duration::hours(2),
            ),
            copy(
                "restore-old",
                SafetyCopyKind::PreRestore,
                Duration::hours(3),
            ),
            copy("update-new", SafetyCopyKind::PreUpdate, Duration::hours(1)),
            copy("update-old", SafetyCopyKind::PreUpdate, Duration::hours(2)),
        ];

        let condemned = doomed(copies, SafetyRetention::KeepNewest(1), now(), &protection);

        assert_eq!(names(condemned), set(&["restore-old"]));
    }

    #[test]
    fn everything_unprotected_includes_the_newest_of_each_kind_but_spares_protected_copies() {
        let protection = Protection {
            names: [OsString::from("restore-referenced")].into_iter().collect(),
            kinds: [SafetyCopyKind::PreUpdate].into_iter().collect(),
        };
        let copies = vec![
            copy("pack-only", SafetyCopyKind::PrePack, Duration::seconds(1)),
            copy("setup-only", SafetyCopyKind::PreSetup, Duration::seconds(1)),
            copy(
                "restore-new",
                SafetyCopyKind::PreRestore,
                Duration::hours(1),
            ),
            copy(
                "restore-referenced",
                SafetyCopyKind::PreRestore,
                Duration::hours(2),
            ),
            copy("update-new", SafetyCopyKind::PreUpdate, Duration::hours(1)),
        ];

        let condemned = everything_unprotected(copies, &protection);

        assert_eq!(
            names(condemned),
            set(&["pack-only", "setup-only", "restore-new"])
        );
    }
}
