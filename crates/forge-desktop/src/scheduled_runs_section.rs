use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, Spacing,
    ToastKind, body_family, card, ghost_button_with_icon, hub_section_header, icon, mono_family,
    radius, section_label, spacing, tr, with_alpha,
};
use forge_runtime::{HandOff, ScheduledRunsHandle};
use forge_storage::{ActionRepo, CatalogChanges, ScheduledRun, ScheduledRunId, ScheduledRunRepo};
use forge_types::ActionId;
use gpui::{
    AnyElement, ClickEvent, Context, EventEmitter, Pixels, Rgba, SharedString, Task, Window, div,
    prelude::*, px,
};
use time::{OffsetDateTime, UtcOffset};

use crate::async_bridge;
use crate::presentation::ActivePresentation;
use crate::queues::running_pill;
use crate::scheduled_run_labels::{
    Countdown, OutcomeTone, Span, countdown, countdown_label, local_stamp, outcome_badge_key,
    outcome_tone, reason_label, system_offset_at,
};
use crate::screen::Screen;
use crate::sidebar::NavRequested;
use crate::toasts::PushToast;
use crate::window_presence::PresenceGate;

const RECENT_LIMIT: usize = 20;
const COUNTDOWN_TICK: Duration = Duration::from_secs(30);
const CARD_PAD: Pixels = px(14.0);
const ROW_GLYPH: Pixels = px(12.0);
const BADGE_GLYPH: Pixels = px(9.0);
const BADGE_RADIUS: Pixels = px(8.0);
const ROW_GAP: Pixels = px(8.0);
const LINE_GAP: Pixels = px(3.0);
const DUE_COL_W: Pixels = px(150.0);
const BADGE_FILL_ALPHA: f32 = 0.12;
const BADGE_BORDER_ALPHA: f32 = 0.30;

struct ScheduledRow {
    run: ScheduledRun,
    offset: UtcOffset,
    scheduled_by_name: Option<String>,
}

impl ScheduledRow {
    fn late_by(&self) -> Option<Span> {
        let resolved = self.run.resolved_at?;
        Some(Span::of_seconds(
            (resolved - self.run.spec.due_at).whole_seconds(),
        ))
    }
}

struct ScheduledSnapshot {
    pending: Vec<ScheduledRow>,
    recent: Vec<ScheduledRow>,
}

enum Scheduled {
    Loading,
    Ready(ScheduledSnapshot),
}

pub struct ScheduledRunsView {
    repo: Arc<dyn ScheduledRunRepo>,
    action_repo: Arc<dyn ActionRepo>,
    runs: ScheduledRunsHandle,
    rt_handle: tokio::runtime::Handle,
    state: Scheduled,
    recent_open: bool,
    busy: HashSet<ScheduledRunId>,
    now: OffsetDateTime,
    _revision_watch: Task<()>,
    _countdown_tick: Task<()>,
}

impl EventEmitter<NavRequested> for ScheduledRunsView {}

impl ScheduledRunsView {
    pub fn new(
        repo: Arc<dyn ScheduledRunRepo>,
        changes: CatalogChanges,
        action_repo: Arc<dyn ActionRepo>,
        runs: ScheduledRunsHandle,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let view = Self {
            repo,
            action_repo,
            runs,
            rt_handle,
            state: Scheduled::Loading,
            recent_open: false,
            busy: HashSet::new(),
            now: OffsetDateTime::now_utc(),
            _revision_watch: Self::watch_revision(changes, cx),
            _countdown_tick: Self::spawn_countdown_tick(cx),
        };
        view.reload(cx);
        view
    }

    fn watch_revision(mut changes: CatalogChanges, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            while changes.changed().await.is_some() {
                if this.update(cx, |this, cx| this.reload(cx)).is_err() {
                    break;
                }
            }
        })
    }

    fn spawn_countdown_tick(cx: &mut Context<Self>) -> Task<()> {
        let mut presence = PresenceGate::of(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(COUNTDOWN_TICK).await;
                presence.until_visible().await;
                let alive = this.update(cx, |this, cx| this.tick(OffsetDateTime::now_utc(), cx));
                if alive.is_err() {
                    break;
                }
            }
        })
    }

    fn tick(&mut self, now: OffsetDateTime, cx: &mut Context<Self>) {
        self.now = now;
        if self.pending_count() > 0 {
            cx.notify();
        }
    }

    fn pending_count(&self) -> usize {
        match &self.state {
            Scheduled::Loading => 0,
            Scheduled::Ready(snapshot) => snapshot.pending.len(),
        }
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        let action_repo = Arc::clone(&self.action_repo);
        async_bridge::run_async(
            &self.rt_handle,
            load_snapshot(repo, action_repo),
            |this, result, cx| match result {
                Ok(snapshot) => this.apply_snapshot(snapshot, OffsetDateTime::now_utc(), cx),
                Err(message) => this.on_load_error(&message, cx),
            },
            cx,
        );
    }

    fn apply_snapshot(
        &mut self,
        snapshot: ScheduledSnapshot,
        now: OffsetDateTime,
        cx: &mut Context<Self>,
    ) {
        self.busy
            .retain(|id| snapshot.pending.iter().any(|row| row.run.id == *id));
        self.state = Scheduled::Ready(snapshot);
        self.now = now;
        cx.notify();
    }

    fn on_load_error(&mut self, message: &str, cx: &mut Context<Self>) {
        if matches!(self.state, Scheduled::Loading) {
            self.state = Scheduled::Ready(ScheduledSnapshot {
                pending: Vec::new(),
                recent: Vec::new(),
            });
        }
        cx.push_toast(
            ToastKind::Error,
            tr!("queues_scheduled_load_failed", error = message),
        );
        cx.notify();
    }

    fn toggle_recent(&mut self, cx: &mut Context<Self>) {
        self.recent_open = !self.recent_open;
        cx.notify();
    }

    fn open_action(&mut self, id: ActionId, cx: &mut Context<Self>) {
        cx.emit(NavRequested(Screen::Actions(Some(id))));
    }

    fn run_now(&mut self, id: ScheduledRunId, label: String, cx: &mut Context<Self>) {
        if !self.busy.insert(id) {
            return;
        }
        let runs = self.runs.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move { runs.run_now(id).await.map_err(|e| e.to_string()) },
            move |this, result, cx| this.on_run_now_result(id, &label, result, cx),
            cx,
        );
        cx.notify();
    }

    fn on_run_now_result(
        &mut self,
        id: ScheduledRunId,
        label: &str,
        result: Result<HandOff, String>,
        cx: &mut Context<Self>,
    ) {
        self.busy.remove(&id);
        let (kind, message) = match result {
            Ok(HandOff::Dispatched) => (
                ToastKind::Success,
                tr!("queues_scheduled_run_now_started", name = label),
            ),
            Ok(HandOff::Skipped(reason)) => (
                ToastKind::Warn,
                tr!(
                    "queues_scheduled_run_now_refused",
                    name = label,
                    reason = reason_label(reason, None)
                ),
            ),
            Ok(HandOff::Failed(reason)) => (
                ToastKind::Error,
                tr!(
                    "queues_scheduled_run_now_refused",
                    name = label,
                    reason = reason_label(reason, None)
                ),
            ),
            Ok(HandOff::NotPending) => (ToastKind::Info, tr!("queues_scheduled_not_pending")),
            Err(message) => (
                ToastKind::Error,
                tr!("queues_scheduled_run_now_failed", error = message),
            ),
        };
        cx.push_toast(kind, message);
        cx.notify();
    }

    fn cancel(&mut self, id: ScheduledRunId, cx: &mut Context<Self>) {
        if !self.busy.insert(id) {
            return;
        }
        let runs = self.runs.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move { runs.cancel(id).await.map_err(|e| e.to_string()) },
            move |this, result, cx| this.on_cancel_result(id, result, cx),
            cx,
        );
        cx.notify();
    }

    fn on_cancel_result(
        &mut self,
        id: ScheduledRunId,
        result: Result<bool, String>,
        cx: &mut Context<Self>,
    ) {
        self.busy.remove(&id);
        match result {
            Ok(true) => {}
            Ok(false) => cx.push_toast(ToastKind::Info, tr!("queues_scheduled_not_pending")),
            Err(message) => cx.push_toast(
                ToastKind::Error,
                tr!("queues_scheduled_cancel_failed", error = message),
            ),
        }
        cx.notify();
    }

    fn render_pending_row(
        &self,
        index: usize,
        row: &ScheduledRow,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let spec = &row.run.spec;
        let id = row.run.id;
        let target = spec.target_action_id;
        let busy = self.busy.contains(&id);

        let mut title = div().flex().items_center().gap(ROW_GAP).child(
            div()
                .id(("sched-target", index))
                .cursor_pointer()
                .font_family(mono_family())
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .hover(|s| s.underline())
                .on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.open_action(target, cx)),
                )
                .child(spec.label.clone()),
        );
        if let Some(key) = spec.key.as_deref() {
            title = title.child(running_pill(key.to_owned(), palette));
        }

        let origin = self.render_origin(index, row, palette, cx);
        let left = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(LINE_GAP)
            .child(title)
            .child(origin);

        let remaining = countdown(spec.due_at, self.now);
        let countdown_ink = match remaining {
            Countdown::DueNow => palette.warning,
            Countdown::In(_) => palette.text_primary,
        };
        let due = div()
            .flex_none()
            .w(DUE_COL_W)
            .flex()
            .flex_col()
            .items_end()
            .gap(LINE_GAP)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(countdown_ink)
                    .child(countdown_label(remaining)),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(local_stamp(spec.due_at, row.offset)),
            );

        let label = spec.label.clone();
        let run_now =
            ghost_button_with_icon(Icon::PlayerPlay, tr!("queues_scheduled_run_now"), palette)
                .density(density)
                .busy(busy)
                .on_click(
                    ("sched-run-now", index),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.run_now(id, label.clone(), cx)
                    }),
                );
        let cancel = ghost_button_with_icon(Icon::X, tr!("queues_scheduled_cancel"), palette)
            .density(density)
            .ink(palette.random)
            .busy(busy)
            .on_click(
                ("sched-cancel", index),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.cancel(id, cx)),
            );
        let buttons = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(run_now)
            .child(cancel);

        row_frame(index, palette, density)
            .child(icon(Icon::Clock, ROW_GLYPH, palette.text_faint))
            .child(left)
            .child(due)
            .child(buttons)
            .into_any_element()
    }

    fn render_origin(
        &self,
        index: usize,
        row: &ScheduledRow,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let caption = div()
            .font_family(body_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint);
        match (row.run.spec.scheduled_by_action, &row.scheduled_by_name) {
            (Some(source), Some(name)) => div()
                .flex()
                .items_center()
                .gap(LINE_GAP)
                .child(caption.child(tr!("queues_scheduled_from")))
                .child(
                    div()
                        .id(("sched-origin", index))
                        .cursor_pointer()
                        .font_family(mono_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_secondary)
                        .hover(|s| s.underline())
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.open_action(source, cx)
                        }))
                        .child(name.clone()),
                )
                .into_any_element(),
            (Some(_), None) => caption
                .child(tr!("queues_scheduled_from_removed"))
                .into_any_element(),
            (None, _) => caption
                .child(tr!("queues_scheduled_from_unknown"))
                .into_any_element(),
        }
    }

    fn render_recent_row(
        &self,
        index: usize,
        row: &ScheduledRow,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let run = &row.run;
        let tone = outcome_tone(run.state);
        let ink = tone_ink(tone, palette);
        let reason = run
            .outcome_reason
            .as_deref()
            .map(|reason| reason_label(reason, row.late_by()))
            .unwrap_or_default();
        let resolved = run
            .resolved_at
            .map(|at| local_stamp(at, row.offset))
            .unwrap_or_default();

        let badge = div()
            .flex_none()
            .py(spacing(Spacing::Xxs, Density::Cozy))
            .px(spacing(Spacing::Xs, Density::Cozy))
            .rounded(BADGE_RADIUS)
            .bg(with_alpha(ink, BADGE_FILL_ALPHA))
            .border(BORDER_THIN)
            .border_color(with_alpha(ink, BADGE_BORDER_ALPHA))
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(ink)
            .child(tr!(outcome_badge_key(tone)));

        let left = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(ROW_GAP)
            .child(
                div()
                    .flex_none()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(run.spec.label.clone()),
            )
            .children(
                run.spec
                    .key
                    .as_deref()
                    .map(|key| running_pill(key.to_owned(), palette)),
            )
            .child(badge)
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(reason),
            );

        row_frame(index, palette, density)
            .child(icon(tone_glyph(tone), ROW_GLYPH, ink))
            .child(left)
            .child(
                div()
                    .flex_none()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(resolved),
            )
            .into_any_element()
    }

    fn render_recent(
        &self,
        recent: &[ScheduledRow],
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chevron = if self.recent_open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        let header = div()
            .id("sched-recent-toggle")
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Xs, density))
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_recent(cx)))
            .child(icon(chevron, BADGE_GLYPH, palette.text_muted))
            .child(section_label(
                tr!("queues_scheduled_recent", count = recent.len() as i64),
                palette,
            ));

        let mut group = div()
            .w_full()
            .flex()
            .flex_col()
            .mt(spacing(Spacing::Xs, density))
            .pt(spacing(Spacing::Xs, density))
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(header);
        if self.recent_open {
            for (index, row) in recent.iter().enumerate() {
                group = group.child(self.render_recent_row(index, row, palette, density));
            }
        }
        group.into_any_element()
    }
}

impl Render for ScheduledRunsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let count = SharedString::from(tr!(
            "queues_scheduled_count",
            count = self.pending_count() as i64
        ));
        let header = hub_section_header(
            tr!("queues_scheduled_title"),
            tr!("queues_scheduled_blurb"),
            Some(count),
            &palette,
        );

        let mut list = div().w_full().flex().flex_col();
        match &self.state {
            Scheduled::Loading => {
                list = list.child(empty_strip(
                    tr!("queues_scheduled_loading"),
                    &palette,
                    density,
                ));
            }
            Scheduled::Ready(snapshot) => {
                if snapshot.pending.is_empty() {
                    list = list.child(empty_strip(
                        tr!("queues_scheduled_empty"),
                        &palette,
                        density,
                    ));
                }
                for (index, row) in snapshot.pending.iter().enumerate() {
                    list = list.child(self.render_pending_row(index, row, &palette, density, cx));
                }
                if !snapshot.recent.is_empty() {
                    list = list.child(self.render_recent(&snapshot.recent, &palette, density, cx));
                }
            }
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .mt(spacing(Spacing::Sm, density))
            .child(header)
            .child(
                card(list, &palette)
                    .full_width()
                    .padding(CARD_PAD)
                    .border_color(palette.border_input),
            )
    }
}

fn row_frame(index: usize, palette: &ForgePalette, density: Density) -> gpui::Div {
    let row = div()
        .w_full()
        .flex()
        .items_center()
        .gap(ROW_GAP)
        .py(spacing(Spacing::Xs, density));
    if index == 0 {
        row
    } else {
        row.border_t(BORDER_THIN)
            .border_color(palette.border_regular)
    }
}

fn empty_strip(caption: String, palette: &ForgePalette, density: Density) -> AnyElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(ROW_GAP)
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .bg(palette.shell)
        .child(icon(Icon::CircleDashed, ROW_GLYPH, palette.text_faint))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(caption),
        )
        .into_any_element()
}

fn tone_ink(tone: OutcomeTone, palette: &ForgePalette) -> Rgba {
    match tone {
        OutcomeTone::Ran => palette.success,
        OutcomeTone::Cancelled => palette.text_faint,
        OutcomeTone::Skipped => palette.warning,
        OutcomeTone::Failed => palette.random,
    }
}

fn tone_glyph(tone: OutcomeTone) -> Icon {
    match tone {
        OutcomeTone::Ran => Icon::PlayerPlay,
        OutcomeTone::Cancelled => Icon::X,
        OutcomeTone::Skipped => Icon::ClockPause,
        OutcomeTone::Failed => Icon::AlertTriangle,
    }
}

async fn load_snapshot(
    repo: Arc<dyn ScheduledRunRepo>,
    action_repo: Arc<dyn ActionRepo>,
) -> Result<ScheduledSnapshot, String> {
    let pending = repo.list_pending().await.map_err(|e| e.to_string())?;
    let recent = repo
        .list_recent_resolved(RECENT_LIMIT)
        .await
        .map_err(|e| e.to_string())?;
    let names: HashMap<ActionId, String> = action_repo
        .list()
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|action| (action.id, action.name))
        .collect();
    let to_row = |run: ScheduledRun| {
        let anchor = run.resolved_at.unwrap_or(run.spec.due_at);
        let scheduled_by_name = run
            .spec
            .scheduled_by_action
            .and_then(|id| names.get(&id).cloned());
        ScheduledRow {
            offset: system_offset_at(anchor),
            scheduled_by_name,
            run,
        }
    };
    Ok(ScheduledSnapshot {
        pending: pending.into_iter().map(to_row).collect(),
        recent: recent.into_iter().map(to_row).collect(),
    })
}
