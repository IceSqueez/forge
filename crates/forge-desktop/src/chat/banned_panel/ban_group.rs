use forge_platform_core::{
    BanEntry, BanListOutcome, BanListSource, BanListUnavailable, BanPageToken, UnbanAbility,
};
use forge_types::PlatformId;
use gpui::Task;
use std::time::Instant;

use crate::chat::viewer_actions::{ViewerAction, ViewerTarget};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UnbanState {
    Idle,
    Running,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BanRow {
    pub entry: BanEntry,
    pub unban: UnbanState,
}

impl BanRow {
    pub(super) fn listed(entry: BanEntry) -> Self {
        Self {
            entry,
            unban: UnbanState::Idle,
        }
    }

    pub(super) fn target(&self, platform: PlatformId) -> ViewerTarget {
        ViewerTarget {
            platform,
            name: self.entry.name.clone(),
            viewer_id: Some(self.entry.viewer_id.clone()),
        }
    }

    pub(crate) fn can_unban(&self, platform: PlatformId) -> bool {
        self.entry.unban == UnbanAbility::Allowed
            && self.unban != UnbanState::Running
            && self.target(platform).supports(&ViewerAction::Unban)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GroupStatus {
    Loading,
    Unavailable(BanListUnavailable),
    Loaded,
}

#[derive(Debug)]
pub(crate) enum ListResult {
    Page {
        rows: Vec<BanRow>,
        next: Option<BanPageToken>,
    },
    Unavailable(BanListUnavailable),
}

impl ListResult {
    pub(super) fn of(outcome: BanListOutcome) -> Self {
        match outcome {
            BanListOutcome::Page(page) => Self::Page {
                rows: page.entries.into_iter().map(BanRow::listed).collect(),
                next: page.next,
            },
            BanListOutcome::Unavailable(reason) => Self::Unavailable(reason),
        }
    }
}

pub(crate) fn lists_only_what_forge_saw(platform: PlatformId) -> bool {
    match platform {
        PlatformId::Twitch => false,
        PlatformId::YouTube | PlatformId::Kick => true,
    }
}

pub(crate) async fn list_pages(
    source: &dyn BanListSource,
    after: Option<BanPageToken>,
    depth: usize,
) -> ListResult {
    let mut rows: Vec<BanRow> = Vec::new();
    let mut token = after;
    for fetched in 0..depth.max(1) {
        match ListResult::of(source.list_bans(token.as_ref()).await) {
            ListResult::Page { rows: page, next } => {
                for row in page {
                    if !rows
                        .iter()
                        .any(|known| known.entry.viewer_id == row.entry.viewer_id)
                    {
                        rows.push(row);
                    }
                }
                token = next;
                if token.is_none() {
                    break;
                }
            }
            ListResult::Unavailable(reason) if fetched == 0 => {
                return ListResult::Unavailable(reason);
            }
            ListResult::Unavailable(_) => break,
        }
    }
    ListResult::Page { rows, next: token }
}

pub(crate) struct BanGroup {
    pub platform: PlatformId,
    pub status: GroupStatus,
    pub rows: Vec<BanRow>,
    pub next: Option<BanPageToken>,
    pub loading_more: bool,
    pub more_failed: Option<BanListUnavailable>,
    pub stale: bool,
    pub pages: usize,
    pub recently_unbanned: Vec<(String, Instant)>,
    pub(super) request_id: u64,
    pub(super) _request: Option<Task<()>>,
    pub(super) _relist: Option<Task<()>>,
}

impl BanGroup {
    pub(super) fn new(platform: PlatformId) -> Self {
        Self {
            platform,
            status: GroupStatus::Loading,
            rows: Vec::new(),
            next: None,
            loading_more: false,
            more_failed: None,
            stale: false,
            pages: 1,
            recently_unbanned: Vec::new(),
            request_id: 0,
            _request: None,
            _relist: None,
        }
    }

    pub(super) fn row_mut(&mut self, viewer_id: &str) -> Option<&mut BanRow> {
        self.rows
            .iter_mut()
            .find(|row| row.entry.viewer_id == viewer_id)
    }

    fn replace_rows(&mut self, rows: Vec<BanRow>) {
        let previous = std::mem::take(&mut self.rows);
        self.rows = rows
            .into_iter()
            .map(|mut row| {
                if let Some(old) = previous
                    .iter()
                    .find(|old| old.entry.viewer_id == row.entry.viewer_id)
                {
                    row.unban = old.unban.clone();
                }
                row
            })
            .collect();
    }

    fn append_rows(&mut self, rows: Vec<BanRow>) {
        for row in rows {
            if !self
                .rows
                .iter()
                .any(|known| known.entry.viewer_id == row.entry.viewer_id)
            {
                self.rows.push(row);
            }
        }
    }

    pub(crate) fn without_recent_unbans(&mut self, result: ListResult, now: Instant) -> ListResult {
        self.recently_unbanned.retain(|(_, until)| *until > now);
        match result {
            ListResult::Page { mut rows, next } => {
                rows.retain(|row| {
                    !self
                        .recently_unbanned
                        .iter()
                        .any(|(viewer_id, _)| *viewer_id == row.entry.viewer_id)
                });
                ListResult::Page { rows, next }
            }
            unavailable @ ListResult::Unavailable(_) => unavailable,
        }
    }

    pub(super) fn apply(&mut self, appending: bool, result: ListResult) {
        self.loading_more = false;
        match (result, appending) {
            (ListResult::Page { rows, next }, true) => {
                self.append_rows(rows);
                self.next = next;
                self.more_failed = None;
                self.pages += 1;
            }
            (ListResult::Page { rows, next }, false) => {
                self.replace_rows(rows);
                self.next = next;
                self.more_failed = None;
                self.status = GroupStatus::Loaded;
            }
            (ListResult::Unavailable(reason), true) => {
                self.more_failed = Some(reason);
            }
            (ListResult::Unavailable(reason), false) => {
                self.rows.clear();
                self.next = None;
                self.more_failed = None;
                self.pages = 1;
                self.status = GroupStatus::Unavailable(reason);
            }
        }
    }
}
