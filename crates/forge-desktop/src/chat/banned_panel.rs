use std::sync::Arc;
use std::time::Duration;

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
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::integration_supervisor::LifecycleState;
use crate::integrations::BuiltinRegistry;
use crate::presentation::ActivePresentation;
use crate::scheduled_run_labels::system_offset_at;

const BAN_RELIST_DEBOUNCE: Duration = Duration::from_millis(750);
const PLATFORM_ORDER: [Platform; 3] = [Platform::Twitch, Platform::YouTube, Platform::Kick];
const GROUP_DOT: Pixels = px(6.0);
const NAME_COLUMN_W: Pixels = px(160.0);
const MODERATOR_COLUMN_W: Pixels = px(130.0);
const BANNED_AT_COLUMN_W: Pixels = px(90.0);
const EXPIRY_COLUMN_W: Pixels = px(170.0);
const UNBAN_COLUMN_W: Pixels = px(84.0);
const DISPATCH_CANCELLED: &str = "dispatch cancelled";

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

pub(crate) struct BanGroup {
    pub platform: Platform,
    pub status: GroupStatus,
    pub rows: Vec<BanRow>,
    pub next: Option<BanPageToken>,
    pub loading_more: bool,
    pub more_failed: Option<BanListUnavailable>,
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

    fn apply(&mut self, appending: bool, result: ListResult) {
        self.loading_more = false;
        match (result, appending) {
            (ListResult::Page { rows, next }, true) => {
                self.append_rows(rows);
                self.next = next;
                self.more_failed = None;
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
                self.list_group(platform, None, cx);
            }
        }
        cx.notify();
    }

    fn schedule_relist(&mut self, platform: Platform, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(BAN_RELIST_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| this.list_group(platform, None, cx));
        });
        if let Some(group) = self.group_mut(platform) {
            group._relist = Some(task);
        }
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

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let outcome = source.list_bans(after.as_ref()).await;
            let _ = tx.send(ListResult::of(outcome));
        });
        let request = cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| {
                    this.apply_listed(platform, request_id, appending, result, cx);
                });
            }
        });

        if self.group_mut(platform).is_none() {
            let position = PLATFORM_ORDER
                .iter()
                .position(|candidate| *candidate == platform)
                .unwrap_or(PLATFORM_ORDER.len());
            let index = self
                .groups
                .iter()
                .take_while(|group| {
                    PLATFORM_ORDER
                        .iter()
                        .position(|candidate| *candidate == group.platform)
                        .unwrap_or(PLATFORM_ORDER.len())
                        < position
                })
                .count();
            self.groups.insert(index, BanGroup::new(platform));
        }
        if let Some(group) = self.group_mut(platform) {
            group.request_id = request_id;
            group.loading_more = appending;
            if !appending && group.status != GroupStatus::Loaded {
                group.status = GroupStatus::Loading;
            }
            group._request = Some(request);
        }
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
        let Some(group) = self.group_mut(platform) else {
            return;
        };
        if group.request_id != request_id {
            return;
        }
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
                .unwrap_or_else(|_| Err(DISPATCH_CANCELLED.to_owned()));
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
        let Some(group) = self.group_mut(platform) else {
            return;
        };
        match outcome {
            Ok(()) => group.rows.retain(|row| row.entry.viewer_id != viewer_id),
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
