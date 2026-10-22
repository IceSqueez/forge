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
