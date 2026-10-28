use std::ffi::OsStr;

use time::OffsetDateTime;

use crate::bundle::naming::{
    FILE_PREFIX, LEGACY_ARCHIVE_EXTENSION, PRE_PACK_PREFIX, PRE_RESTORE_PREFIX, PRE_SETUP_PREFIX,
    PRE_UPDATE_SNAPSHOT_EXTENSION, stamp_of,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SafetyCopyKind {
    PreRestore,
    PrePack,
    PreSetup,
    PreUpdate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Classified {
    pub kind: SafetyCopyKind,
    pub created_at: OffsetDateTime,
}

pub(crate) fn classify(name: &OsStr, is_dir: bool) -> Option<Classified> {
    let name = name.to_str()?;
    let archive_suffix = format!(".{LEGACY_ARCHIVE_EXTENSION}");
    let update_suffix = format!(".{PRE_UPDATE_SNAPSHOT_EXTENSION}");
    let (kind, rest) = if is_dir {
        (
            SafetyCopyKind::PreRestore,
            name.strip_prefix(PRE_RESTORE_PREFIX)?,
        )
    } else if let Some(rest) = name.strip_prefix(PRE_PACK_PREFIX) {
        (SafetyCopyKind::PrePack, rest.strip_suffix(&archive_suffix)?)
    } else if let Some(rest) = name.strip_prefix(PRE_SETUP_PREFIX) {
        (
            SafetyCopyKind::PreSetup,
            rest.strip_suffix(&archive_suffix)?,
        )
    } else {
        (
            SafetyCopyKind::PreUpdate,
            name.strip_prefix(FILE_PREFIX)?
                .strip_suffix(&update_suffix)?,
        )
    };
    let created_at = stamp_of(rest.strip_prefix('-')?)?;
    Some(Classified { kind, created_at })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use time::{Date, Month, PrimitiveDateTime, Time};

    use super::{SafetyCopyKind, classify};

    fn stamped_at() -> time::OffsetDateTime {
        PrimitiveDateTime::new(
            Date::from_calendar_date(2026, Month::October, 22).unwrap(),
            Time::from_hms(9, 5, 7).unwrap(),
        )
        .assume_utc()
    }

    #[test]
    fn each_safety_copy_name_is_classified_with_the_time_stamped_in_it() {
        for (name, is_dir, kind) in [
            (
                "forge-pre-restore-20261022-090507Z-a1b2c3",
                true,
                SafetyCopyKind::PreRestore,
            ),
            (
                "forge-pre-pack-20261022-090507Z-a1b2c3.tar.gz",
                false,
                SafetyCopyKind::PrePack,
            ),
            (
                "forge-pre-setup-20261022-090507Z-a1b2c3.tar.gz",
                false,
                SafetyCopyKind::PreSetup,
            ),
            (
                "forge-backup-20261022-090507Z-a1b2c3.pre-update.db",
                false,
                SafetyCopyKind::PreUpdate,
            ),
        ] {
            let classified = classify(OsStr::new(name), is_dir).unwrap();

            assert_eq!(
                (classified.kind, classified.created_at),
                (kind, stamped_at()),
                "{name}"
            );
        }
    }

    #[test]
    fn names_that_are_not_safety_copies_are_never_classified() {
        for (name, is_dir) in [
            ("forge-backup-20261022-090507Z-a1b2c3.forgebundle", false),
            ("forge-scheduled-20261022-090507Z-a1b2c3.forgebundle", false),
            ("forge-backup-20261022-090507Z-a1b2c3.tar.gz", false),
            ("forge-backup-20261022-090507Z-a1b2c3.db.snapshot", false),
            (
                "forge-pre-pack-20261022-090507Z-a1b2c3.tar.gz.partial",
                false,
            ),
            (
                "forge-pre-setup-20261022-090507Z-a1b2c3.tar.gz.partial",
                false,
            ),
            (
                "forge-backup-20261022-090507Z-a1b2c3.pre-update.db.partial",
                false,
            ),
            ("forge-pre-restore-20261022-090507Z-a1b2c3", false),
            ("forge-pre-pack-20261022-090507Z-a1b2c3.tar.gz", true),
            ("forge-backup-20261022-090507Z-a1b2c3.pre-update.db", true),
            ("forge-restore-staging", true),
            ("notes.txt", false),
        ] {
            assert!(
                classify(OsStr::new(name), is_dir).is_none(),
                "{name} dir={is_dir}"
            );
        }
    }

    #[test]
    fn a_safety_copy_name_whose_stamp_does_not_parse_is_never_classified() {
        for (name, is_dir) in [
            ("forge-pre-restore-manual-copy", true),
            ("forge-pre-restore-20261022-090507-a1b2c3", true),
            ("forge-pre-restore20261022-090507Z-a1b2c3", true),
            ("forge-pre-restore-", true),
            ("forge-pre-pack-20261340-090507Z-a1b2c3.tar.gz", false),
            ("forge-pre-pack-20261022-250000Z-a1b2c3.tar.gz", false),
            ("forge-pre-setup-2026102-090507Z-a1b2c3.tar.gz", false),
            ("forge-pre-setup-20261022-090507junkZ-a1b2c3.tar.gz", false),
            ("forge-backup-latest.pre-update.db", false),
            ("forge-pre-restore-２０２６1022-090507Z-a1b2c3", true),
        ] {
            assert!(
                classify(OsStr::new(name), is_dir).is_none(),
                "{name} dir={is_dir}"
            );
        }
    }
}
