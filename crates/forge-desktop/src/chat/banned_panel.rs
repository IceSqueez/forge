use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_components::{
    BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, Platform, PlatformKind, Radius,
    Spacing, body_family, empty_state, ghost_button, mono_family, platform_color, radius,
    secondary_button, spacing, status_dot, tr,
};
use forge_platform_core::{
    BanDuration, BanEntry, BanListOutcome, BanListSource, BanListUnavailable, BanPageToken,
    UnbanAbility,
};
use forge_runtime::{ActionEngineHandle, EventBus};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, Pixels, SharedString, Subscription,
    Task, Window, div, prelude::*, px,
};
use time::UtcOffset;

use super::ban_labels::{
    banned_at_label, changed_platforms, expiry_label, optional_label, refusal_label,
    unavailable_label,
};
use super::platform_display_name;
use super::platform_gate::platform_integration;
use super::viewer_actions::{ViewerAction, ViewerTarget};
use crate::async_bridge::{self, EventBatch};
use crate::integration_disabled::disclaimer_line;
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::integration_supervisor::LifecycleState;
use crate::integrations::BuiltinRegistry;
use crate::presentation::ActivePresentation;
use crate::scheduled_run_labels::system_offset_at;

const BAN_RELIST_DEBOUNCE: Duration = Duration::from_millis(750);
pub(crate) const UNBAN_RELIST_SUPPRESSION: Duration = Duration::from_secs(5);
const PLATFORM_ORDER: [Platform; 3] = [Platform::Twitch, Platform::YouTube, Platform::Kick];
const GROUP_DOT: Pixels = px(6.0);
const NAME_COLUMN_W: Pixels = px(160.0);
const MODERATOR_COLUMN_W: Pixels = px(130.0);
const BANNED_AT_COLUMN_W: Pixels = px(90.0);
const EXPIRY_COLUMN_W: Pixels = px(170.0);
const UNBAN_COLUMN_W: Pixels = px(84.0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UnbanState {
    Idle,
    Running,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BanRow {
    pub entry: BanEntry,
    pub expiry_offset: UtcOffset,
    pub unban: UnbanState,
}

impl BanRow {
    fn listed(entry: BanEntry) -> Self {
        let expiry_offset = match entry.duration {
            BanDuration::Until(at) => system_offset_at(at),
            BanDuration::Permanent => UtcOffset::UTC,
        };
        Self {
            entry,
            expiry_offset,
            unban: UnbanState::Idle,
        }
    }

    fn target(&self, platform: Platform) -> ViewerTarget {
        ViewerTarget {
            platform,
            name: self.entry.name.clone(),
            viewer_id: Some(self.entry.viewer_id.clone()),
        }
    }

    pub fn can_unban(&self, platform: Platform) -> bool {
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
    fn of(outcome: BanListOutcome) -> Self {
        match outcome {
            BanListOutcome::Page(page) => Self::Page {
                rows: page.entries.into_iter().map(BanRow::listed).collect(),
                next: page.next,
            },
            BanListOutcome::Unavailable(reason) => Self::Unavailable(reason),
        }
    }
}

pub(crate) fn lists_only_what_forge_saw(platform: Platform) -> bool {
    match platform {
        Platform::Twitch => false,
        Platform::YouTube | Platform::Kick => true,
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
    pub platform: Platform,
    pub status: GroupStatus,
    pub rows: Vec<BanRow>,
    pub next: Option<BanPageToken>,
    pub loading_more: bool,
    pub more_failed: Option<BanListUnavailable>,
    pub stale: bool,
    pub pages: usize,
    pub recently_unbanned: Vec<(String, Instant)>,
    request_id: u64,
    _request: Option<Task<()>>,
    _relist: Option<Task<()>>,
}

impl BanGroup {
    fn new(platform: Platform) -> Self {
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

    fn row_mut(&mut self, viewer_id: &str) -> Option<&mut BanRow> {
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

    fn apply(&mut self, appending: bool, result: ListResult) {
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

pub struct BannedPanel {
    builtins: BuiltinRegistry,
    lifecycle: Option<Entity<IntegrationLifecycle>>,
    rt_handle: tokio::runtime::Handle,
    action_engine: ActionEngineHandle,
    groups: Vec<BanGroup>,
    visible: bool,
    last_request: u64,
    _bus_watch: Option<Task<()>>,
    _lifecycle_obs: Option<Subscription>,
}

impl BannedPanel {
    pub fn new(
        builtins: BuiltinRegistry,
        lifecycle: Option<Entity<IntegrationLifecycle>>,
        bus: Option<Arc<EventBus>>,
        rt_handle: tokio::runtime::Handle,
        action_engine: ActionEngineHandle,
        cx: &mut Context<Self>,
    ) -> Self {
        let lifecycle_obs = lifecycle
            .as_ref()
            .map(|lifecycle| cx.observe(lifecycle, |this, _, cx| this.sync_platforms(cx)));
        let bus_watch = bus.map(|bus| Self::watch_ban_changes(&bus, cx));
        let mut this = Self {
            builtins,
            lifecycle,
            rt_handle,
            action_engine,
            groups: Vec::new(),
            visible: true,
            last_request: 0,
            _bus_watch: bus_watch,
            _lifecycle_obs: lifecycle_obs,
        };
        this.sync_platforms(cx);
        this
    }

    fn watch_ban_changes(bus: &Arc<EventBus>, cx: &mut Context<Self>) -> Task<()> {
        let mut subscription = bus.subscribe();
        cx.spawn(async move |this, cx| {
            while let EventBatch::Ready(batch) =
                async_bridge::recv_event_batch(&mut subscription).await
            {
                let platforms = changed_platforms(&batch);
                if platforms.is_empty() {
                    continue;
                }
                let alive = this.update(cx, |this, cx| {
                    for platform in platforms {
                        this.schedule_relist(platform, cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
    }

    fn platform_running(&self, platform: Platform, cx: &App) -> bool {
        match &self.lifecycle {
            Some(lifecycle) => {
                lifecycle
                    .read(cx)
                    .state_of(&platform_integration(platform).builtin_id())
                    == LifecycleState::Running
            }
            None => true,
        }
    }

    fn source_for(&self, platform: Platform) -> Option<Arc<dyn BanListSource>> {
        self.builtins
            .get(&platform_integration(platform).builtin_id())
            .and_then(|object| object.ban_list)
    }

    fn group_mut(&mut self, platform: Platform) -> Option<&mut BanGroup> {
        self.groups
            .iter_mut()
            .find(|group| group.platform == platform)
    }

    fn sync_platforms(&mut self, cx: &mut Context<Self>) {
        let shown: Vec<Platform> = PLATFORM_ORDER
            .into_iter()
            .filter(|platform| {
                self.platform_running(*platform, cx) && self.source_for(*platform).is_some()
            })
            .collect();
        self.groups.retain(|group| shown.contains(&group.platform));
        for platform in shown {
            if self.group_mut(platform).is_none() {
                self.relist(platform, cx);
            }
        }
        cx.notify();
    }

    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        if visible {
            let stale: Vec<Platform> = self
                .groups
                .iter()
                .filter(|group| group.stale)
                .map(|group| group.platform)
                .collect();
            for platform in stale {
                self.list_group(platform, None, cx);
            }
        }
        cx.notify();
    }

    fn relist(&mut self, platform: Platform, cx: &mut Context<Self>) {
        if self.visible {
            self.list_group(platform, None, cx);
        } else if self.source_for(platform).is_some() {
            self.ensure_group(platform).stale = true;
        }
    }

    fn schedule_relist(&mut self, platform: Platform, cx: &mut Context<Self>) {
        if !self.visible {
            self.relist(platform, cx);
            return;
        }
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(BAN_RELIST_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| this.relist(platform, cx));
        });
        if let Some(group) = self.group_mut(platform) {
            group._relist = Some(task);
        }
    }

    fn ensure_group(&mut self, platform: Platform) -> &mut BanGroup {
        let rank = |platform: Platform| {
            PLATFORM_ORDER
                .iter()
                .position(|candidate| *candidate == platform)
                .unwrap_or(PLATFORM_ORDER.len())
        };
        let index = match self
            .groups
            .iter()
            .position(|group| group.platform == platform)
        {
            Some(index) => index,
            None => {
                let index = self
                    .groups
                    .iter()
                    .take_while(|group| rank(group.platform) < rank(platform))
                    .count();
                self.groups.insert(index, BanGroup::new(platform));
                index
            }
        };
        &mut self.groups[index]
    }

    fn list_group(
        &mut self,
        platform: Platform,
        after: Option<BanPageToken>,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = self.source_for(platform) else {
            self.groups.retain(|group| group.platform != platform);
            cx.notify();
            return;
        };
        self.last_request += 1;
        let request_id = self.last_request;
        let appending = after.is_some();
        let depth = match self.group_mut(platform) {
            Some(group) if !appending => group.pages,
            _ => 1,
        };

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let result = list_pages(source.as_ref(), after, depth).await;
            let _ = tx.send(result);
        });
        let request = cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| {
                    this.apply_listed(platform, request_id, appending, result, cx);
                });
            }
        });

        let group = self.ensure_group(platform);
        group.request_id = request_id;
        group.loading_more = appending;
        if !appending {
            group.stale = false;
            if group.status != GroupStatus::Loaded {
                group.status = GroupStatus::Loading;
            }
        }
        group._request = Some(request);
        cx.notify();
    }

    pub(crate) fn apply_listed(
        &mut self,
        platform: Platform,
        request_id: u64,
        appending: bool,
        result: ListResult,
        cx: &mut Context<Self>,
    ) {
        let now = cx.background_executor().now();
        let Some(group) = self.group_mut(platform) else {
            return;
        };
        if group.request_id != request_id {
            return;
        }
        let result = group.without_recent_unbans(result, now);
        group.apply(appending, result);
        cx.notify();
    }

    fn load_more(&mut self, platform: Platform, cx: &mut Context<Self>) {
        let Some(next) = self
            .group_mut(platform)
            .filter(|group| !group.loading_more)
            .and_then(|group| group.next.clone())
        else {
            return;
        };
        self.list_group(platform, Some(next), cx);
    }

    fn retry(&mut self, platform: Platform, cx: &mut Context<Self>) {
        if let Some(group) = self.group_mut(platform) {
            group.status = GroupStatus::Loading;
        }
        self.list_group(platform, None, cx);
    }

    fn unban(&mut self, platform: Platform, viewer_id: String, cx: &mut Context<Self>) {
        let Some(row) = self
            .group_mut(platform)
            .and_then(|group| group.row_mut(&viewer_id))
        else {
            return;
        };
        if !row.can_unban(platform) {
            return;
        }
        let target = row.target(platform);
        let Some(step) = target.step(&ViewerAction::Unban) else {
            return;
        };
        row.unban = UnbanState::Running;

        let engine = self.action_engine.clone();
        let builtin_id = target.builtin_id();
        let label = target.label(&ViewerAction::Unban);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let outcome = async_bridge::run_quick_step(engine, step, builtin_id, label).await;
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| {
            let outcome = rx
                .await
                .unwrap_or_else(|_| Err(tr!("chat_dispatch_cancelled")));
            let _ = this.update(cx, |this, cx| {
                this.apply_unban(platform, &viewer_id, outcome, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn apply_unban(
        &mut self,
        platform: Platform,
        viewer_id: &str,
        outcome: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        let now = cx.background_executor().now();
        let Some(group) = self.group_mut(platform) else {
            return;
        };
        match outcome {
            Ok(()) => {
                group.rows.retain(|row| row.entry.viewer_id != viewer_id);
                group
                    .recently_unbanned
                    .push((viewer_id.to_owned(), now + UNBAN_RELIST_SUPPRESSION));
            }
            Err(error) => {
                if let Some(row) = group.row_mut(viewer_id) {
                    row.unban = UnbanState::Failed(error);
                }
            }
        }
        cx.notify();
    }

    fn render_note(text: impl Into<SharedString>, color: gpui::Rgba) -> AnyElement {
        div()
            .font_family(body_family())
            .text_size(FONT_XS)
            .text_color(color)
            .child(text.into())
            .into_any_element()
    }

    fn render_group_header(
        group: &BanGroup,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let kind = match group.platform {
            Platform::Twitch => PlatformKind::Twitch,
            Platform::YouTube => PlatformKind::YouTube,
            Platform::Kick => PlatformKind::Kick,
        };
        let count = match group.status {
            GroupStatus::Loaded => group.rows.len().to_string(),
            GroupStatus::Loading | GroupStatus::Unavailable(_) => String::new(),
        };
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xs, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(status_dot(platform_color(kind, palette), GROUP_DOT))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(palette.text_primary)
                    .child(platform_display_name(group.platform)),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(count),
            )
            .into_any_element()
    }

    fn render_group_notes(
        platform: Platform,
        palette: &ForgePalette,
        density: Density,
    ) -> Option<AnyElement> {
        let disclaimer = disclaimer_line(&platform_integration(platform).builtin_id(), palette);
        let seen_note = lists_only_what_forge_saw(platform).then(|| {
            div()
                .font_family(body_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(tr!("chat_banned_seen_by_forge"))
                .into_any_element()
        });
        if disclaimer.is_none() && seen_note.is_none() {
            return None;
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xxs, density))
                .px(spacing(Spacing::Sm, density))
                .py(spacing(Spacing::Xs, density))
                .border_b(BORDER_THIN)
                .border_color(palette.border_regular)
                .children(seen_note)
                .children(disclaimer)
                .into_any_element(),
        )
    }

    fn render_column_header(palette: &ForgePalette, density: Density) -> AnyElement {
        let cell = |label: String, width: Option<Pixels>| {
            let base = div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(label);
            match width {
                Some(width) => base.flex_none().w(width),
                None => base.flex_1().min_w(px(0.0)),
            }
        };
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xxs, density))
            .child(cell(tr!("chat_banned_col_viewer"), Some(NAME_COLUMN_W)))
            .child(cell(tr!("chat_banned_col_reason"), None))
            .child(cell(
                tr!("chat_banned_col_moderator"),
                Some(MODERATOR_COLUMN_W),
            ))
            .child(cell(
                tr!("chat_banned_col_banned"),
                Some(BANNED_AT_COLUMN_W),
            ))
            .child(cell(tr!("chat_banned_col_expires"), Some(EXPIRY_COLUMN_W)))
            .child(div().flex_none().w(UNBAN_COLUMN_W))
            .into_any_element()
    }

    fn render_row(
        &self,
        platform: Platform,
        row: &BanRow,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entry = &row.entry;
        let text_cell = |text: String, color: gpui::Rgba, width: Option<Pixels>| {
            let base = div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(color)
                .truncate()
                .child(text);
            match width {
                Some(width) => base.flex_none().w(width),
                None => base.flex_1().min_w(px(0.0)),
            }
        };
        let viewer_id = entry.viewer_id.clone();
        let unban_button = secondary_button(tr!("chat_banned_unban"), palette)
            .density(density)
            .disabled(!row.can_unban(platform))
            .busy(row.unban == UnbanState::Running)
            .on_click(
                SharedString::from(format!("chat-banned-unban-{platform:?}-{viewer_id}")),
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.unban(platform, viewer_id.clone(), cx);
                }),
            );
        let note = match (&entry.unban, &row.unban) {
            (_, UnbanState::Failed(error)) => Some(Self::render_note(
                tr!("chat_banned_unban_failed", error = error.clone()),
                palette.random,
            )),
            (UnbanAbility::Refused(refusal), _) => Some(Self::render_note(
                refusal_label(*refusal),
                palette.text_faint,
            )),
            (UnbanAbility::Allowed, _) => None,
        };

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xs, density))
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Sm, density))
                    .child(text_cell(
                        entry.name.clone(),
                        palette.text_primary,
                        Some(NAME_COLUMN_W),
                    ))
                    .child(text_cell(
                        optional_label(entry.reason.as_deref()),
                        palette.text_secondary,
                        None,
                    ))
                    .child(text_cell(
                        optional_label(entry.moderator.as_deref()),
                        palette.text_secondary,
                        Some(MODERATOR_COLUMN_W),
                    ))
                    .child(text_cell(
                        banned_at_label(entry.created_at),
                        palette.text_muted,
                        Some(BANNED_AT_COLUMN_W),
                    ))
                    .child(text_cell(
                        expiry_label(entry.duration, row.expiry_offset),
                        match entry.duration {
                            BanDuration::Permanent => palette.random,
                            BanDuration::Until(_) => palette.warning,
                        },
                        Some(EXPIRY_COLUMN_W),
                    ))
                    .child(
                        div()
                            .flex_none()
                            .w(UNBAN_COLUMN_W)
                            .flex()
                            .justify_end()
                            .child(unban_button),
                    ),
            )
            .children(note)
            .into_any_element()
    }

    fn render_group_body(
        &self,
        group: &BanGroup,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let platform = group.platform;
        let padded = |content: AnyElement| {
            div()
                .px(spacing(Spacing::Sm, density))
                .py(spacing(Spacing::Sm, density))
                .flex()
                .items_center()
                .gap(spacing(Spacing::Sm, density))
                .child(content)
        };
        match &group.status {
            GroupStatus::Loading => padded(
                empty_state(tr!("chat_banned_loading"), palette)
                    .density(density)
                    .loading(SharedString::from(format!(
                        "chat-banned-loading-{platform:?}"
                    )))
                    .into_any_element(),
            )
            .into_any_element(),
            GroupStatus::Unavailable(reason) => padded(Self::render_note(
                unavailable_label(*reason),
                palette.text_muted,
            ))
            .child(
                ghost_button(tr!("chat_banned_retry"), palette)
                    .density(density)
                    .on_click(
                        SharedString::from(format!("chat-banned-retry-{platform:?}")),
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.retry(platform, cx)),
                    ),
            )
            .into_any_element(),
            GroupStatus::Loaded if group.rows.is_empty() => padded(Self::render_note(
                tr!("chat_banned_empty"),
                palette.text_faint,
            ))
            .into_any_element(),
            GroupStatus::Loaded => {
                let rows: Vec<AnyElement> = group
                    .rows
                    .iter()
                    .map(|row| self.render_row(platform, row, palette, density, cx))
                    .collect();
                let more = group.next.as_ref().map(|_| {
                    let failed = group
                        .more_failed
                        .map(|reason| Self::render_note(unavailable_label(reason), palette.random));
                    padded(
                        ghost_button(tr!("chat_banned_load_more"), palette)
                            .density(density)
                            .busy(group.loading_more)
                            .on_click(
                                SharedString::from(format!("chat-banned-more-{platform:?}")),
                                cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.load_more(platform, cx);
                                }),
                            )
                            .into_any_element(),
                    )
                    .border_t(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .children(failed)
                });
                div()
                    .flex()
                    .flex_col()
                    .child(Self::render_column_header(palette, density))
                    .children(rows)
                    .children(more)
                    .into_any_element()
            }
        }
    }
}

impl Render for BannedPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let mut content = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .p(spacing(Spacing::Md, density));
        if self.groups.is_empty() {
            content = content.child(
                empty_state(tr!("chat_banned_no_platform"), &palette)
                    .density(density)
                    .glyph(Icon::Ban),
            );
        }
        let groups: Vec<AnyElement> = self
            .groups
            .iter()
            .map(|group| {
                div()
                    .flex()
                    .flex_col()
                    .rounded(radius(Radius::Md))
                    .border(BORDER_THIN)
                    .border_color(palette.border_regular)
                    .bg(palette.shell)
                    .overflow_hidden()
                    .child(Self::render_group_header(group, &palette, density))
                    .children(Self::render_group_notes(group.platform, &palette, density))
                    .child(self.render_group_body(group, &palette, density, cx))
                    .into_any_element()
            })
            .collect();
        div()
            .id("chat-banned-panel")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .child(content.children(groups))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(super) mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use forge_components::Platform;
    use forge_events::{Event, EventSource};
    use forge_platform_core::{
        BanDuration, BanEntry, BanListOutcome, BanListSource, BanListUnavailable, BanPage,
        BanPageToken, UnbanAbility, UnbanRefusal,
    };
    use forge_registry::{
        FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionRegistry,
        SubActionRunner,
    };
    use forge_runtime::{ActionCancelRegistry, ActionEngineHandle, EventBus, spawn_action_engine};
    use forge_storage::history::MockHistoryRepo;
    use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
    use gpui::{AppContext as _, Entity, TestAppContext};

    use super::{
        BAN_RELIST_DEBOUNCE, BanGroup, BanRow, BannedPanel, GroupStatus, ListResult, UnbanState,
    };
    use crate::chat::tests::chat_states;
    use crate::home_stats::Integration;
    use crate::integration_lifecycle::IntegrationLifecycle;
    use crate::integration_supervisor::LifecycleState;
    use crate::integrations::{BuiltinObject, BuiltinRegistry};
    use crate::test_support::{StubActions, StubEventLog, pump, runtime, switch_lifecycle};
    use crate::unavailable_builtin::unavailable_builtin;

    const UNBAN_KIND: &str = "twitch.moderation.unban_user";
    const SETTLE_ROUNDS: usize = 4;

    pub(crate) struct FakeBans {
        asked: Mutex<Vec<Option<String>>>,
        answers: Mutex<VecDeque<BanListOutcome>>,
    }

    impl FakeBans {
        pub(crate) fn answering(answers: Vec<BanListOutcome>) -> Arc<Self> {
            Arc::new(Self {
                asked: Mutex::new(Vec::new()),
                answers: Mutex::new(answers.into()),
            })
        }

        pub(crate) fn asked(&self) -> Vec<Option<String>> {
            self.asked.lock().unwrap().clone()
        }

        pub(crate) fn calls(&self) -> usize {
            self.asked.lock().unwrap().len()
        }
    }

    #[async_trait::async_trait]
    impl BanListSource for FakeBans {
        async fn list_bans(&self, after: Option<&BanPageToken>) -> BanListOutcome {
            self.asked
                .lock()
                .unwrap()
                .push(after.map(|token| token.as_str().to_owned()));
            self.answers
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| outcome(&["1"], None))
        }
    }

    struct UnbanRunner {
        seen: Arc<Mutex<Vec<SubActionConfig>>>,
        outcome: SubActionOutcome,
    }

    #[async_trait::async_trait]
    impl SubActionRunner for UnbanRunner {
        fn id(&self) -> &str {
            UNBAN_KIND
        }
        fn category(&self) -> SubActionCategory {
            SubActionCategory::Util
        }
        fn label(&self) -> &str {
            UNBAN_KIND
        }
        fn summary(&self) -> &str {
            ""
        }
        fn search_text(&self) -> &str {
            ""
        }
        fn icon_name(&self) -> &str {
            ""
        }
        fn default_config(&self) -> SubActionConfig {
            SubActionConfig::new()
        }
        fn config_fields(&self) -> Vec<FormField> {
            Vec::new()
        }
        fn validate_config(&self, _: &SubActionConfig) -> Result<(), RegistryError> {
            Ok(())
        }
        async fn execute(
            &self,
            config: &SubActionConfig,
            ctx: &RunContext<'_>,
        ) -> (SubActionTelemetry, Option<ArgStack>) {
            self.seen.lock().unwrap().push(config.clone());
            (
                StepTimer::start(ctx, UNBAN_KIND).finish(self.outcome.clone()),
                None,
            )
        }
    }

    pub(crate) fn entry(viewer_id: &str, unban: UnbanAbility) -> BanEntry {
        BanEntry {
            viewer_id: viewer_id.to_owned(),
            name: format!("viewer{viewer_id}"),
            reason: None,
            moderator: None,
            created_at: None,
            duration: BanDuration::Permanent,
            unban,
        }
    }

    pub(crate) fn outcome(ids: &[&str], next: Option<&str>) -> BanListOutcome {
        BanListOutcome::Page(BanPage {
            entries: ids
                .iter()
                .map(|id| entry(id, UnbanAbility::Allowed))
                .collect(),
            next: next.map(BanPageToken::new),
        })
    }

    fn page(ids: &[&str], next: Option<&str>) -> ListResult {
        ListResult::of(outcome(ids, next))
    }

    fn ids(group: &BanGroup) -> Vec<String> {
        group
            .rows
            .iter()
            .map(|row| row.entry.viewer_id.clone())
            .collect()
    }

    fn loaded(ids: &[&str], next: Option<&str>) -> BanGroup {
        let mut group = BanGroup::new(Platform::Twitch);
        group.apply(false, page(ids, next));
        group
    }

    fn engine_with(registry: SubActionRegistry) -> ActionEngineHandle {
        let mut history = MockHistoryRepo::new();
        history.expect_save().returning(|_| Ok(()));
        spawn_action_engine(
            EventBus::new(Arc::new(StubEventLog)),
            crate::test_support::stub_catalog(),
            Arc::new(StubActions),
            Arc::new(history),
            Arc::new(registry),
            Arc::new(ActionCancelRegistry::new()),
        )
    }

    fn unban_engine(
        outcome: SubActionOutcome,
    ) -> (ActionEngineHandle, Arc<Mutex<Vec<SubActionConfig>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut registry = SubActionRegistry::new();
        registry
            .register(Box::new(UnbanRunner {
                seen: Arc::clone(&seen),
                outcome,
            }))
            .expect("one unban runner");
        (engine_with(registry), seen)
    }

    pub(crate) fn registry_with(
        sources: &[(Integration, Option<Arc<FakeBans>>)],
    ) -> BuiltinRegistry {
        let registry = BuiltinRegistry::default();
        for (integration, source) in sources {
            registry.install(BuiltinObject {
                ban_list: source
                    .clone()
                    .map(|source| source as Arc<dyn BanListSource>),
                ..unavailable_builtin(&integration.builtin_id())
            });
        }
        registry
    }

    struct Mount {
        builtins: BuiltinRegistry,
        lifecycle: Option<Entity<IntegrationLifecycle>>,
        bus: Option<Arc<EventBus>>,
        engine: Option<ActionEngineHandle>,
    }

    impl Mount {
        fn over(builtins: BuiltinRegistry) -> Self {
            Self {
                builtins,
                lifecycle: None,
                bus: None,
                engine: None,
            }
        }

        fn open(
            self,
            cx: &mut TestAppContext,
            rt: &tokio::runtime::Runtime,
        ) -> Entity<BannedPanel> {
            let _enter = rt.enter();
            let engine = self
                .engine
                .unwrap_or_else(|| engine_with(SubActionRegistry::new()));
            let panel = cx.new(|cx| {
                BannedPanel::new(
                    self.builtins,
                    self.lifecycle,
                    self.bus,
                    rt.handle().clone(),
                    engine,
                    cx,
                )
            });
            settle(cx, rt);
            panel
        }
    }

    pub(crate) fn settle(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
        for _ in 0..SETTLE_ROUNDS {
            cx.run_until_parked();
            pump(rt);
        }
        cx.run_until_parked();
    }

    fn shown(cx: &mut TestAppContext, panel: &Entity<BannedPanel>) -> Vec<Platform> {
        panel.read_with(cx, |panel, _| {
            panel.groups.iter().map(|group| group.platform).collect()
        })
    }

    fn twitch_ids(cx: &mut TestAppContext, panel: &Entity<BannedPanel>) -> Vec<String> {
        panel.read_with(cx, |panel, _| {
            panel
                .groups
                .iter()
                .find(|group| group.platform == Platform::Twitch)
                .map(ids)
                .unwrap_or_default()
        })
    }

    fn twitch_row_state(
        cx: &mut TestAppContext,
        panel: &Entity<BannedPanel>,
        viewer_id: &str,
    ) -> Option<UnbanState> {
        panel.read_with(cx, |panel, _| {
            panel
                .groups
                .iter()
                .find(|group| group.platform == Platform::Twitch)
                .and_then(|group| {
                    group
                        .rows
                        .iter()
                        .find(|row| row.entry.viewer_id == viewer_id)
                })
                .map(|row| row.unban.clone())
        })
    }

    fn ban_event(kind: &str) -> Event {
        Event::new(EventSource::Twitch, kind, serde_json::Value::Null)
    }

    #[test]
    fn a_first_page_replaces_the_rows_and_marks_the_group_loaded() {
        let mut group = loaded(&["1", "2"], Some("p2"));

        group.apply(false, page(&["3"], None));

        assert_eq!(ids(&group), ["3"]);
        assert_eq!(group.status, GroupStatus::Loaded);
        assert_eq!(group.next, None);
    }

    #[test]
    fn load_more_appends_only_viewers_not_already_listed() {
        let mut group = loaded(&["1", "2"], Some("p2"));

        group.apply(true, page(&["2", "3", "1", "4"], Some("p3")));

        assert_eq!(ids(&group), ["1", "2", "3", "4"]);
        assert_eq!(group.next, Some(BanPageToken::new("p3")));
    }

    #[test]
    fn a_relist_keeps_each_viewers_unban_state_by_viewer_id() {
        let mut group = loaded(&["1", "2", "3"], None);
        group.rows[0].unban = UnbanState::Running;
        group.rows[1].unban = UnbanState::Failed("rate limited".to_owned());

        group.apply(false, page(&["4", "2", "1"], None));

        let states: Vec<(String, UnbanState)> = group
            .rows
            .iter()
            .map(|row| (row.entry.viewer_id.clone(), row.unban.clone()))
            .collect();
        assert_eq!(
            states,
            [
                ("4".to_owned(), UnbanState::Idle),
                (
                    "2".to_owned(),
                    UnbanState::Failed("rate limited".to_owned())
                ),
                ("1".to_owned(), UnbanState::Running),
            ]
        );
    }

    #[test]
    fn a_failed_load_more_keeps_the_rows_and_the_next_page_and_reports_under_the_button() {
        let mut group = loaded(&["1"], Some("p2"));
        group.loading_more = true;

        group.apply(true, ListResult::Unavailable(BanListUnavailable::Transport));

        assert_eq!(ids(&group), ["1"]);
        assert_eq!(group.next, Some(BanPageToken::new("p2")));
        assert_eq!(group.status, GroupStatus::Loaded);
        assert_eq!(group.more_failed, Some(BanListUnavailable::Transport));
        assert!(!group.loading_more);
    }

    #[test]
    fn a_successful_load_more_clears_the_previous_load_more_error() {
        let mut group = loaded(&["1"], Some("p2"));
        group.apply(
            true,
            ListResult::Unavailable(BanListUnavailable::QuotaExhausted),
        );

        group.apply(true, page(&["2"], None));

        assert_eq!(group.more_failed, None);
        assert_eq!(ids(&group), ["1", "2"]);
    }

    #[test]
    fn an_unavailable_first_page_drops_the_rows_and_the_next_page() {
        let mut group = loaded(&["1"], Some("p2"));

        group.apply(
            false,
            ListResult::Unavailable(BanListUnavailable::MissingScope),
        );

        assert!(group.rows.is_empty());
        assert_eq!(group.next, None);
        assert_eq!(
            group.status,
            GroupStatus::Unavailable(BanListUnavailable::MissingScope)
        );
    }

    #[test]
    fn can_unban_only_an_allowed_idle_or_failed_twitch_row_with_a_viewer_id() {
        let refused = UnbanAbility::Refused(UnbanRefusal::BannedOutsideForge);
        for (platform, viewer_id, ability, state, expected) in [
            (
                Platform::Twitch,
                "1",
                UnbanAbility::Allowed,
                UnbanState::Idle,
                true,
            ),
            (
                Platform::Twitch,
                "1",
                UnbanAbility::Allowed,
                UnbanState::Failed("x".to_owned()),
                true,
            ),
            (
                Platform::Twitch,
                "1",
                UnbanAbility::Allowed,
                UnbanState::Running,
                false,
            ),
            (Platform::Twitch, "1", refused, UnbanState::Idle, false),
            (
                Platform::Twitch,
                "",
                UnbanAbility::Allowed,
                UnbanState::Idle,
                false,
            ),
            (
                Platform::YouTube,
                "UCabc",
                UnbanAbility::Allowed,
                UnbanState::Idle,
                false,
            ),
            (
                Platform::Kick,
                "4242",
                UnbanAbility::Allowed,
                UnbanState::Idle,
                false,
            ),
        ] {
            let mut row = BanRow::listed(entry(viewer_id, ability));
            row.unban = state.clone();

            assert_eq!(
                row.can_unban(platform),
                expected,
                "{platform:?} {viewer_id:?} {ability:?} {state:?}"
            );
        }
    }

    #[gpui::test]
    fn only_running_platforms_that_offer_a_ban_list_get_a_group(cx: &mut TestAppContext) {
        let rt = runtime();
        let twitch = FakeBans::answering(Vec::new());
        let youtube = FakeBans::answering(Vec::new());
        let lifecycle = cx.new(|_| {
            IntegrationLifecycle::new(chat_states(&[
                (Integration::Twitch, LifecycleState::Running),
                (
                    Integration::YouTube,
                    LifecycleState::Failed("quota".to_owned()),
                ),
                (Integration::Kick, LifecycleState::Running),
            ]))
        });
        let mut mount = Mount::over(registry_with(&[
            (Integration::Twitch, Some(Arc::clone(&twitch))),
            (Integration::YouTube, Some(Arc::clone(&youtube))),
            (Integration::Kick, None),
        ]));
        mount.lifecycle = Some(lifecycle);

        let panel = mount.open(cx, &rt);

        assert_eq!(shown(cx, &panel), [Platform::Twitch]);
        assert_eq!(twitch.asked(), [None]);
        assert_eq!(youtube.calls(), 0);
    }

    #[gpui::test]
    fn groups_follow_platform_order_whatever_order_the_builtins_were_installed(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let sources = [
            (Integration::Kick, Some(FakeBans::answering(Vec::new()))),
            (Integration::Twitch, Some(FakeBans::answering(Vec::new()))),
            (Integration::YouTube, Some(FakeBans::answering(Vec::new()))),
        ];

        let panel = Mount::over(registry_with(&sources)).open(cx, &rt);

        assert_eq!(
            shown(cx, &panel),
            [Platform::Twitch, Platform::YouTube, Platform::Kick]
        );
    }

    #[gpui::test]
    fn the_first_page_is_listed_without_a_token_and_shown_loaded(cx: &mut TestAppContext) {
        let rt = runtime();
        let twitch = FakeBans::answering(vec![outcome(&["7", "8"], Some("p2"))]);

        let panel = Mount::over(registry_with(&[(
            Integration::Twitch,
            Some(Arc::clone(&twitch)),
        )]))
        .open(cx, &rt);

        assert_eq!(twitch.asked(), [None]);
        assert_eq!(twitch_ids(cx, &panel), ["7", "8"]);
        assert_eq!(
            panel.read_with(cx, |panel, _| panel.groups[0].status.clone()),
            GroupStatus::Loaded
        );
    }

    #[gpui::test]
    fn load_more_asks_for_the_next_token_and_appends_without_duplicates(cx: &mut TestAppContext) {
        let rt = runtime();
        let twitch = FakeBans::answering(vec![
            outcome(&["1", "2"], Some("p2")),
            outcome(&["2", "3"], None),
        ]);
        let panel = Mount::over(registry_with(&[(
            Integration::Twitch,
            Some(Arc::clone(&twitch)),
        )]))
        .open(cx, &rt);

        panel.update(cx, |panel, cx| panel.load_more(Platform::Twitch, cx));
        settle(cx, &rt);

        assert_eq!(twitch.asked(), [None, Some("p2".to_owned())]);
        assert_eq!(twitch_ids(cx, &panel), ["1", "2", "3"]);
    }

    #[gpui::test]
    fn load_more_on_the_last_page_or_while_one_is_in_flight_asks_nothing(cx: &mut TestAppContext) {
        let rt = runtime();
        let last = FakeBans::answering(vec![outcome(&["1"], None)]);
        let panel = Mount::over(registry_with(&[(
            Integration::Twitch,
            Some(Arc::clone(&last)),
        )]))
        .open(cx, &rt);
        panel.update(cx, |panel, cx| panel.load_more(Platform::Twitch, cx));
        settle(cx, &rt);
        assert_eq!(last.calls(), 1);

        let paged = FakeBans::answering(vec![outcome(&["1"], Some("p2"))]);
        let panel = Mount::over(registry_with(&[(
            Integration::Twitch,
            Some(Arc::clone(&paged)),
        )]))
        .open(cx, &rt);
        panel.update(cx, |panel, cx| {
            panel.load_more(Platform::Twitch, cx);
            panel.load_more(Platform::Twitch, cx);
        });
        settle(cx, &rt);
        assert_eq!(paged.calls(), 2);
    }

    #[gpui::test]
    fn a_result_for_a_superseded_request_is_dropped(cx: &mut TestAppContext) {
        let rt = runtime();
        let twitch = FakeBans::answering(vec![outcome(&["1"], None)]);
        let panel = Mount::over(registry_with(&[(
            Integration::Twitch,
            Some(Arc::clone(&twitch)),
        )]))
        .open(cx, &rt);
        let current = panel.read_with(cx, |panel, _| panel.groups[0].request_id);

        panel.update(cx, |panel, cx| {
            panel.apply_listed(Platform::Twitch, current - 1, false, page(&["9"], None), cx);
        });
        assert_eq!(twitch_ids(cx, &panel), ["1"]);

        panel.update(cx, |panel, cx| {
            panel.apply_listed(Platform::Twitch, current, false, page(&["9"], None), cx);
        });
        assert_eq!(twitch_ids(cx, &panel), ["9"]);
    }

    #[gpui::test]
    fn retry_after_an_unavailable_list_loads_the_group(cx: &mut TestAppContext) {
        let rt = runtime();
        let twitch = FakeBans::answering(vec![
            BanListOutcome::Unavailable(BanListUnavailable::Transport),
            outcome(&["5"], None),
        ]);
        let panel = Mount::over(registry_with(&[(
            Integration::Twitch,
            Some(Arc::clone(&twitch)),
        )]))
        .open(cx, &rt);
        assert_eq!(
            panel.read_with(cx, |panel, _| panel.groups[0].status.clone()),
            GroupStatus::Unavailable(BanListUnavailable::Transport)
        );

        panel.update(cx, |panel, cx| panel.retry(Platform::Twitch, cx));
        settle(cx, &rt);

        assert_eq!(twitch.asked(), [None, None]);
        assert_eq!(twitch_ids(cx, &panel), ["5"]);
    }

    #[gpui::test]
    fn a_burst_of_ban_events_relists_the_platform_once_after_the_quiet_debounce(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let bus = EventBus::new(Arc::new(StubEventLog));
        let twitch = FakeBans::answering(Vec::new());
        let mut mount = Mount::over(registry_with(&[(
            Integration::Twitch,
            Some(Arc::clone(&twitch)),
        )]));
        mount.bus = Some(Arc::clone(&bus));
        let _panel = mount.open(cx, &rt);
        let half = BAN_RELIST_DEBOUNCE / 2;

        bus.publish(ban_event("twitch.channel.ban"));
        settle(cx, &rt);
        cx.executor().advance_clock(half);
        settle(cx, &rt);
        bus.publish(ban_event("twitch.channel.unban"));
        settle(cx, &rt);
        cx.executor().advance_clock(half);
        settle(cx, &rt);
        assert_eq!(twitch.calls(), 1);

        cx.executor().advance_clock(half + Duration::from_millis(1));
        settle(cx, &rt);
        assert_eq!(twitch.calls(), 2);

        cx.executor().advance_clock(BAN_RELIST_DEBOUNCE * 4);
        settle(cx, &rt);
        assert_eq!(twitch.calls(), 2);
    }

    #[gpui::test]
    fn events_that_are_not_twitch_ban_changes_never_relist_twitch(cx: &mut TestAppContext) {
        let rt = runtime();
        let bus = EventBus::new(Arc::new(StubEventLog));
        let twitch = FakeBans::answering(Vec::new());
        let mut mount = Mount::over(registry_with(&[(
            Integration::Twitch,
            Some(Arc::clone(&twitch)),
        )]));
        mount.bus = Some(Arc::clone(&bus));
        let _panel = mount.open(cx, &rt);

        bus.publish(ban_event("twitch.chat.message"));
        bus.publish(ban_event("kick.moderation.banned"));
        settle(cx, &rt);
        cx.executor().advance_clock(BAN_RELIST_DEBOUNCE * 2);
        settle(cx, &rt);

        assert_eq!(twitch.calls(), 1);
    }

    #[gpui::test]
    fn a_platform_restart_reloads_its_ban_list(cx: &mut TestAppContext) {
        let rt = runtime();
        let twitch = FakeBans::answering(vec![outcome(&["1"], None), outcome(&["2"], None)]);
        let lifecycle = cx.new(|_| {
            IntegrationLifecycle::new(chat_states(&[(
                Integration::Twitch,
                LifecycleState::Running,
            )]))
        });
        let mut mount = Mount::over(registry_with(&[(
            Integration::Twitch,
            Some(Arc::clone(&twitch)),
        )]));
        mount.lifecycle = Some(lifecycle.clone());
        let panel = mount.open(cx, &rt);

        switch_lifecycle(
            cx,
            &lifecycle,
            chat_states(&[(Integration::Twitch, LifecycleState::Starting)]),
        );
        settle(cx, &rt);
        assert!(shown(cx, &panel).is_empty());

        switch_lifecycle(
            cx,
            &lifecycle,
            chat_states(&[(Integration::Twitch, LifecycleState::Running)]),
        );
        settle(cx, &rt);

        assert_eq!(twitch.calls(), 2);
        assert_eq!(twitch_ids(cx, &panel), ["2"]);
    }

    #[gpui::test]
    fn a_lifecycle_change_on_another_platform_does_not_refetch(cx: &mut TestAppContext) {
        let rt = runtime();
        let twitch = FakeBans::answering(Vec::new());
        let lifecycle = cx.new(|_| {
            IntegrationLifecycle::new(chat_states(&[
                (Integration::Twitch, LifecycleState::Running),
                (Integration::Kick, LifecycleState::Starting),
            ]))
        });
        let mut mount = Mount::over(registry_with(&[(
            Integration::Twitch,
            Some(Arc::clone(&twitch)),
        )]));
        mount.lifecycle = Some(lifecycle.clone());
        let _panel = mount.open(cx, &rt);

        switch_lifecycle(
            cx,
            &lifecycle,
            chat_states(&[
                (Integration::Twitch, LifecycleState::Running),
                (Integration::Kick, LifecycleState::Running),
            ]),
        );
        settle(cx, &rt);

        assert_eq!(twitch.calls(), 1);
    }

    fn unban_panel(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        entries: Vec<BanEntry>,
        result: SubActionOutcome,
    ) -> (Entity<BannedPanel>, Arc<Mutex<Vec<SubActionConfig>>>) {
        let twitch = FakeBans::answering(vec![BanListOutcome::Page(BanPage {
            entries,
            next: None,
        })]);
        let (engine, seen) = {
            let _enter = rt.enter();
            unban_engine(result)
        };
        let mut mount = Mount::over(registry_with(&[(Integration::Twitch, Some(twitch))]));
        mount.engine = Some(engine);
        (mount.open(cx, rt), seen)
    }

    fn unban(cx: &mut TestAppContext, panel: &Entity<BannedPanel>, viewer_id: &str) {
        let viewer_id = viewer_id.to_owned();
        panel.update(cx, |panel, cx| panel.unban(Platform::Twitch, viewer_id, cx));
    }

    #[gpui::test]
    fn unban_runs_the_twitch_unban_step_for_the_viewer_and_removes_the_row(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (panel, seen) = unban_panel(
            cx,
            &rt,
            vec![
                entry("1001", UnbanAbility::Allowed),
                entry("1002", UnbanAbility::Allowed),
            ],
            SubActionOutcome::Success,
        );

        unban(cx, &panel, "1001");
        settle(cx, &rt);

        assert_eq!(
            *seen.lock().unwrap(),
            [SubActionConfig::from([(
                "target_user_id".to_owned(),
                Variant::String("1001".to_owned())
            )])]
        );
        assert_eq!(twitch_ids(cx, &panel), ["1002"]);
    }

    #[gpui::test]
    fn a_failed_unban_keeps_the_row_and_marks_only_that_row_failed(cx: &mut TestAppContext) {
        let rt = runtime();
        let (panel, _seen) = unban_panel(
            cx,
            &rt,
            vec![
                entry("1001", UnbanAbility::Allowed),
                entry("1002", UnbanAbility::Allowed),
            ],
            SubActionOutcome::Failed("missing scope".to_owned()),
        );

        unban(cx, &panel, "1001");
        settle(cx, &rt);

        assert_eq!(twitch_ids(cx, &panel), ["1001", "1002"]);
        assert_eq!(
            twitch_row_state(cx, &panel, "1001"),
            Some(UnbanState::Failed("missing scope".to_owned()))
        );
        assert_eq!(twitch_row_state(cx, &panel, "1002"), Some(UnbanState::Idle));
    }

    #[gpui::test]
    fn a_second_unban_click_while_one_runs_dispatches_nothing_more(cx: &mut TestAppContext) {
        let rt = runtime();
        let (panel, seen) = unban_panel(
            cx,
            &rt,
            vec![entry("1001", UnbanAbility::Allowed)],
            SubActionOutcome::Failed("busy".to_owned()),
        );

        unban(cx, &panel, "1001");
        assert_eq!(
            twitch_row_state(cx, &panel, "1001"),
            Some(UnbanState::Running)
        );
        unban(cx, &panel, "1001");
        settle(cx, &rt);

        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[gpui::test]
    fn a_ban_placed_outside_forge_is_never_unbanned(cx: &mut TestAppContext) {
        let rt = runtime();
        let (panel, seen) = unban_panel(
            cx,
            &rt,
            vec![entry(
                "1001",
                UnbanAbility::Refused(UnbanRefusal::BannedOutsideForge),
            )],
            SubActionOutcome::Success,
        );

        unban(cx, &panel, "1001");
        settle(cx, &rt);

        assert!(seen.lock().unwrap().is_empty());
        assert_eq!(twitch_row_state(cx, &panel, "1001"), Some(UnbanState::Idle));
    }
}
