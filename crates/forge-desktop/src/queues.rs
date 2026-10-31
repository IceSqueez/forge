mod confirms;
mod edit_modal;
mod queue_card;
mod store;
mod subheader;

pub(crate) use queue_card::running_pill;

use crate::async_bridge::{self, Loadable};
use crate::load_notice::{LoadNotice, NoticeWidth, load_notice};
use crate::navigation::NavRequested;
use crate::queue_health::QueueHealth;
use crate::repo_failure::RepoFailure;
use crate::scheduled_runs_section::ScheduledRunsView;
use crate::window_presence::PresenceGate;
use edit_modal::{EditQueueEvent, EditQueueModal, QueueDraft};
use forge_components::{
    ActivePresentation, BreadcrumbCrumb, Confirm, FONT_FIELD, FONT_LABEL, FONT_META, ForgePalette,
    Icon, InputEvent, OverflowRevealingFocus, PushToast, SearchState, Spacing, TextInput,
    ToastKind, body_family, empty_state, fmt_uptime, focus_inside_modal, page_frame, spacing, tr,
};
use forge_runtime::{
    MembershipOutcome, QueueIntake, QueueMode, QueueProcessing, QueueSchedulerHandle, RunningAction,
};
use forge_storage::{ActionRepo, QueueRepo};
use forge_types::{ActionId, DEFAULT_QUEUE_ID, Queue, QueueId};
use gpui::prelude::*;
use gpui::{
    ClickEvent, Context, Entity, EventEmitter, Pixels, Point, Rgba, SharedString, Subscription,
    Task, Window, div, px,
};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};
use store::{apply_queue_live, delete_queue, load_queues};

const BADGE_RADIUS: Pixels = px(8.0);
const PILL_RADIUS: Pixels = px(5.0);
const CONCURRENCY_BOX_RADIUS: Pixels = px(7.0);
const CONCURRENCY_VALUE_W: Pixels = px(28.0);
const BADGE_GLYPH: Pixels = px(9.0);
const PANEL_GLYPH: Pixels = px(12.0);
const CARDS_PER_ROW: usize = 2;
const STATS_FS: Pixels = FONT_LABEL;
const CARD_PAD: Pixels = px(14.0);
const HEADER_GAP: Pixels = px(12.0);
const BAR_GAP: Pixels = px(8.0);
const STAT_VALUE_FS: Pixels = FONT_FIELD;
const DESC_FS: Pixels = FONT_META;
const MAX_PILLS: usize = 3;
const RUNNING_ELAPSED_TICK: Duration = Duration::from_millis(250);
const UNNAMED_ACTION: &str = "-";
const SERIAL_CONCURRENCY: u32 = 1;
const PARALLEL_CONCURRENCY: u32 = 8;
const MIN_CONCURRENCY: u32 = 1;
const MAX_CONCURRENCY: u32 = 16;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum QueueFilter {
    #[default]
    All,
    Running,
    Paused,
    Parallel,
    Sequential,
}

impl QueueFilter {
    const TABS: [(&'static str, QueueFilter); 5] = [
        ("queue-filter-all", QueueFilter::All),
        ("queue-filter-running", QueueFilter::Running),
        ("queue-filter-paused", QueueFilter::Paused),
        ("queue-filter-parallel", QueueFilter::Parallel),
        ("queue-filter-sequential", QueueFilter::Sequential),
    ];

    fn keeps(self, row: &QueueRow) -> bool {
        match self {
            QueueFilter::All => true,
            QueueFilter::Running => !row.frozen(),
            QueueFilter::Paused => row.frozen(),
            QueueFilter::Parallel => !row.blocking,
            QueueFilter::Sequential => row.blocking,
        }
    }

    fn label_key(self) -> &'static str {
        match self {
            QueueFilter::All => "queues_filter_all",
            QueueFilter::Running => "queues_filter_running",
            QueueFilter::Paused => "queues_filter_paused",
            QueueFilter::Parallel => "queues_filter_parallel",
            QueueFilter::Sequential => "queues_filter_sequential",
        }
    }

    fn dot(self, palette: &ForgePalette) -> Rgba {
        match self {
            QueueFilter::All => palette.brand,
            QueueFilter::Running => palette.success,
            QueueFilter::Paused => palette.warning,
            QueueFilter::Parallel => palette.info,
            QueueFilter::Sequential => palette.accent_peach,
        }
    }
}

fn queue_matches(row: &QueueRow, filter: QueueFilter, search: &SearchState) -> bool {
    filter.keeps(row) && search.matches(&row.name)
}

struct ModePreset {
    mode: QueueMode,
    label_key: &'static str,
    tooltip_key: &'static str,
    element_id: &'static str,
    glyph: Icon,
}

const MODE_PRESETS: [ModePreset; 3] = [
    ModePreset {
        mode: QueueMode::PAUSED,
        label_key: "queues_pause_btn",
        tooltip_key: "queues_mode_pause_tooltip",
        element_id: "q-mode-pause",
        glyph: Icon::PlayerPause,
    },
    ModePreset {
        mode: QueueMode::DRAINING,
        label_key: "queues_drain_btn",
        tooltip_key: "queues_mode_drain_tooltip",
        element_id: "q-mode-drain",
        glyph: Icon::FilterOff,
    },
    ModePreset {
        mode: QueueMode::HOLDING,
        label_key: "queues_hold_btn",
        tooltip_key: "queues_mode_hold_tooltip",
        element_id: "q-mode-hold",
        glyph: Icon::ClockPause,
    },
];

fn mode_ink(mode: QueueMode, palette: &ForgePalette) -> Rgba {
    match (mode.processing, mode.intake) {
        (QueueProcessing::Running, QueueIntake::Accept) => palette.success,
        (QueueProcessing::Running, QueueIntake::Skip) => palette.info,
        (QueueProcessing::Frozen, QueueIntake::Accept) => palette.accent_peach,
        (QueueProcessing::Frozen, QueueIntake::Skip) => palette.warning,
    }
}

fn mode_glyph(mode: QueueMode) -> Icon {
    match (mode.processing, mode.intake) {
        (QueueProcessing::Running, QueueIntake::Accept) => Icon::PlayerPlay,
        (QueueProcessing::Running, QueueIntake::Skip) => Icon::FilterOff,
        (QueueProcessing::Frozen, QueueIntake::Accept) => Icon::ClockPause,
        (QueueProcessing::Frozen, QueueIntake::Skip) => Icon::PlayerPause,
    }
}

fn mode_badge_key(mode: QueueMode) -> &'static str {
    match (mode.processing, mode.intake) {
        (QueueProcessing::Running, QueueIntake::Accept) => "queues_status_running",
        (QueueProcessing::Running, QueueIntake::Skip) => "queues_status_draining",
        (QueueProcessing::Frozen, QueueIntake::Accept) => "queues_status_held",
        (QueueProcessing::Frozen, QueueIntake::Skip) => "queues_status_paused",
    }
}

fn mode_caption_key(mode: QueueMode) -> &'static str {
    match (mode.processing, mode.intake) {
        (QueueProcessing::Running, QueueIntake::Accept) => "queues_mode_running_caption",
        (QueueProcessing::Running, QueueIntake::Skip) => "queues_mode_drain_caption",
        (QueueProcessing::Frozen, QueueIntake::Accept) => "queues_mode_hold_caption",
        (QueueProcessing::Frozen, QueueIntake::Skip) => "queues_mode_pause_caption",
    }
}

struct QueueRow {
    id: QueueId,
    name: String,
    description: String,
    blocking: bool,
    concurrency: u32,
    mode: QueueMode,
    live: bool,
    pending: u32,
    in_flight: u32,
    overflowed: u64,
    actions: u32,
    running: Vec<RunningEntry>,
}

#[derive(Clone, PartialEq, Eq)]
struct RunningEntry {
    name: String,
    started_at: Instant,
}

impl QueueRow {
    fn concurrency_label(&self) -> SharedString {
        if self.blocking {
            SharedString::from(tr!("queues_metric_serial"))
        } else {
            SharedString::from(tr!("queues_metric_parallel"))
        }
    }

    fn frozen(&self) -> bool {
        self.mode.processing == QueueProcessing::Frozen
    }
}

struct QueueListing {
    rows: Vec<QueueRow>,
    action_names: HashMap<ActionId, String>,
}

fn running_entries(
    running: &[RunningAction],
    action_names: &HashMap<ActionId, String>,
) -> Vec<RunningEntry> {
    running
        .iter()
        .map(|run| RunningEntry {
            name: action_names
                .get(&run.action_id)
                .cloned()
                .unwrap_or_else(|| UNNAMED_ACTION.to_owned()),
            started_at: run.started_at,
        })
        .collect()
}

fn elapsed_text(elapsed: Duration) -> String {
    let millis = elapsed.as_millis();
    if millis < 1_000 {
        tr!(
            "queues_elapsed_ms",
            ms = i64::try_from(millis).unwrap_or(i64::MAX)
        )
    } else {
        fmt_uptime(elapsed.as_secs())
    }
}

pub(crate) struct QueuesView {
    queues: Vec<QueueRow>,
    list_load: Loadable<()>,
    modal: Option<Entity<EditQueueModal>>,
    _modal_sub: Option<Subscription>,
    pending_delete: Confirm<QueueId>,
    pending_free: Confirm<QueueId>,
    menu_open: Option<QueueId>,
    menu_click_pos: Option<Point<Pixels>>,
    diverged: HashSet<QueueId>,
    queue_health: Entity<QueueHealth>,
    scheduler: QueueSchedulerHandle,
    queue_repo: Arc<dyn QueueRepo>,
    action_repo: Arc<dyn ActionRepo>,
    rt_handle: tokio::runtime::Handle,
    status_filter: QueueFilter,
    search: SearchState,
    scheduled: Option<Entity<ScheduledRunsView>>,
    _scheduled_sub: Option<Subscription>,
    action_names: HashMap<ActionId, String>,
    named_lookups_requested: HashSet<ActionId>,
    now: Instant,
    _elapsed_tick: Option<Task<()>>,
    _health_obs: Subscription,
    _search_sub: Subscription,
}

impl EventEmitter<NavRequested> for QueuesView {}

impl QueuesView {
    pub(crate) fn new(
        queue_health: Entity<QueueHealth>,
        scheduler: QueueSchedulerHandle,
        queue_repo: Arc<dyn QueueRepo>,
        action_repo: Arc<dyn ActionRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let health_obs = cx.observe(&queue_health, |this, health, cx| {
            this.sync_modes(&health, cx);
            if this.apply_depths(health.read(cx)) {
                cx.notify();
            }
            this.look_up_unnamed_runs(&health, cx);
            this.sync_elapsed_tick(cx);
        });
        let search = SearchState::new(cx, palette, tr!("queues_search_placeholder"));
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);
        let view = Self {
            queues: vec![],
            list_load: Loadable::Loading,
            modal: None,
            _modal_sub: None,
            pending_delete: Confirm::default(),
            pending_free: Confirm::default(),
            menu_open: None,
            menu_click_pos: None,
            diverged: HashSet::new(),
            queue_health,
            scheduler,
            queue_repo,
            action_repo,
            rt_handle,
            status_filter: QueueFilter::default(),
            search,
            scheduled: None,
            _scheduled_sub: None,
            action_names: HashMap::new(),
            named_lookups_requested: HashSet::new(),
            now: Instant::now(),
            _elapsed_tick: None,
            _health_obs: health_obs,
            _search_sub: search_sub,
        };
        view.reload(cx);
        view
    }

    pub(crate) fn with_scheduled(
        mut self,
        section: Entity<ScheduledRunsView>,
        cx: &mut Context<Self>,
    ) -> Self {
        self._scheduled_sub = Some(cx.subscribe(
            &section,
            |_this, _section, event: &NavRequested, cx| {
                cx.emit(NavRequested(event.0.clone()));
            },
        ));
        self.scheduled = Some(section);
        self
    }

    fn on_search_event(
        &mut self,
        _f: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if self.search.on_changed(event) {
            cx.notify();
        }
    }

    fn set_status_filter(&mut self, filter: QueueFilter, cx: &mut Context<Self>) {
        self.status_filter = filter;
        cx.notify();
    }

    fn sync_modes(&mut self, health: &Entity<QueueHealth>, cx: &mut Context<Self>) {
        let mut changed = false;
        {
            let health = health.read(cx);
            for row in &mut self.queues {
                if let Some(mode) = health.mode(row.id)
                    && row.mode != mode
                {
                    row.mode = mode;
                    changed = true;
                }
            }
        }
        if changed {
            self.reload(cx);
            cx.notify();
        }
    }

    fn apply_depths(&mut self, health: &QueueHealth) -> bool {
        let mut changed = false;
        for row in &mut self.queues {
            let depth = health.depth(row.id).unwrap_or_default();
            let pending = u32::try_from(depth.pending).unwrap_or(u32::MAX);
            let in_flight = u32::try_from(depth.in_flight).unwrap_or(u32::MAX);
            let running = running_entries(health.running(row.id), &self.action_names);
            if (row.pending, row.in_flight, row.overflowed)
                != (pending, in_flight, depth.overflowed)
                || row.running != running
            {
                row.pending = pending;
                row.in_flight = in_flight;
                row.overflowed = depth.overflowed;
                row.running = running;
                changed = true;
            }
        }
        if changed {
            self.now = Instant::now();
        }
        changed
    }

    fn look_up_unnamed_runs(&mut self, health: &Entity<QueueHealth>, cx: &mut Context<Self>) {
        let unknown: Vec<ActionId> = {
            let health = health.read(cx);
            self.queues
                .iter()
                .flat_map(|row| health.running(row.id))
                .map(|run| run.action_id)
                .filter(|id| !self.action_names.contains_key(id))
                .filter(|id| !self.named_lookups_requested.contains(id))
                .collect()
        };
        if !unknown.is_empty() {
            self.named_lookups_requested.extend(unknown);
            self.reload(cx);
        }
    }

    fn sync_elapsed_tick(&mut self, cx: &mut Context<Self>) {
        let running = self.queues.iter().any(|row| !row.running.is_empty());
        if !running {
            self._elapsed_tick = None;
        } else if self._elapsed_tick.is_none() {
            self._elapsed_tick = Some(Self::spawn_elapsed_tick(cx));
        }
    }

    fn spawn_elapsed_tick(cx: &mut Context<Self>) -> Task<()> {
        let mut presence = PresenceGate::of(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(RUNNING_ELAPSED_TICK).await;
                presence.until_visible().await;
                let alive = this.update(cx, |this, cx| {
                    this.now = Instant::now();
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        })
    }

    fn visible_indices(&self) -> Vec<usize> {
        self.queues
            .iter()
            .enumerate()
            .filter(|(_, row)| queue_matches(row, self.status_filter, &self.search))
            .map(|(i, _)| i)
            .collect()
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let queue_repo = Arc::clone(&self.queue_repo);
        let action_repo = Arc::clone(&self.action_repo);
        let scheduler = self.scheduler.clone();
        async_bridge::run_async(
            &self.rt_handle,
            load_queues(queue_repo, action_repo, scheduler),
            |this, result, cx| match result {
                Ok(listing) => {
                    this.action_names = listing.action_names;
                    this.apply_rows(listing.rows, cx);
                }
                Err(failure) => this.on_load_error(failure, cx),
            },
            cx,
        );
    }

    fn apply_rows(&mut self, rows: Vec<QueueRow>, cx: &mut Context<Self>) {
        self.diverged = rows.iter().filter(|r| !r.live).map(|r| r.id).collect();
        self.queues = rows;
        self.list_load = Loadable::Loaded(());
        let health = self.queue_health.clone();
        self.apply_depths(health.read(cx));
        self.sync_elapsed_tick(cx);
        cx.notify();
    }

    fn on_load_error(&mut self, failure: RepoFailure, cx: &mut Context<Self>) {
        if !self.queues.is_empty() {
            cx.push_toast(
                ToastKind::Error,
                tr!("queues_load_failed", error = failure.text()),
            );
        }
        self.list_load = Loadable::Failed(failure);
        cx.notify();
    }

    fn retry_list_load(&mut self, cx: &mut Context<Self>) {
        self.list_load = Loadable::Loading;
        self.reload(cx);
        cx.notify();
    }

    fn log_membership_outcome(id: QueueId, outcome: &Result<MembershipOutcome, RepoFailure>) {
        match outcome {
            Ok(MembershipOutcome::Applied | MembershipOutcome::AlreadyRegistered) => {}
            Ok(MembershipOutcome::NotFound) => {
                tracing::warn!(queue = %id, "scheduler reported the queue as not found");
            }
            Err(error) => {
                tracing::warn!(queue = %id, %error, "scheduler membership call failed");
            }
        }
    }

    fn apply_live(&mut self, id: QueueId, cx: &mut Context<Self>) {
        let queue_repo = Arc::clone(&self.queue_repo);
        let scheduler = self.scheduler.clone();
        async_bridge::run_async(
            &self.rt_handle,
            apply_queue_live(queue_repo, scheduler, id),
            |this, result, cx| {
                match result {
                    Ok(()) => cx.push_toast(ToastKind::Success, tr!("queues_apply_live_done")),
                    Err(failure) => cx.push_toast(
                        ToastKind::Error,
                        tr!("queues_apply_live_failed", error = failure.message()),
                    ),
                }
                this.reload(cx);
            },
            cx,
        );
    }

    fn on_scheduler_error(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        cx.push_toast(ToastKind::Error, message);
        self.reload(cx);
    }

    fn dispatch_mode(&self, id: QueueId, mode: QueueMode, cx: &mut Context<Self>) {
        let scheduler = self.scheduler.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                scheduler
                    .set_mode(id, mode)
                    .await
                    .map_err(RepoFailure::from)
            },
            |this, result, cx| {
                if let Err(message) = result {
                    this.on_scheduler_error(
                        tr!("queues_mode_change_failed", error = message.text()),
                        cx,
                    );
                }
            },
            cx,
        );
    }

    fn dispatch_pause_all(&self, ids: Vec<QueueId>, cx: &mut Context<Self>) {
        let scheduler = self.scheduler.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let mut failure: Option<RepoFailure> = None;
                for id in ids {
                    if let Err(err) = scheduler.set_mode(id, QueueMode::PAUSED).await
                        && failure.is_none()
                    {
                        failure = Some(RepoFailure::from(err));
                    }
                }
                match failure {
                    Some(message) => Err(message),
                    None => Ok(()),
                }
            },
            |this, result, cx| {
                if let Err(message) = result {
                    this.on_scheduler_error(
                        tr!("queues_pause_all_failed", error = message.text()),
                        cx,
                    );
                }
            },
            cx,
        );
    }

    fn set_mode(&mut self, id: QueueId, mode: QueueMode, cx: &mut Context<Self>) {
        if let Some(q) = self.queues.iter_mut().find(|q| q.id == id) {
            q.mode = mode;
        }
        self.dispatch_mode(id, mode, cx);
        cx.notify();
    }

    fn toggle_mode(&mut self, id: QueueId, preset: QueueMode, cx: &mut Context<Self>) {
        let current = self.queues.iter().find(|q| q.id == id).map(|q| q.mode);
        let next = if current == Some(preset) {
            QueueMode::RUNNING
        } else {
            preset
        };
        self.set_mode(id, next, cx);
    }

    fn resume(&mut self, id: QueueId, cx: &mut Context<Self>) {
        self.set_mode(id, QueueMode::RUNNING, cx);
    }

    fn request_free(&mut self, id: QueueId, cx: &mut Context<Self>) {
        self.pending_free.request(id);
        cx.notify();
    }

    fn cancel_free(&mut self, cx: &mut Context<Self>) {
        self.pending_free.cancel();
        cx.notify();
    }

    fn confirm_free(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.pending_free.take() {
            self.free(id, cx);
        }
        cx.notify();
    }

    fn free(&mut self, id: QueueId, cx: &mut Context<Self>) {
        let Some(q) = self.queues.iter().find(|q| q.id == id) else {
            return;
        };
        let name = q.name.clone();

        let scheduler = self.scheduler.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move { scheduler.clear(id, true).await.map_err(RepoFailure::from) },
            move |this, result, cx| match result {
                Ok(()) => {
                    cx.push_toast(
                        ToastKind::Info,
                        tr!("queues_free_feedback", name = name.as_str()),
                    );
                    this.reload(cx);
                }
                Err(message) => {
                    this.on_scheduler_error(tr!("queues_free_failed", error = message.text()), cx)
                }
            },
            cx,
        );
    }

    fn pause_all(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<QueueId> = self.queues.iter().map(|q| q.id).collect();
        for q in &mut self.queues {
            q.mode = QueueMode::PAUSED;
        }
        self.dispatch_pause_all(ids, cx);
        cx.notify();
    }

    fn request_delete(&mut self, id: QueueId, cx: &mut Context<Self>) {
        self.menu_open = None;
        if id != DEFAULT_QUEUE_ID {
            self.pending_delete.request(id);
        }
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete.cancel();
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(deleted_id) = self.pending_delete.take() else {
            return;
        };
        cx.notify();

        let queue_repo = Arc::clone(&self.queue_repo);
        let scheduler = self.scheduler.clone();
        async_bridge::run_async(
            &self.rt_handle,
            delete_queue(queue_repo, scheduler, deleted_id),
            |this, result, cx| match result {
                Ok(()) => this.reload(cx),
                Err(message) => {
                    this.on_scheduler_error(tr!("queues_delete_failed", error = message.text()), cx)
                }
            },
            cx,
        );
    }

    fn open_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let modal = cx.new(|cx| EditQueueModal::new(None, "", "", PARALLEL_CONCURRENCY, cx));
        focus_inside_modal(window, cx, |window, cx| {
            modal.update(cx, |m, cx| m.focus(window, cx))
        });
        self._modal_sub = Some(cx.subscribe(&modal, Self::on_modal_event));
        self.modal = Some(modal);
        cx.notify();
    }

    fn open_configure(&mut self, id: QueueId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(q) = self.queues.iter().find(|q| q.id == id) else {
            return;
        };
        let name = q.name.clone();
        let description = q.description.clone();
        let concurrency = q.concurrency;
        let modal =
            cx.new(|cx| EditQueueModal::new(Some(id), &name, &description, concurrency, cx));
        focus_inside_modal(window, cx, |window, cx| {
            modal.update(cx, |m, cx| m.focus(window, cx))
        });
        self._modal_sub = Some(cx.subscribe(&modal, Self::on_modal_event));
        self.modal = Some(modal);
        cx.notify();
    }

    fn on_modal_event(
        &mut self,
        _modal: Entity<EditQueueModal>,
        event: &EditQueueEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            EditQueueEvent::Submit(draft) => self.persist(draft, cx),
            EditQueueEvent::Cancel => self.close_modal(cx),
        }
    }

    fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.modal = None;
        self._modal_sub = None;
        cx.notify();
    }

    fn toggle_menu(&mut self, id: QueueId, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.menu_open == Some(id) {
            self.menu_open = None;
        } else {
            self.menu_open = Some(id);
            self.menu_click_pos = Some(position);
        }
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
    }

    fn persist(&mut self, draft: &QueueDraft, cx: &mut Context<Self>) {
        let editing = draft.editing;
        let queue = Queue {
            id: editing.unwrap_or_default(),
            name: draft.name.clone(),
            description: draft.description.clone(),
            concurrency: NonZeroU32::new(draft.concurrency.clamp(MIN_CONCURRENCY, MAX_CONCURRENCY))
                .unwrap_or(NonZeroU32::MIN),
        };
        let id = queue.id;
        let is_edit = editing.is_some();
        if let Some(modal) = self.modal.as_ref() {
            modal.update(cx, |m, cx| {
                m.saving = true;
                cx.notify();
            });
        }
        cx.notify();

        let queue_repo = Arc::clone(&self.queue_repo);
        let scheduler = self.scheduler.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                queue_repo.save(&queue).await.map_err(RepoFailure::from)?;
                let membership = if is_edit {
                    scheduler.reconfigure(queue).await
                } else {
                    scheduler.register(queue).await
                };
                Ok::<_, RepoFailure>(membership.map_err(RepoFailure::from))
            },
            move |this, result, cx| match result {
                Ok(membership) => {
                    Self::log_membership_outcome(id, &membership);
                    this.close_modal(cx);
                    this.reload(cx);
                }
                Err(message) => this.on_save_error(&message.text(), cx),
            },
            cx,
        );
    }

    fn on_save_error(&mut self, message: &str, cx: &mut Context<Self>) {
        tracing::warn!(error = %message, "queue save failed");
        if let Some(modal) = self.modal.as_ref() {
            modal.update(cx, |m, cx| {
                m.saving = false;
                cx.notify();
            });
        }
        cx.push_toast(ToastKind::Error, tr!("queues_save_failed", error = message));
        cx.notify();
    }
}

impl Render for QueuesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let stats = self.render_stats(&palette, density);
        let subheader_left = self.render_subheader_left(&palette, density, cx);
        let subheader_right = self.render_subheader_right(&palette, density, cx);

        let subtitle = div()
            .font_family(body_family())
            .text_size(STATS_FS)
            .text_color(palette.text_muted)
            .child(tr!("queues_subtitle"));

        let visible_count = self.visible_indices().len();
        let body = if self.queues.is_empty() {
            load_notice(
                &self.list_load,
                LoadNotice {
                    id: "queues-load",
                    loading: tr!("queues_loading"),
                    width: NoticeWidth::Pane,
                },
                cx.listener(|this, _: &ClickEvent, _, cx| this.retry_list_load(cx)),
                &palette,
                density,
            )
            .unwrap_or_else(|| {
                empty_state(tr!("queues_empty"), &palette)
                    .density(density)
                    .into_any_element()
            })
        } else if visible_count == 0 {
            empty_state(tr!("queues_no_filter_match"), &palette)
                .density(density)
                .into_any_element()
        } else {
            self.queue_grid(&palette, density, cx)
        };

        let scroll = div()
            .id("queues-scroll")
            .flex_1()
            .h_full()
            .overflow_y_revealing_focus()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Sm, density))
                    .p(spacing(Spacing::Md, density))
                    .child(subtitle)
                    .child(body)
                    .children(self.scheduled.clone()),
            );

        let body_col = div().flex_1().h_full().flex().flex_col().child(scroll);

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("queues_breadcrumb_automation")),
                BreadcrumbCrumb::leaf(tr!("queues_breadcrumb_queues")),
            ],
            &palette,
        )
        .header_right(stats)
        .subheader_left(subheader_left)
        .subheader_right(subheader_right)
        .density(density)
        .body(body_col);

        let modal_overlay = self.modal.clone();

        let delete_overlay = self.pending_delete.get().copied().and_then(|id| {
            let name = self.queues.iter().find(|q| q.id == id)?.name.clone();
            Some(self.render_delete_confirm(SharedString::from(name), &palette, cx))
        });
        let free_overlay = self.pending_free.get().copied().and_then(|id| {
            let name = self.queues.iter().find(|q| q.id == id)?.name.clone();
            Some(self.render_free_confirm(SharedString::from(name), &palette, cx))
        });

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(modal_overlay)
            .children(delete_overlay)
            .children(free_overlay)
    }
}

#[cfg(test)]
mod tests {
    use super::store::LiveApplyFailure;
    use forge_components::{Density, ThemeId};
    use forge_runtime::{QueueDepth, QueueDepths};
    use forge_storage::Language;
    use forge_storage::MockQueueRepo;
    use gpui::{Modifiers, TestAppContext, VisualTestContext, point, size};

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{
        StubActions, alive_tasks, held_queue_scheduler, pump, pump_until, runtime, settle_until,
        settle_until_tasks_drain_to, worker_runtime,
    };
    use forge_components::Presentation;
    use forge_components::Toasts;

    const FREE_WINDOW_W: f32 = 1000.0;
    const FREE_WINDOW_H: f32 = 700.0;
    const FREE_BUTTONS_W: f32 = 640.0;
    const FREE_BUTTONS_H: f32 = 48.0;
    const FREE_SCAN_STEP: f32 = 8.0;

    fn queue(id: QueueId) -> Queue {
        Queue {
            id,
            name: "Default".to_owned(),
            description: String::new(),
            concurrency: std::num::NonZeroU32::MIN,
        }
    }

    fn busy(id: QueueId) -> QueueDepths {
        QueueDepths::from([(
            id,
            QueueDepth {
                pending: 4,
                in_flight: 1,
                overflowed: 2,
            },
        )])
    }

    fn mount(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        id: QueueId,
        health: Entity<QueueHealth>,
    ) -> Entity<QueuesView> {
        install_language(Language::En);
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            cx.set_global(Toasts::new());
        });
        let scheduler = held_queue_scheduler(rt, queue(id));
        let mut repo = MockQueueRepo::new();
        repo.expect_list().returning(move || Ok(vec![queue(id)]));
        let view = cx.update(|cx| {
            cx.new(|cx| {
                QueuesView::new(
                    health,
                    scheduler,
                    Arc::new(repo),
                    Arc::new(StubActions),
                    rt.handle().clone(),
                    cx,
                )
            })
        });
        pump(rt);
        cx.run_until_parked();
        view
    }

    struct Shown {
        view: Entity<QueuesView>,
    }

    impl Render for Shown {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(self.view.clone())
        }
    }

    fn on_screen<'a>(
        cx: &'a mut TestAppContext,
        view: &Entity<QueuesView>,
    ) -> &'a mut VisualTestContext {
        cx.update(forge_components::bind_confirm_keys);
        let view = view.clone();
        let (_, vcx) = cx.add_window_view(move |_, _| Shown { view });
        vcx.simulate_resize(size(px(FREE_WINDOW_W), px(FREE_WINDOW_H)));
        vcx.run_until_parked();
        vcx
    }

    fn staged_free(view: &Entity<QueuesView>, cx: &mut TestAppContext) -> Option<QueueId> {
        view.read_with(cx, |view, _| view.pending_free.get().copied())
    }

    fn free_feedback() -> (ToastKind, String) {
        (
            ToastKind::Info,
            tr!("queues_free_feedback", name = "Default"),
        )
    }

    fn mounted(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        id: QueueId,
    ) -> Entity<QueuesView> {
        let health = cx.new(|_| QueueHealth::new());
        mount(cx, rt, id, health)
    }

    struct CardButtons {
        view: Entity<QueuesView>,
    }

    impl Render for CardButtons {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let palette = cx.palette();
            let density = cx.density();
            let buttons = self.view.update(cx, |view, cx| {
                let view = &*view;
                view.queues
                    .first()
                    .map(|row| view.card_buttons(0, row, &palette, density, cx))
            });
            div().w(px(FREE_BUTTONS_W)).children(buttons)
        }
    }

    #[gpui::test]
    fn the_free_button_only_asks_and_drops_nothing_until_confirmed(cx: &mut TestAppContext) {
        let rt = runtime();
        let id = QueueId::new();
        let view = mounted(cx, &rt, id);
        let mut reached = None;
        {
            let shown = view.clone();
            let (_, vcx) = cx.add_window_view(move |_, _| CardButtons { view: shown });
            vcx.run_until_parked();
            let mut y = FREE_SCAN_STEP / 2.0;
            'scan: while y < FREE_BUTTONS_H {
                let mut x = FREE_SCAN_STEP / 2.0;
                while x < FREE_BUTTONS_W {
                    vcx.simulate_click(point(px(x), px(y)), Modifiers::none());
                    reached = view.read_with(vcx, |view, _| view.pending_free.get().copied());
                    if reached.is_some() {
                        break 'scan;
                    }
                    x += FREE_SCAN_STEP;
                }
                y += FREE_SCAN_STEP;
            }
        }
        settle(cx, &rt);

        assert_eq!(reached, Some(id));
        assert!(!toasts(cx).contains(&free_feedback()));
    }

    #[gpui::test]
    fn escape_on_the_free_prompt_cancels_without_dropping(cx: &mut TestAppContext) {
        let rt = runtime();
        let id = QueueId::new();
        let view = mounted(cx, &rt, id);
        view.update(cx, |view, cx| view.request_free(id, cx));
        {
            let vcx = on_screen(cx, &view);
            vcx.simulate_keystrokes("escape");
            vcx.run_until_parked();
        }
        settle(cx, &rt);

        assert_eq!((staged_free(&view, cx), toasts(cx)), (None, Vec::new()));
    }

    #[gpui::test]
    fn enter_on_the_free_prompt_frees_the_queue_it_named(cx: &mut TestAppContext) {
        let rt = runtime();
        let id = QueueId::new();
        let view = mounted(cx, &rt, id);
        view.update(cx, |view, cx| view.request_free(id, cx));
        {
            let vcx = on_screen(cx, &view);
            vcx.simulate_keystrokes("enter");
            vcx.run_until_parked();
        }
        settle(cx, &rt);

        assert_eq!(
            (staged_free(&view, cx), toasts(cx)),
            (None, vec![free_feedback()])
        );
    }

    #[gpui::test]
    fn a_cancelled_free_cannot_be_confirmed_afterwards(cx: &mut TestAppContext) {
        let rt = runtime();
        let id = QueueId::new();
        let view = mounted(cx, &rt, id);

        view.update(cx, |view, cx| {
            view.request_free(id, cx);
            view.cancel_free(cx);
            view.confirm_free(cx);
        });
        settle(cx, &rt);

        assert_eq!(toasts(cx), Vec::new());
    }

    #[gpui::test]
    fn confirming_twice_frees_once(cx: &mut TestAppContext) {
        let rt = runtime();
        let id = QueueId::new();
        let view = mounted(cx, &rt, id);

        view.update(cx, |view, cx| {
            view.request_free(id, cx);
            view.confirm_free(cx);
            view.confirm_free(cx);
        });
        settle(cx, &rt);

        assert_eq!(toasts(cx), vec![free_feedback()]);
    }

    #[gpui::test]
    fn confirming_a_free_of_a_queue_that_is_gone_does_nothing(cx: &mut TestAppContext) {
        let rt = runtime();
        let view = mounted(cx, &rt, QueueId::new());

        view.update(cx, |view, cx| {
            view.request_free(QueueId::new(), cx);
            view.confirm_free(cx);
        });
        settle(cx, &rt);

        assert_eq!((staged_free(&view, cx), toasts(cx)), (None, Vec::new()));
    }

    #[test]
    fn only_a_frozen_queue_reads_as_paused_in_the_filters() {
        for (mode, paused) in [
            (QueueMode::RUNNING, false),
            (QueueMode::DRAINING, false),
            (QueueMode::HOLDING, true),
            (QueueMode::PAUSED, true),
        ] {
            let row = QueueRow {
                id: QueueId::new(),
                name: String::new(),
                description: String::new(),
                blocking: true,
                concurrency: 1,
                mode,
                live: true,
                pending: 0,
                in_flight: 0,
                overflowed: 0,
                actions: 0,
                running: Vec::new(),
            };

            assert_eq!(
                (
                    QueueFilter::Paused.keeps(&row),
                    QueueFilter::Running.keeps(&row)
                ),
                (paused, !paused),
                "{mode:?}"
            );
        }
    }

    fn row_depth(view: &Entity<QueuesView>, cx: &mut TestAppContext) -> Option<(u32, u32, u64)> {
        view.read_with(cx, |view, _| {
            view.queues
                .first()
                .map(|row| (row.pending, row.in_flight, row.overflowed))
        })
    }

    #[gpui::test]
    fn a_depth_change_in_queue_health_repaints_the_row_without_a_reload(cx: &mut TestAppContext) {
        let rt = runtime();
        let id = QueueId::new();
        let health = cx.new(|_| QueueHealth::new());
        let view = mount(cx, &rt, id, health.clone());

        health.update(cx, |health, cx| {
            health.apply_depths(busy(id));
            cx.notify();
        });
        cx.run_until_parked();

        assert_eq!(row_depth(&view, cx), Some((4, 1, 2)));
    }

    #[gpui::test]
    fn rows_loaded_after_the_depths_arrived_show_them(cx: &mut TestAppContext) {
        let rt = runtime();
        let id = QueueId::new();
        let health = cx.new(|_| {
            let mut health = QueueHealth::new();
            health.apply_depths(busy(id));
            health
        });

        let view = mount(cx, &rt, id, health);

        assert_eq!(row_depth(&view, cx), Some((4, 1, 2)));
    }

    fn mount_stored(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        scheduler: QueueSchedulerHandle,
        stored: Vec<Queue>,
    ) -> Entity<QueuesView> {
        install_language(Language::En);
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            cx.set_global(Toasts::new());
        });
        let mut repo = MockQueueRepo::new();
        repo.expect_list().returning(move || Ok(stored.clone()));
        mount_over(cx, rt, scheduler, repo)
    }

    fn mount_over(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        scheduler: QueueSchedulerHandle,
        repo: MockQueueRepo,
    ) -> Entity<QueuesView> {
        let health = cx.new(|_| QueueHealth::new());
        let view = cx.update(|cx| {
            cx.new(|cx| {
                QueuesView::new(
                    health,
                    scheduler,
                    Arc::new(repo),
                    Arc::new(StubActions),
                    rt.handle().clone(),
                    cx,
                )
            })
        });
        settle(cx, rt);
        view
    }

    fn settle(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
        for _ in 0..4 {
            pump(rt);
            cx.run_until_parked();
        }
    }

    fn diverged(view: &Entity<QueuesView>, cx: &mut TestAppContext) -> Vec<QueueId> {
        view.read_with(cx, |view, _| view.diverged.iter().copied().collect())
    }

    fn toasts(cx: &mut TestAppContext) -> Vec<(ToastKind, String)> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|t| (t.kind, t.message.to_string()))
                .collect()
        })
    }

    #[gpui::test]
    fn a_stored_queue_the_scheduler_does_not_run_is_flagged_not_live(cx: &mut TestAppContext) {
        let rt = runtime();
        let (live, orphan) = (QueueId::new(), QueueId::new());
        let scheduler = held_queue_scheduler(&rt, queue(live));

        let view = mount_stored(cx, &rt, scheduler, vec![queue(live), queue(orphan)]);

        assert_eq!(diverged(&view, cx), vec![orphan]);
    }

    #[gpui::test]
    fn applying_a_not_live_queue_registers_it_clears_the_flag_and_toasts_success(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (live, orphan) = (QueueId::new(), QueueId::new());
        let scheduler = held_queue_scheduler(&rt, queue(live));
        let view = mount_stored(cx, &rt, scheduler, vec![queue(live), queue(orphan)]);

        view.update(cx, |view, cx| view.apply_live(orphan, cx));
        settle(cx, &rt);

        assert_eq!(
            (diverged(&view, cx), toasts(cx)),
            (
                Vec::new(),
                vec![(ToastKind::Success, tr!("queues_apply_live_done"))]
            )
        );
    }

    #[gpui::test]
    fn applying_a_queue_deleted_meanwhile_toasts_the_failure_and_stays_flagged(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (live, orphan) = (QueueId::new(), QueueId::new());
        let scheduler = held_queue_scheduler(&rt, queue(live));
        let view = mount_stored(cx, &rt, scheduler, vec![queue(live), queue(orphan)]);

        view.update(cx, |view, cx| view.apply_live(QueueId::new(), cx));
        settle(cx, &rt);

        assert_eq!(
            (diverged(&view, cx), toasts(cx)),
            (
                vec![orphan],
                vec![(
                    ToastKind::Error,
                    tr!(
                        "queues_apply_live_failed",
                        error = tr!("queues_apply_live_missing")
                    )
                )]
            )
        );
    }

    fn named(id: QueueId, name: &str) -> Queue {
        Queue {
            name: name.to_owned(),
            ..queue(id)
        }
    }

    #[gpui::test]
    fn only_a_queue_other_than_the_default_can_be_staged_for_delete_whatever_its_name(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let impostor = QueueId::new();
        let scheduler = held_queue_scheduler(&rt, named(DEFAULT_QUEUE_ID, "Main"));
        let view = mount_stored(
            cx,
            &rt,
            scheduler,
            vec![named(DEFAULT_QUEUE_ID, "Main"), named(impostor, "Default")],
        );

        for (asked, staged) in [(DEFAULT_QUEUE_ID, None), (impostor, Some(impostor))] {
            let pending = view.update(cx, |view, cx| {
                view.request_delete(asked, cx);
                let pending = view.pending_delete.get().copied();
                view.cancel_delete(cx);
                pending
            });

            assert_eq!(pending, staged, "{asked}");
        }
    }

    const STORE_FAILURE: &str = "disk gone";

    fn live_mode(
        rt: &tokio::runtime::Runtime,
        scheduler: &QueueSchedulerHandle,
        id: QueueId,
    ) -> Option<QueueMode> {
        rt.block_on(scheduler.queue_states())
            .unwrap()
            .get(&id)
            .map(|state| state.mode)
    }

    fn row_mode(
        view: &Entity<QueuesView>,
        id: QueueId,
        cx: &mut TestAppContext,
    ) -> Option<QueueMode> {
        view.read_with(cx, |view, _| {
            view.queues.iter().find(|q| q.id == id).map(|q| q.mode)
        })
    }

    fn acting_on(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        view: &Entity<QueuesView>,
        act: impl FnOnce(&mut QueuesView, &mut Context<QueuesView>),
    ) {
        let background_tasks = alive_tasks(rt);
        view.update(cx, act);
        settle_until_tasks_drain_to(cx, rt, background_tasks);
    }

    #[gpui::test]
    fn toggling_a_mode_applies_it_and_toggling_the_active_mode_returns_to_running(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let id = QueueId::new();
        let scheduler = held_queue_scheduler(&rt, named(id, "Alerts"));
        let view = mount_stored(cx, &rt, scheduler.clone(), vec![named(id, "Alerts")]);

        for (toggled, expected) in [
            (QueueMode::HOLDING, QueueMode::RUNNING),
            (QueueMode::PAUSED, QueueMode::PAUSED),
            (QueueMode::DRAINING, QueueMode::DRAINING),
            (QueueMode::DRAINING, QueueMode::RUNNING),
        ] {
            acting_on(cx, &rt, &view, |view, cx| {
                view.toggle_mode(id, toggled, cx);
            });

            assert_eq!(
                (row_mode(&view, id, cx), live_mode(&rt, &scheduler, id)),
                (Some(expected), Some(expected)),
                "{toggled:?}"
            );
        }
    }

    #[gpui::test]
    fn pause_all_reports_the_first_failure_once_and_still_pauses_the_queues_after_it(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (first_lost, second_lost, live) = (QueueId::new(), QueueId::new(), QueueId::new());
        let scheduler = held_queue_scheduler(&rt, named(live, "Alerts"));
        let view = mount_stored(
            cx,
            &rt,
            scheduler.clone(),
            vec![
                named(first_lost, "Lost one"),
                named(second_lost, "Lost two"),
                named(live, "Alerts"),
            ],
        );

        acting_on(cx, &rt, &view, |view, cx| view.pause_all(cx));
        settle(cx, &rt);

        let shown = toasts(cx);
        assert_eq!(live_mode(&rt, &scheduler, live), Some(QueueMode::PAUSED));
        assert!(
            matches!(shown.as_slice(), [(ToastKind::Error, _)]),
            "{shown:?}"
        );
    }

    #[gpui::test]
    fn freeing_a_live_queue_confirms_with_its_name(cx: &mut TestAppContext) {
        let rt = runtime();
        let id = QueueId::new();
        let scheduler = held_queue_scheduler(&rt, named(id, "Alerts"));
        let view = mount_stored(cx, &rt, scheduler, vec![named(id, "Alerts")]);

        acting_on(cx, &rt, &view, |view, cx| view.free(id, cx));

        assert_eq!(
            toasts(cx),
            vec![(
                ToastKind::Info,
                tr!("queues_free_feedback", name = "Alerts")
            )]
        );
    }

    #[gpui::test]
    fn freeing_a_queue_the_scheduler_lost_reports_the_failure(cx: &mut TestAppContext) {
        let rt = runtime();
        let (live, lost) = (QueueId::new(), QueueId::new());
        let scheduler = held_queue_scheduler(&rt, named(live, "Alerts"));
        let view = mount_stored(cx, &rt, scheduler, vec![named(lost, "Lost")]);

        acting_on(cx, &rt, &view, |view, cx| view.free(lost, cx));
        settle(cx, &rt);

        let shown = toasts(cx);
        assert!(
            matches!(shown.as_slice(), [(ToastKind::Error, _)]),
            "{shown:?}"
        );
    }

    #[gpui::test]
    fn freeing_a_queue_not_on_the_screen_does_nothing(cx: &mut TestAppContext) {
        let rt = runtime();
        let id = QueueId::new();
        let scheduler = held_queue_scheduler(&rt, named(id, "Alerts"));
        let view = mount_stored(cx, &rt, scheduler, vec![named(id, "Alerts")]);

        acting_on(cx, &rt, &view, |view, cx| view.free(QueueId::new(), cx));

        assert_eq!(toasts(cx), Vec::new());
    }

    fn saving_repo(
        stored: Vec<Queue>,
        outcome: Result<(), &'static str>,
    ) -> (MockQueueRepo, Arc<std::sync::Mutex<Vec<Queue>>>) {
        let saved = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = Arc::clone(&saved);
        let mut repo = MockQueueRepo::new();
        repo.expect_list().returning(move || Ok(stored.clone()));
        repo.expect_save().returning(move |queue| {
            outcome.map_err(|reason| forge_storage::StorageError::Connection {
                reason: reason.to_owned(),
            })?;
            recorded.lock().unwrap().push(queue.clone());
            Ok(())
        });
        (repo, saved)
    }

    fn with_open_modal(view: &Entity<QueuesView>, cx: &mut TestAppContext) {
        view.update(cx, |view, cx| {
            view.modal = Some(cx.new(|cx| EditQueueModal::new(None, "", "", 1, cx)));
        });
    }

    fn draft(editing: Option<QueueId>, concurrency: u32) -> QueueDraft {
        QueueDraft {
            editing,
            name: "Saved".to_owned(),
            description: String::new(),
            concurrency,
        }
    }

    fn modal_state(view: &Entity<QueuesView>, cx: &mut TestAppContext) -> Option<bool> {
        view.read_with(cx, |view, cx| {
            view.modal.as_ref().map(|modal| modal.read(cx).saving)
        })
    }

    #[gpui::test]
    fn saving_a_new_queue_registers_it_live_and_closes_the_modal(cx: &mut TestAppContext) {
        let rt = runtime();
        let live = QueueId::new();
        let scheduler = held_queue_scheduler(&rt, named(live, "Alerts"));
        let (repo, saved) = saving_repo(vec![named(live, "Alerts")], Ok(()));
        let view = mount_over_in_english(cx, &rt, scheduler.clone(), repo);
        with_open_modal(&view, cx);

        view.update(cx, |view, cx| {
            view.persist(&draft(None, PARALLEL_CONCURRENCY), cx);
        });
        settle_until(cx, &rt, |cx| modal_state(&view, cx).is_none());

        let new_id = saved.lock().unwrap()[0].id;
        assert_ne!(new_id, live);
        assert_eq!(live_mode(&rt, &scheduler, new_id), Some(QueueMode::RUNNING));
    }

    #[gpui::test]
    fn saving_an_edit_keeps_the_queue_id_and_clamps_the_concurrency_to_the_ceiling(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let id = QueueId::new();
        let scheduler = held_queue_scheduler(&rt, named(id, "Alerts"));
        let (repo, saved) = saving_repo(vec![named(id, "Alerts")], Ok(()));
        let view = mount_over_in_english(cx, &rt, scheduler.clone(), repo);

        for (asked, kept) in [
            (MAX_CONCURRENCY, MAX_CONCURRENCY),
            (MAX_CONCURRENCY + 1, MAX_CONCURRENCY),
            (MIN_CONCURRENCY - 1, MIN_CONCURRENCY),
        ] {
            acting_on(cx, &rt, &view, |view, cx| {
                view.persist(&draft(Some(id), asked), cx);
            });

            let last = saved.lock().unwrap().last().cloned().unwrap();
            assert_eq!((last.id, last.concurrency.get()), (id, kept), "{asked}");
        }
        assert_eq!(rt.block_on(scheduler.queue_states()).unwrap().len(), 1);
    }

    #[gpui::test]
    fn a_failed_save_leaves_the_modal_open_and_usable_and_registers_nothing(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let live = QueueId::new();
        let scheduler = held_queue_scheduler(&rt, named(live, "Alerts"));
        let (repo, _saved) = saving_repo(vec![named(live, "Alerts")], Err(STORE_FAILURE));
        let view = mount_over_in_english(cx, &rt, scheduler.clone(), repo);
        with_open_modal(&view, cx);

        acting_on(cx, &rt, &view, |view, cx| {
            view.persist(&draft(None, PARALLEL_CONCURRENCY), cx);
        });

        let shown = toasts(cx);
        assert_eq!(
            (
                modal_state(&view, cx),
                rt.block_on(scheduler.queue_states()).unwrap().len()
            ),
            (Some(false), 1)
        );
        assert!(
            matches!(shown.as_slice(), [(ToastKind::Error, _)]),
            "{shown:?}"
        );
    }

    fn mount_over_in_english(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        scheduler: QueueSchedulerHandle,
        repo: MockQueueRepo,
    ) -> Entity<QueuesView> {
        install_language(Language::En);
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            cx.set_global(Toasts::new());
        });
        mount_over(cx, rt, scheduler, repo)
    }

    #[test]
    fn a_delete_the_store_refused_leaves_the_queue_running() {
        let rt = runtime();
        let id = QueueId::new();
        let scheduler = held_queue_scheduler(&rt, named(id, "Alerts"));
        let mut repo = MockQueueRepo::new();
        repo.expect_delete_reassigning_actions().returning(|_| {
            Err(forge_storage::StorageError::Connection {
                reason: STORE_FAILURE.to_owned(),
            })
        });

        let deleted = rt.block_on(delete_queue(Arc::new(repo), scheduler.clone(), id));

        assert_eq!(
            (deleted.is_err(), live_mode(&rt, &scheduler, id)),
            (true, Some(QueueMode::HOLDING))
        );
    }

    #[test]
    fn a_delete_of_a_queue_already_gone_from_the_store_leaves_the_scheduler_alone() {
        let rt = runtime();
        let id = QueueId::new();
        let scheduler = held_queue_scheduler(&rt, named(id, "Alerts"));
        let mut repo = MockQueueRepo::new();
        repo.expect_delete_reassigning_actions()
            .returning(|_| Ok(None));

        let deleted = rt.block_on(delete_queue(Arc::new(repo), scheduler.clone(), id));

        assert_eq!(
            (deleted.is_ok(), live_mode(&rt, &scheduler, id)),
            (true, Some(QueueMode::HOLDING))
        );
    }

    #[test]
    fn deleting_a_queue_stops_it_live_and_leaves_actions_of_other_queues_alone() {
        let rt = runtime();
        let doomed = named(QueueId::new(), "Doomed");
        let kept = named(QueueId::new(), "Kept");
        let scheduler = held_queue_scheduler(&rt, doomed.clone());
        let bystander_queue = rt.block_on(async {
            let provider = crate::test_support::sandboxed_provider().await;
            let queues = provider.queue_repo();
            queues.save(&named(DEFAULT_QUEUE_ID, "Main")).await.unwrap();
            queues.save(&doomed).await.unwrap();
            queues.save(&kept).await.unwrap();
            let bystander = forge_types::Action {
                queue_id: kept.id,
                ..crate::test_support::action_running("Bystander", Vec::new())
            };
            provider.action_repo().save(&bystander).await.unwrap();

            delete_queue(queues, scheduler.clone(), doomed.id)
                .await
                .unwrap();

            provider
                .action_repo()
                .get(bystander.id)
                .await
                .unwrap()
                .map(|a| a.queue_id)
        });

        assert_eq!(
            (bystander_queue, live_mode(&rt, &scheduler, doomed.id)),
            (Some(kept.id), None)
        );
    }

    #[test]
    fn deleting_a_queue_named_default_moves_its_actions_to_the_real_default_queue() {
        let rt = runtime();
        let impostor = named(QueueId::new(), "Default");
        let scheduler = held_queue_scheduler(&rt, impostor.clone());
        let (moved_to, remaining) = rt.block_on(async {
            let provider = crate::test_support::sandboxed_provider().await;
            let queues = provider.queue_repo();
            queues.save(&named(DEFAULT_QUEUE_ID, "Main")).await.unwrap();
            queues.save(&impostor).await.unwrap();
            let action = forge_types::Action {
                queue_id: impostor.id,
                ..crate::test_support::action_running("Greeter", Vec::new())
            };
            provider.action_repo().save(&action).await.unwrap();

            delete_queue(queues.clone(), scheduler, impostor.id)
                .await
                .unwrap();

            let moved_to = provider
                .action_repo()
                .get(action.id)
                .await
                .unwrap()
                .map(|a| a.queue_id);
            let mut remaining: Vec<_> = queues
                .list()
                .await
                .unwrap()
                .into_iter()
                .map(|q| (q.id, q.name))
                .collect();
            remaining.sort();
            (moved_to, remaining)
        });

        assert_eq!(
            (moved_to, remaining),
            (
                Some(DEFAULT_QUEUE_ID),
                vec![(DEFAULT_QUEUE_ID, "Main".to_owned())]
            )
        );
    }

    #[test]
    fn apply_queue_live_reconfigures_a_queue_the_scheduler_already_runs() {
        let rt = runtime();
        let id = QueueId::new();
        let scheduler = held_queue_scheduler(&rt, queue(id));
        let mut changed = queue(id);
        changed.concurrency = NonZeroU32::new(3).unwrap();
        let mut repo = MockQueueRepo::new();
        repo.expect_list()
            .returning(move || Ok(vec![changed.clone()]));

        let outcome = rt.block_on(apply_queue_live(Arc::new(repo), scheduler.clone(), id));
        let states = rt.block_on(scheduler.queue_states());

        assert_eq!(outcome, Ok(()));
        assert!(states.is_ok_and(|s| s.contains_key(&id)));
    }

    #[test]
    fn apply_queue_live_reports_a_store_failure_without_its_diagnostic() {
        let shown =
            crate::test_support::shown_in_english(forge_storage::StorageError::Connection {
                reason: "disk gone".to_owned(),
            });
        let rt = runtime();
        let id = QueueId::new();
        let scheduler = held_queue_scheduler(&rt, queue(id));
        let mut repo = MockQueueRepo::new();
        repo.expect_list().returning(|| {
            Err(forge_storage::StorageError::Connection {
                reason: "disk gone".to_owned(),
            })
        });

        let outcome = rt.block_on(apply_queue_live(Arc::new(repo), scheduler, id));

        assert_eq!(outcome.map_err(LiveApplyFailure::message), Err(shown));
    }

    #[test]
    fn apply_queue_live_on_a_worker_thread_reports_a_vanished_queue_untranslated_for_the_ui_thread()
    {
        let rt = worker_runtime();
        let id = QueueId::new();
        let scheduler = held_queue_scheduler(&rt, queue(id));
        let mut repo = MockQueueRepo::new();
        repo.expect_list().returning(|| Ok(Vec::new()));

        let joined = rt.block_on(rt.spawn(apply_queue_live(Arc::new(repo), scheduler, id)));
        install_language(Language::Uk);

        assert_eq!(
            joined
                .ok()
                .map(|outcome| outcome.map_err(LiveApplyFailure::message)),
            Some(Err("черги більше не існує".to_owned()))
        );
    }

    #[test]
    fn apply_queue_live_on_a_worker_thread_leaves_a_store_failure_for_the_ui_language() {
        let shown =
            crate::test_support::shown_in_english(forge_storage::StorageError::Connection {
                reason: "disk gone".to_owned(),
            });
        let rt = worker_runtime();
        let id = QueueId::new();
        let scheduler = held_queue_scheduler(&rt, queue(id));
        let mut repo = MockQueueRepo::new();
        repo.expect_list().returning(|| {
            Err(forge_storage::StorageError::Connection {
                reason: "disk gone".to_owned(),
            })
        });

        let joined = rt.block_on(rt.spawn(apply_queue_live(Arc::new(repo), scheduler, id)));

        assert_eq!(
            joined
                .ok()
                .map(|outcome| outcome.map_err(LiveApplyFailure::message)),
            Some(Err(shown))
        );
    }

    #[test]
    fn live_apply_failures_resolve_their_text_in_the_installed_language() {
        install_language(Language::Uk);
        for (failure, expected) in [
            (LiveApplyFailure::QueueMissing, "черги більше не існує"),
            (
                LiveApplyFailure::SchedulerLostQueue,
                "планувальник не знає цієї черги",
            ),
            (
                LiveApplyFailure::Failed(crate::repo_failure::RepoFailure::localized("disk gone")),
                "disk gone",
            ),
        ] {
            assert_eq!(failure.message(), expected);
        }
    }

    fn run_of(action_id: ActionId) -> RunningAction {
        RunningAction {
            action_id,
            started_at: Instant::now(),
        }
    }

    fn stored_action(name: &str, queue_id: QueueId) -> forge_types::Action {
        forge_types::Action {
            id: ActionId::new(),
            name: name.to_owned(),
            group: None,
            queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::default(),
            description: None,
            sub_actions: vec![],
        }
    }

    fn set_running(
        health: &Entity<QueueHealth>,
        id: QueueId,
        runs: Vec<RunningAction>,
        cx: &mut TestAppContext,
    ) {
        health.update(cx, |health, cx| {
            health.apply_running(forge_runtime::QueueRunning::from([(id, runs)]));
            cx.notify();
        });
    }

    struct CountedLoads {
        view: Entity<QueuesView>,
        loads: Arc<std::sync::atomic::AtomicUsize>,
        actions: Arc<std::sync::Mutex<Vec<forge_types::Action>>>,
    }

    impl CountedLoads {
        fn count(&self) -> usize {
            self.loads.load(std::sync::atomic::Ordering::SeqCst)
        }

        fn settle_at(
            &self,
            expected: usize,
            rt: &tokio::runtime::Runtime,
            cx: &mut TestAppContext,
        ) {
            pump_until(rt, || {
                cx.run_until_parked();
                self.count() >= expected
            });
            pump(rt);
            cx.run_until_parked();
        }

        fn running_names(&self, cx: &mut TestAppContext) -> Vec<String> {
            self.view.read_with(cx, |view, _| {
                view.queues
                    .first()
                    .map(|row| row.running.iter().map(|run| run.name.clone()).collect())
                    .unwrap_or_default()
            })
        }
    }

    fn mount_counted(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        id: QueueId,
        health: Entity<QueueHealth>,
    ) -> CountedLoads {
        install_language(Language::En);
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            cx.set_global(Toasts::new());
        });
        let scheduler = held_queue_scheduler(rt, queue(id));
        let loads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let actions = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut queue_repo = MockQueueRepo::new();
        let counted = Arc::clone(&loads);
        queue_repo.expect_list().returning(move || {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(vec![queue(id)])
        });
        let mut action_repo = forge_storage::MockActionRepo::new();
        let stored = Arc::clone(&actions);
        action_repo
            .expect_list()
            .returning(move || Ok(stored.lock().unwrap().clone()));
        let view = cx.update(|cx| {
            cx.new(|cx| {
                QueuesView::new(
                    health,
                    scheduler,
                    Arc::new(queue_repo),
                    Arc::new(action_repo),
                    rt.handle().clone(),
                    cx,
                )
            })
        });
        let counted = CountedLoads {
            view,
            loads,
            actions,
        };
        counted.settle_at(1, rt, cx);
        counted
    }

    #[test]
    fn a_run_is_named_after_its_action_or_a_placeholder_when_the_action_is_unknown() {
        let (known, unknown) = (ActionId::new(), ActionId::new());
        let names = HashMap::from([(known, "Intro".to_owned())]);
        let runs = [run_of(known), run_of(unknown)];

        let entries = running_entries(&runs, &names);

        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.name.as_str(), entry.started_at))
                .collect::<Vec<_>>(),
            vec![
                ("Intro", runs[0].started_at),
                (UNNAMED_ACTION, runs[1].started_at),
            ]
        );
    }

    #[test]
    fn elapsed_time_reads_in_milliseconds_below_a_second_and_as_uptime_from_one() {
        install_language(Language::En);

        for (elapsed, expected) in [
            (Duration::ZERO, "0ms"),
            (Duration::from_millis(999), "999ms"),
            (Duration::from_millis(1_000), "1s"),
            (Duration::from_secs(61), "1m 1s"),
        ] {
            let shown = elapsed_text(elapsed).replace(crate::test_support::FORMAT_ISOLATES, "");

            assert_eq!(shown, expected, "{elapsed:?}");
        }
    }

    #[gpui::test]
    fn a_run_of_an_action_unknown_to_the_screen_reloads_and_gets_its_name(cx: &mut TestAppContext) {
        let rt = runtime();
        let id = QueueId::new();
        let health = cx.new(|_| QueueHealth::new());
        let screen = mount_counted(cx, &rt, id, health.clone());
        let created = stored_action("Intro", id);
        let created_id = created.id;
        screen.actions.lock().unwrap().push(created);

        set_running(&health, id, vec![run_of(created_id)], cx);
        screen.settle_at(2, &rt, cx);

        assert_eq!(screen.running_names(cx), vec!["Intro".to_owned()]);
    }

    #[gpui::test]
    fn an_action_that_stays_unknown_reloads_once_and_a_new_unknown_one_reloads_again(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let id = QueueId::new();
        let health = cx.new(|_| QueueHealth::new());
        let screen = mount_counted(cx, &rt, id, health.clone());
        let (first, second) = (ActionId::new(), ActionId::new());

        set_running(&health, id, vec![run_of(first)], cx);
        screen.settle_at(2, &rt, cx);
        set_running(&health, id, vec![run_of(first)], cx);
        pump(&rt);
        cx.run_until_parked();
        assert_eq!(screen.count(), 2, "the same deleted action reloaded again");

        set_running(&health, id, vec![run_of(first), run_of(second)], cx);
        screen.settle_at(3, &rt, cx);

        assert_eq!(screen.count(), 3);
        assert_eq!(
            screen.running_names(cx),
            vec![UNNAMED_ACTION.to_owned(), UNNAMED_ACTION.to_owned()]
        );
    }
}
