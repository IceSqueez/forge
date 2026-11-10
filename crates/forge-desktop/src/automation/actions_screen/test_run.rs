use super::*;
use crate::app::async_bridge::{BridgeFlow, drain_subscription};
use crate::app::runtime_event::{self, RuntimeEvent};
use crate::shared::repo_failure::RepoFailure;
use crate::shared::user_text::resolve_user_text;
use forge_components::ROW_OFF_OPACITY;
use forge_components::{
    ActivePresentation, BORDER_THIN, ConfirmTone, Density, FONT_SM, FONT_XS, FONT_XXS,
    KeyActivated, MenuItem, PushToast, Radius, Spacing, ToastKind, confirm_modal, context_menu,
    ghost_button_with_icon, menu_header, menu_item, modal, primary_button, radius, scaled, spacing,
};
use forge_events::{Event, RunOutcome, SubActionDone};
use forge_runtime::{QueueAdmission, RunCanceller, RunCompletion, RunEnd, WatchedRun};
use forge_types::{ArgStack, EventId, ExecutionOutcome};
use gpui::{App, Rgba, Task, relative};
use std::time::Duration;

const MODAL_W: Pixels = px(560.0);
const PROGRESS_H: Pixels = px(4.0);
const MARK: Pixels = px(20.0);
const ROW_PAD_V: Pixels = px(8.0);
const ROW_PAD_H: Pixels = px(11.0);
const ROW_GLYPH: Pixels = px(13.0);
const BANNER_ICON: Pixels = px(15.0);
const EMPTY_PAD: Pixels = px(20.0);
const SELECT_PAD_V: Pixels = px(5.0);
const TRIGGER_TIMEOUT: Duration = Duration::from_secs(3);
const FAIL_TINT_ALPHA: f32 = 0.09;

#[derive(Clone)]
pub(super) enum RowStatus {
    Queued,
    Running,
    Ok { ms: u64 },
    Failed { message: SharedString },
    Skipped,
}

impl RowStatus {
    fn is_terminal(&self) -> bool {
        matches!(
            self,
            RowStatus::Ok { .. } | RowStatus::Failed { .. } | RowStatus::Skipped
        )
    }
}

pub(super) struct TestRunRow {
    pub(super) index: usize,
    pub(super) name: SharedString,
    pub(super) icon: Icon,
    pub(super) color: Rgba,
    pub(super) status: RowStatus,
}

pub(super) struct TestTriggerChoice {
    pub(super) name: SharedString,
    pub(super) kind_label: SharedString,
}

pub(super) enum TestRunNote {
    NoSchema,
    NoTriggers,
}

enum TestRunPhase {
    Awaiting,
    Running,
    Done { errors: usize },
    Halted { step: usize },
    NotStarted,
    Stopped,
}

impl TestRunPhase {
    fn is_terminal(&self) -> bool {
        matches!(
            self,
            TestRunPhase::Done { .. }
                | TestRunPhase::Halted { .. }
                | TestRunPhase::NotStarted
                | TestRunPhase::Stopped
        )
    }
}

enum FireOutcome {
    StartTimeout,
    Repaint,
    Error(String),
}

enum RunControl {
    Dispatching,
    Live(RunCanceller),
    Ended,
    Refused,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AfterClose {
    Stop,
    Report,
}

pub(super) struct TestRunLaunch {
    pub(super) action_id: ActionId,
    pub(super) action_name: SharedString,
    pub(super) queue_id: QueueId,
    pub(super) admission: QueueAdmission,
    pub(super) rows: Vec<TestRunRow>,
    pub(super) triggers: Vec<TestTriggerChoice>,
    pub(super) selected_trigger: Option<usize>,
    pub(super) trigger_kind: Option<String>,
    pub(super) initial_args: ArgStack,
    pub(super) note: Option<TestRunNote>,
}

pub(super) enum TestRunEvent {
    Relaunch {
        action_id: ActionId,
        selected: Option<usize>,
    },
    DetailChanged(ActionId),
    Closed(ActionId),
    Failed(String),
}

pub(super) struct TestRunModal {
    action_id: ActionId,
    action_name: SharedString,
    rows: Vec<TestRunRow>,
    triggers: Vec<TestTriggerChoice>,
    selected_trigger: Option<usize>,
    note: Option<TestRunNote>,
    trigger_menu: Option<Point<Pixels>>,
    menu_focus: FocusHandle,
    menu_focus_restore: Option<FocusHandle>,
    dispatched: EventId,
    root: Option<EventId>,
    top_run_ids: HashMap<EventId, usize>,
    phase: TestRunPhase,
    run: RunControl,
    close_prompt: bool,
    after_close: Option<AfterClose>,
    _bridge: Task<()>,
    _timeout: Option<Task<()>>,
}

impl EventEmitter<TestRunEvent> for TestRunModal {}

struct TestRowLook {
    mark_glyph: String,
    mark_bg: Rgba,
    mark_fg: Rgba,
    label: String,
    label_color: Rgba,
    border: Rgba,
    row_bg: Rgba,
    opacity: f32,
}

impl TestRowLook {
    fn of(row: &TestRunRow, palette: &ForgePalette) -> Self {
        let number = (row.index + 1).to_string();
        match &row.status {
            RowStatus::Queued => Self {
                mark_glyph: number,
                mark_bg: palette.surface_overlay,
                mark_fg: palette.text_faint,
                label: tr!("action_editor_test_run_status_queued"),
                label_color: palette.text_faint,
                border: palette.border_regular,
                row_bg: palette.shell,
                opacity: ROW_OFF_OPACITY,
            },
            RowStatus::Running => Self {
                mark_glyph: number,
                mark_bg: palette.brand,
                mark_fg: palette.shell,
                label: tr!("action_editor_test_run_status_running"),
                label_color: palette.brand,
                border: palette.brand,
                row_bg: palette.shell,
                opacity: 1.0,
            },
            RowStatus::Ok { ms } => Self {
                mark_glyph: "\u{2713}".to_owned(),
                mark_bg: palette.success,
                mark_fg: palette.shell,
                label: tr!(
                    "action_editor_test_run_status_ms",
                    ms = i64::try_from(*ms).unwrap_or(i64::MAX)
                ),
                label_color: palette.success,
                border: palette.border_regular,
                row_bg: palette.shell,
                opacity: 1.0,
            },
            RowStatus::Failed { .. } => Self {
                mark_glyph: "\u{00d7}".to_owned(),
                mark_bg: palette.danger,
                mark_fg: palette.shell,
                label: tr!("action_editor_test_run_status_failed"),
                label_color: palette.danger,
                border: palette.danger,
                row_bg: Rgba {
                    a: FAIL_TINT_ALPHA,
                    ..palette.danger
                },
                opacity: 1.0,
            },
            RowStatus::Skipped => Self {
                mark_glyph: number,
                mark_bg: palette.surface_overlay,
                mark_fg: palette.text_faint,
                label: tr!("action_editor_test_run_status_skipped"),
                label_color: palette.text_muted,
                border: palette.border_regular,
                row_bg: palette.shell,
                opacity: ROW_OFF_OPACITY,
            },
        }
    }
}

impl TestRunModal {
    pub(super) fn new(
        launch: TestRunLaunch,
        bus: Arc<EventBus>,
        scheduler: QueueSchedulerHandle,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let TestRunLaunch {
            action_id,
            action_name,
            queue_id,
            admission,
            rows,
            triggers,
            selected_trigger,
            trigger_kind,
            initial_args,
            note,
        } = launch;

        let subscription = bus.subscribe();
        let bridge = cx.spawn(async move |this, cx| {
            drain_subscription(subscription, cx, move |batch, cx| {
                for event in batch {
                    match this.update(cx, |this, cx| this.on_test_event(event, cx)) {
                        Ok(true) | Err(_) => return BridgeFlow::Stop,
                        Ok(false) => {}
                    }
                }
                BridgeFlow::Continue
            })
            .await;
        });

        let dispatch_root = super::test_trigger::manual_test_root(action_id);
        let dispatched = dispatch_root.id;
        let reply = crate::app::async_bridge::on_runtime(&rt_handle, async move {
            super::test_trigger::dispatch_test_run(
                &scheduler,
                &bus,
                super::test_trigger::TestRunRequest {
                    root: dispatch_root,
                    action_id,
                    queue_id,
                    admission,
                    trigger_kind,
                    initial_args,
                },
            )
            .await
        });
        cx.spawn(async move |this, cx| {
            let Some(modal) = this.upgrade() else {
                return;
            };
            let Ok(result) = reply.await else {
                return;
            };
            let completion =
                modal.update(cx, |modal, cx| modal.on_test_fired(action_id, result, cx));
            if let Some(completion) = completion {
                let end = completion.finished().await;
                modal.update(cx, |modal, cx| modal.on_run_end(end, cx));
            }
        })
        .detach();

        TestRunModal {
            action_id,
            action_name,
            rows,
            triggers,
            selected_trigger,
            note,
            trigger_menu: None,
            menu_focus: cx.focus_handle(),
            menu_focus_restore: None,
            dispatched,
            root: None,
            top_run_ids: HashMap::new(),
            phase: TestRunPhase::Awaiting,
            run: RunControl::Dispatching,
            close_prompt: false,
            after_close: None,
            _bridge: bridge,
            _timeout: None,
        }
    }

    fn completed(&self) -> usize {
        self.rows.iter().filter(|r| r.status.is_terminal()).count()
    }

    fn errors(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| matches!(r.status, RowStatus::Failed { .. }))
            .count()
    }

    fn progress_fraction(&self) -> f32 {
        if self.rows.is_empty() {
            if self.phase.is_terminal() { 1.0 } else { 0.0 }
        } else {
            self.completed() as f32 / self.rows.len() as f32
        }
    }

    fn run_test_again(&mut self, cx: &mut Context<Self>) {
        cx.emit(TestRunEvent::Relaunch {
            action_id: self.action_id,
            selected: self.selected_trigger,
        });
    }

    fn pick_test_trigger(&mut self, index: usize, cx: &mut Context<Self>) {
        cx.emit(TestRunEvent::Relaunch {
            action_id: self.action_id,
            selected: Some(index),
        });
    }

    fn toggle_trigger_menu(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        self.trigger_menu = if self.trigger_menu.is_some() {
            None
        } else {
            Some(pos)
        };
        cx.notify();
    }

    fn close_trigger_menu(&mut self, cx: &mut Context<Self>) {
        self.trigger_menu = None;
        cx.notify();
    }

    fn run_settled(&self) -> bool {
        matches!(self.run, RunControl::Ended | RunControl::Refused)
            || matches!(
                self.phase,
                TestRunPhase::Done { .. } | TestRunPhase::Halted { .. }
            )
    }

    fn request_close(&mut self, cx: &mut Context<Self>) {
        if self.run_settled() {
            cx.emit(TestRunEvent::Closed(self.action_id));
        } else {
            self.close_prompt = true;
            cx.notify();
        }
    }

    fn return_to_test_run(&mut self, cx: &mut Context<Self>) {
        self.close_prompt = false;
        cx.notify();
    }

    fn stop_test(&mut self, cx: &mut Context<Self>) {
        self.close_prompt = false;
        self.after_close = Some(AfterClose::Stop);
        if let RunControl::Live(canceller) = &self.run {
            canceller.cancel();
        }
        cx.emit(TestRunEvent::Closed(self.action_id));
        cx.notify();
    }

    fn keep_running(&mut self, cx: &mut Context<Self>) {
        self.close_prompt = false;
        self.after_close = Some(AfterClose::Report);
        cx.emit(TestRunEvent::Closed(self.action_id));
        cx.notify();
    }

    pub(super) fn continue_in_background(&mut self) {
        if !self.run_settled() && self.after_close.is_none() {
            self.after_close = Some(AfterClose::Report);
        }
    }

    fn on_test_fired(
        &mut self,
        id: ActionId,
        result: Result<WatchedRun, RepoFailure>,
        cx: &mut Context<Self>,
    ) -> Option<RunCompletion> {
        if self.action_id != id {
            return None;
        }
        let (outcome, completion) = match result {
            Ok(WatchedRun {
                canceller,
                completion,
            }) => {
                if self.after_close == Some(AfterClose::Stop) {
                    canceller.cancel();
                }
                self.run = RunControl::Live(canceller);
                let outcome = if matches!(self.phase, TestRunPhase::Awaiting) {
                    FireOutcome::StartTimeout
                } else {
                    FireOutcome::Repaint
                };
                (outcome, Some(completion))
            }
            Err(message) => {
                self.run = RunControl::Refused;
                (FireOutcome::Error(message.text()), None)
            }
        };
        match outcome {
            FireOutcome::StartTimeout => self.start_test_timeout(cx),
            FireOutcome::Repaint => cx.notify(),
            FireOutcome::Error(message) if self.after_close.is_some() => cx.push_toast(
                ToastKind::Error,
                tr!("action_editor_test_failed", error = message.as_str()),
            ),
            FireOutcome::Error(message) => cx.emit(TestRunEvent::Failed(message)),
        }
        completion
    }

    fn on_run_end(&mut self, end: RunEnd, cx: &mut Context<Self>) {
        match self.after_close {
            Some(AfterClose::Report) => {
                let (kind, message) = background_report(&self.action_name, &end);
                cx.push_toast(kind, message);
            }
            Some(AfterClose::Stop) => {}
            None => {
                self.close_prompt = false;
                let ended_early = match &end {
                    RunEnd::NotStarted => Some(TestRunPhase::NotStarted),
                    RunEnd::Finished(ExecutionOutcome::Cancelled) => Some(TestRunPhase::Stopped),
                    RunEnd::Finished(_) => None,
                };
                if let Some(phase) = ended_early
                    && !matches!(
                        self.phase,
                        TestRunPhase::Done { .. } | TestRunPhase::Halted { .. }
                    )
                {
                    self.phase = phase;
                }
            }
        }
        self.run = RunControl::Ended;
        cx.notify();
    }

    fn start_test_timeout(&mut self, cx: &mut Context<Self>) {
        let timeout = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TRIGGER_TIMEOUT).await;
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = this.update(cx, |this, cx| this.on_test_timeout(cx));
        });
        self._timeout = Some(timeout);
        cx.notify();
    }

    fn on_test_timeout(&mut self, cx: &mut Context<Self>) {
        if matches!(self.phase, TestRunPhase::Awaiting) {
            self.phase = TestRunPhase::NotStarted;
            cx.notify();
        }
    }

    fn on_test_event(&mut self, event: &Event, cx: &mut Context<Self>) -> bool {
        match runtime_event::decode(event) {
            Some(RuntimeEvent::ActionStarted(started)) => {
                if self.root.is_some()
                    || started.action_id != self.action_id
                    || event.caused_by != Some(self.dispatched)
                {
                    return false;
                }
                self.root = Some(event.id);
                if !self.phase.is_terminal() || matches!(self.phase, TestRunPhase::NotStarted) {
                    self.phase = TestRunPhase::Running;
                }
                cx.notify();
                false
            }
            Some(RuntimeEvent::ActionSkipped(skipped)) => {
                if skipped.action_id == self.action_id
                    && event.caused_by == Some(self.dispatched)
                    && matches!(self.phase, TestRunPhase::Awaiting)
                {
                    self.phase = TestRunPhase::NotStarted;
                    cx.notify();
                    return true;
                }
                false
            }
            Some(RuntimeEvent::SubActionRun(run)) => {
                let Some(root) = self.root else {
                    return false;
                };
                if event.caused_by != Some(root) {
                    return false;
                }
                if let Some(index) = step_row_index(run.step_index) {
                    self.top_run_ids.insert(event.id, index);
                    if let Some(row) = self.rows.get_mut(index) {
                        row.status = RowStatus::Running;
                    }
                    cx.notify();
                }
                false
            }
            Some(RuntimeEvent::SubActionDone(done)) => {
                let Some(root) = self.root else {
                    return false;
                };
                let is_disabled_top = event.caused_by == Some(root);
                let is_executed_top = event
                    .caused_by
                    .is_some_and(|c| self.top_run_ids.contains_key(&c));
                if !is_disabled_top && !is_executed_top {
                    return false;
                }
                if let Some(index) = step_row_index(done.step_index)
                    && let Some(row) = self.rows.get_mut(index)
                {
                    row.status = row_status_from_done(&done);
                    cx.notify();
                }
                false
            }
            Some(RuntimeEvent::ActionDone(done)) => {
                if event.caused_by != self.root {
                    return false;
                }
                if done.outcome == RunOutcome::Failed {
                    let step = self
                        .rows
                        .iter()
                        .rposition(|r| matches!(r.status, RowStatus::Failed { .. }))
                        .unwrap_or(self.rows.len().saturating_sub(1));
                    self.phase = TestRunPhase::Halted { step };
                } else {
                    self.phase = TestRunPhase::Done {
                        errors: self.errors(),
                    };
                }
                cx.emit(TestRunEvent::DetailChanged(self.action_id));
                cx.notify();
                true
            }
            _ => false,
        }
    }

    fn render_trigger_selector(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let label: SharedString = self
            .selected_trigger
            .and_then(|i| self.triggers.get(i))
            .map(|choice| choice.kind_label.clone())
            .unwrap_or_else(|| tr!("action_editor_test_run_subtitle_none").into());
        let hover = palette.surface_overlay;

        div()
            .id("actions-test-trigger-select")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(scaled(SELECT_PAD_V, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .bg(palette.shell)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .child(icon(Icon::Bolt, ROW_GLYPH, palette.brand))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(label),
            )
            .child(icon(Icon::ChevronDown, ROW_GLYPH, palette.text_faint))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(|this, ev: &ClickEvent, _, cx| {
                this.toggle_trigger_menu(ev.position(), cx)
            }))
            .into_any_element()
    }

    fn render_trigger_menu(
        &self,
        pos: Point<Pixels>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let mut items: Vec<MenuItem> =
            vec![menu_header(tr!("action_editor_test_run_trigger_pick"))];
        for (i, choice) in self.triggers.iter().enumerate() {
            let mut entry = menu_item(
                SharedString::from(format!("actions-test-trigger-{i}")),
                format!("{} · {}", choice.name, choice.kind_label),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_test_trigger(i, cx)),
            )
            .icon(Icon::Bolt);
            if self.selected_trigger == Some(i) {
                entry = entry.color(palette.brand);
            }
            items.push(entry.into());
        }

        context_menu(pos, palette)
            .items(items)
            .dismiss_on_escape(&self.menu_focus)
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_trigger_menu(cx));
            })
            .into_any_element()
    }

    fn render_test_run_body(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let progress_color = match self.phase {
            TestRunPhase::Halted { .. } => palette.danger,
            TestRunPhase::Done { errors } if errors > 0 => palette.warning,
            TestRunPhase::Done { .. } => palette.success,
            TestRunPhase::NotStarted | TestRunPhase::Stopped => palette.text_faint,
            _ => palette.brand,
        };
        let progress = div()
            .w_full()
            .h(PROGRESS_H)
            .rounded(radius(Radius::Sm))
            .bg(palette.shell)
            .overflow_hidden()
            .child(
                div()
                    .h_full()
                    .w(relative(self.progress_fraction()))
                    .bg(progress_color),
            );

        let rows: AnyElement = if self.rows.is_empty() {
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .p(scaled(EMPTY_PAD, density))
                .font_family(cx.body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(tr!("action_editor_test_run_empty"))
                .into_any_element()
        } else {
            let mut col = div().flex().flex_col().gap(spacing(Spacing::Xs, density));
            for row in &self.rows {
                col = col.child(self.render_test_run_row(row, palette, density, cx));
            }
            col.into_any_element()
        };

        let mut body = div().flex().flex_col().gap(spacing(Spacing::Sm, density));
        if self.triggers.len() > 1 {
            body = body.child(self.render_trigger_selector(palette, cx));
        }
        body = body.child(progress).child(rows);
        if let Some(note) = &self.note {
            let text = match note {
                TestRunNote::NoSchema => tr!("action_editor_test_run_note_no_schema"),
                TestRunNote::NoTriggers => tr!("action_editor_test_run_note_no_triggers"),
            };
            body = body.child(
                div()
                    .w_full()
                    .font_family(cx.body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(text),
            );
        }
        if let Some(banner) = self.render_test_run_banner(palette, density, cx) {
            body = body.child(banner);
        }
        body.into_any_element()
    }

    fn render_test_run_row(
        &self,
        row: &TestRunRow,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let look = TestRowLook::of(row, palette);

        let mark = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(MARK)
            .rounded(radius(Radius::Pill))
            .bg(look.mark_bg)
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XXS)
                    .text_color(look.mark_fg)
                    .child(look.mark_glyph),
            );

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(ROW_PAD_V, density))
            .px(scaled(ROW_PAD_H, density))
            .rounded(radius(Radius::Md))
            .bg(look.row_bg)
            .border(BORDER_THIN)
            .border_color(look.border)
            .opacity(look.opacity)
            .child(mark)
            .child(icon(row.icon, ROW_GLYPH, row.color))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(row.name.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XXS)
                    .text_color(look.label_color)
                    .child(look.label),
            )
            .into_any_element()
    }

    fn render_test_run_banner(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> Option<AnyElement> {
        match &self.phase {
            TestRunPhase::Halted { step } => {
                let row = self.rows.get(*step);
                let name = row
                    .map(|r| r.name.clone())
                    .unwrap_or_else(|| SharedString::from(""));
                let message = row
                    .and_then(|r| match &r.status {
                        RowStatus::Failed { message, .. } => Some(message.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| tr!("action_editor_test_run_default_error").into());
                Some(Self::test_run_banner(
                    Icon::CircleX,
                    palette.danger,
                    tr!(
                        "action_editor_test_run_failed_banner",
                        step = step.saturating_add(1),
                        name = name.as_ref()
                    ),
                    Some(message),
                    palette,
                    density,
                    cx,
                ))
            }
            TestRunPhase::Done { errors } if *errors > 0 => Some(Self::test_run_banner(
                Icon::AlertTriangle,
                palette.warning,
                tr!(
                    "action_editor_test_run_completed",
                    count = i64::try_from(self.rows.len()).unwrap_or(i64::MAX),
                    errors = i64::try_from(*errors).unwrap_or(i64::MAX)
                ),
                None,
                palette,
                density,
                cx,
            )),
            TestRunPhase::Done { .. } if !self.rows.is_empty() => Some(Self::test_run_banner(
                Icon::CircleCheck,
                palette.success,
                tr!(
                    "action_editor_test_run_completed",
                    count = i64::try_from(self.rows.len()).unwrap_or(i64::MAX),
                    errors = 0
                ),
                None,
                palette,
                density,
                cx,
            )),
            TestRunPhase::NotStarted => Some(Self::test_run_banner(
                Icon::InfoCircle,
                palette.text_muted,
                tr!("action_editor_test_run_notstarted"),
                None,
                palette,
                density,
                cx,
            )),
            _ => None,
        }
    }

    fn test_run_banner(
        glyph: Icon,
        tint: Rgba,
        title: String,
        message: Option<SharedString>,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let heading = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(glyph, BANNER_ICON, tint))
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(tint)
                    .child(title),
            );

        let mut banner = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .bg(Rgba {
                a: FAIL_TINT_ALPHA,
                ..tint
            })
            .border(BORDER_THIN)
            .border_color(tint)
            .child(heading);
        if let Some(message) = message {
            banner = banner.child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_secondary)
                    .child(message),
            );
        }
        banner.into_any_element()
    }

    fn render_test_run_footer(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let status = match self.phase {
            TestRunPhase::Halted { .. } => tr!("action_editor_test_run_foot_halted"),
            TestRunPhase::Done { .. } => tr!("action_editor_test_run_foot_finished"),
            TestRunPhase::NotStarted => tr!("action_editor_test_run_foot_notstarted"),
            TestRunPhase::Stopped => tr!("action_editor_test_run_foot_stopped"),
            _ => tr!("action_editor_test_run_foot_simulating"),
        };

        let mut actions = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density));
        if self.phase.is_terminal() {
            actions = actions.child(
                ghost_button_with_icon(Icon::Refresh, tr!("action_editor_test_run_again"), palette)
                    .on_click(
                        "actions-test-run-again",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.run_test_again(cx)),
                    ),
            );
        }
        actions = actions.child(
            primary_button(tr!("action_editor_test_run_close"), palette).on_click(
                "actions-test-run-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.request_close(cx)),
            ),
        );

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(cx.body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(status),
            )
            .child(actions)
            .into_any_element()
    }
}

impl Render for TestRunModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drive_overlay_focus(
            self.trigger_menu.is_some(),
            &self.menu_focus,
            &mut self.menu_focus_restore,
            window,
            cx,
        );
        let palette = cx.palette();
        let palette = &palette;
        let halted = matches!(self.phase, TestRunPhase::Halted { .. });
        let header_tint = if halted {
            palette.danger
        } else {
            palette.success
        };

        let subtitle = match self.selected_trigger.and_then(|i| self.triggers.get(i)) {
            Some(choice) => tr!(
                "action_editor_test_run_subtitle_trigger",
                name = choice.kind_label.as_ref()
            ),
            None => tr!("action_editor_test_run_subtitle_none"),
        };

        let body = self.render_test_run_body(palette, cx);
        let footer = self.render_test_run_footer(palette, cx);

        let card = modal(
            tr!(
                "action_editor_test_run_title",
                name = self.action_name.as_ref()
            ),
            body,
            palette,
        )
        .width(MODAL_W)
        .header_icon(Icon::PlayerPlay, header_tint)
        .subtitle(subtitle)
        .footer(footer)
        .on_close(
            "actions-test-run-close-x",
            cx.listener(|this, _: &ClickEvent, _, cx| this.request_close(cx)),
        );

        let view = cx.entity();
        let card_overlay = overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-test-run-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.request_close(cx));
            });

        let menu = self
            .trigger_menu
            .map(|pos| self.render_trigger_menu(pos, palette, cx));
        let close_prompt = self
            .close_prompt
            .then(|| self.render_close_prompt(palette, cx));

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(card_overlay)
            .children(menu)
            .children(close_prompt)
    }
}

impl TestRunModal {
    fn render_close_prompt(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let card = confirm_modal(
            tr!("action_editor_test_run_busy_title"),
            tr!("action_editor_test_run_busy_body"),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(self.action_name.clone())
        .esc_hint(tr!("action_editor_test_run_busy_esc_hint"))
        .on_cancel(
            "actions-test-run-keep",
            tr!("action_editor_test_run_busy_keep"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.keep_running(cx)),
        )
        .on_confirm(
            "actions-test-run-stop",
            tr!("action_editor_test_run_busy_stop"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.stop_test(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-test-run-busy-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.return_to_test_run(cx));
            })
            .into_any_element()
    }
}

fn background_report(action_name: &str, end: &RunEnd) -> (ToastKind, String) {
    match end {
        RunEnd::Finished(ExecutionOutcome::Success) => (
            ToastKind::Success,
            tr!("action_editor_test_run_toast_finished", name = action_name),
        ),
        RunEnd::Finished(ExecutionOutcome::Failed(reason)) => {
            let reason = resolve_user_text(reason).inline();
            (
                ToastKind::Error,
                tr!(
                    "action_editor_test_run_toast_failed",
                    name = action_name,
                    error = reason.as_str()
                ),
            )
        }
        RunEnd::Finished(ExecutionOutcome::Cancelled) => (
            ToastKind::Info,
            tr!("action_editor_test_run_toast_stopped", name = action_name),
        ),
        RunEnd::NotStarted => (
            ToastKind::Warn,
            tr!(
                "action_editor_test_run_toast_notstarted",
                name = action_name
            ),
        ),
    }
}

fn step_row_index(step_index: usize) -> Option<usize> {
    (step_index != usize::MAX).then_some(step_index)
}

fn row_status_from_done(done: &SubActionDone) -> RowStatus {
    match done.outcome {
        RunOutcome::Failed => RowStatus::Failed {
            message: done
                .message_text
                .as_ref()
                .map(|text| SharedString::from(resolve_user_text(text).inline()))
                .unwrap_or_else(|| tr!("action_editor_test_run_default_error").into()),
        },
        RunOutcome::Skipped => RowStatus::Skipped,
        RunOutcome::Success => RowStatus::Ok {
            ms: done.duration_ms,
        },
    }
}

#[cfg(test)]
mod tests {
    use forge_runtime::{ActionCancelRegistry, Catalog, QueueScheduler, spawn_action_engine};
    use forge_types::{IntegrationId, UserText, integration_disabled_message};

    use super::*;
    use crate::test_support::{
        StubEventLog, owned_sub_actions, runtime, sandboxed_provider, seed_action, step,
    };

    const OBS_SCENE: &str = "obs.scene.set";
    const RUN_SETTLE: Duration = Duration::from_secs(30);

    fn launch(action_id: ActionId, queue_id: QueueId) -> TestRunLaunch {
        TestRunLaunch {
            action_id,
            action_name: "Scene".into(),
            queue_id,
            admission: QueueAdmission::default(),
            rows: vec![TestRunRow {
                index: 0,
                name: OBS_SCENE.into(),
                icon: Icon::Bolt,
                color: gpui::rgba(0),
                status: RowStatus::Queued,
            }],
            triggers: Vec::new(),
            selected_trigger: None,
            trigger_kind: None,
            initial_args: ArgStack::new(),
            note: Some(TestRunNote::NoTriggers),
        }
    }

    async fn action_done(mut sub: forge_events::EventSubscription) {
        while let Ok(event) = sub.recv().await {
            if event.kind == "action.done" {
                return;
            }
        }
    }

    #[gpui::test]
    fn a_run_refused_at_once_by_a_switched_off_integration_halts_with_the_reason(
        cx: &mut gpui::TestAppContext,
    ) {
        let rt = runtime();
        let obs = IntegrationId::new("obs");
        let (_provider, bus, scheduler, action_id, queue_id) = rt.block_on(async {
            let provider = sandboxed_provider().await;
            let action_id = seed_action(&provider, "Scene", vec![step(OBS_SCENE, true)]).await;
            let queue = provider
                .queue_repo()
                .get(forge_types::DEFAULT_QUEUE_ID)
                .await
                .unwrap()
                .expect("migrations seed the default queue");
            let queue_id = queue.id;
            let bus = EventBus::new(Arc::new(StubEventLog));
            let engine = spawn_action_engine(forge_runtime::ActionEngineParts {
                bus: Arc::clone(&bus),
                catalog: Catalog::from_provider(provider.as_ref()),
                actions: provider.action_repo(),
                history: provider.history_repo(),
                sub_action_registry: Arc::new(owned_sub_actions(&[(OBS_SCENE, "obs")])),
                cancel_registry: Arc::new(ActionCancelRegistry::new()),
                date_names: forge_runtime::DateNamesHandle::default(),
                config: forge_runtime::Config::default(),
            });
            engine.integration_gate().disable(obs.clone());
            let scheduler = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
            (provider, bus, scheduler, action_id, queue_id)
        });
        let finished = bus.subscribe();

        let modal = cx.update(|cx| {
            cx.new(|cx| {
                TestRunModal::new(
                    launch(action_id, queue_id),
                    Arc::clone(&bus),
                    scheduler.clone(),
                    rt.handle().clone(),
                    cx,
                )
            })
        });
        rt.block_on(async { tokio::time::timeout(RUN_SETTLE, action_done(finished)).await })
            .expect("the engine finishes the refused run");
        cx.run_until_parked();

        let (halted_at, reason) = modal.read_with(cx, |modal, _| {
            let halted_at = match modal.phase {
                TestRunPhase::Halted { step } => Some(step),
                _ => None,
            };
            let reason = match &modal.rows[0].status {
                RowStatus::Failed { message } => Some(message.to_string()),
                _ => None,
            };
            (halted_at, reason)
        });
        assert_eq!(
            (halted_at, reason),
            (
                Some(0),
                Some(
                    crate::shared::user_text::resolve_user_text(&UserText::from(
                        integration_disabled_message(&obs)
                    ))
                    .inline()
                )
            )
        );
    }
}
