use std::ffi::OsStr;

use time::OffsetDateTime;

use crate::naming::{
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
