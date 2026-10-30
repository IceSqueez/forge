use super::*;
use crate::async_bridge::{BridgeFlow, drain_subscription};
use crate::runtime_event::{self, RuntimeEvent};
use crate::user_text::resolve_user_text;
use forge_components::ROW_OFF_OPACITY;
use forge_components::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, FONT_XXS, MenuItem, Radius, Spacing, body_family,
    context_menu, ghost_button_with_icon, menu_header, menu_item, modal, mono_family,
    primary_button, radius, scaled, spacing,
};
use forge_events::{Event, RunOutcome, SubActionDone};
use forge_registry::{SynthesisSample, synthesize_args};
use forge_runtime::QueueAdmission;
use forge_types::{ArgStack, EventId};
use gpui::{Rgba, Task, relative};
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
enum RowStatus {
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

struct TestRunRow {
    index: usize,
    name: SharedString,
    icon: Icon,
    color: Rgba,
    status: RowStatus,
}

struct TestTriggerChoice {
    name: SharedString,
    kind_label: SharedString,
}

enum TestRunNote {
    NoSchema,
    NoTriggers,
}

enum TestRunPhase {
    Awaiting,
    Running,
    Done { errors: usize },
    Halted { step: usize },
    NotStarted,
}

impl TestRunPhase {
    fn is_terminal(&self) -> bool {
        matches!(
            self,
            TestRunPhase::Done { .. } | TestRunPhase::Halted { .. } | TestRunPhase::NotStarted
        )
    }
}

enum FireOutcome {
    StartTimeout,
    Repaint,
    Error(String),
}

pub(super) struct TestRunLaunch {
    action_id: ActionId,
    action_name: SharedString,
    queue_id: QueueId,
    admission: QueueAdmission,
    rows: Vec<TestRunRow>,
    triggers: Vec<TestTriggerChoice>,
    selected_trigger: Option<usize>,
    trigger_kind: Option<String>,
    initial_args: ArgStack,
    note: Option<TestRunNote>,
}

pub(super) enum TestRunEvent {
    Relaunch {
        action_id: ActionId,
        selected: Option<usize>,
    },
    DetailChanged(ActionId),
    Cancelled(ActionId),
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
    root: Option<EventId>,
    top_run_ids: HashMap<EventId, usize>,
    phase: TestRunPhase,
    _bridge: Task<()>,
    _fire: Task<()>,
    _timeout: Option<Task<()>>,
}

impl EventEmitter<TestRunEvent> for TestRunModal {}

impl TestRunModal {
    fn new(
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

        let (tx, rx) = tokio::sync::oneshot::channel();
        rt_handle.spawn(async move {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = tx.send(
                super::test_trigger::dispatch_test_run(
                    &scheduler,
                    &bus,
                    action_id,
                    queue_id,
                    admission,
                    trigger_kind,
                    initial_args,
                )
                .await,
            );
        });
        let fire = cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the view may close before the result arrives"
                )]
                let _ = this.update(cx, |this, cx| this.on_test_fired(action_id, result, cx));
            }
        });

        TestRunModal {
            action_id,
            action_name,
            rows,
            triggers,
            selected_trigger,
            note,
            trigger_menu: None,
            root: None,
            top_run_ids: HashMap::new(),
            phase: TestRunPhase::Awaiting,
            _bridge: bridge,
            _fire: fire,
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

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(TestRunEvent::Cancelled(self.action_id));
    }

    fn on_test_fired(&mut self, id: ActionId, result: Result<(), String>, cx: &mut Context<Self>) {
        if self.action_id != id {
            return;
        }
        let outcome = match result {
            Ok(()) => {
                if matches!(self.phase, TestRunPhase::Awaiting) {
                    FireOutcome::StartTimeout
                } else {
                    FireOutcome::Repaint
                }
            }
            Err(message) => FireOutcome::Error(message),
        };
        match outcome {
            FireOutcome::StartTimeout => self.start_test_timeout(cx),
            FireOutcome::Repaint => cx.notify(),
            FireOutcome::Error(message) => cx.emit(TestRunEvent::Failed(message)),
        }
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
                if self.root.is_some() || started.action_id != self.action_id {
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
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(label),
            )
            .child(icon(Icon::ChevronDown, ROW_GLYPH, palette.text_faint))
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
            TestRunPhase::NotStarted => palette.text_faint,
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
                .font_family(body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(tr!("action_editor_test_run_empty"))
                .into_any_element()
        } else {
            let mut col = div().flex().flex_col().gap(spacing(Spacing::Xs, density));
            for row in &self.rows {
                col = col.child(self.render_test_run_row(row, palette, density));
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
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(text),
            );
        }
        if let Some(banner) = self.render_test_run_banner(palette, density) {
            body = body.child(banner);
        }
        body.into_any_element()
    }

    fn render_test_run_row(
        &self,
        row: &TestRunRow,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let number = (row.index + 1).to_string();
        let (mark_glyph, mark_bg, mark_fg, label, label_color, border, row_bg, opacity) =
            match &row.status {
                RowStatus::Queued => (
                    number.clone(),
                    palette.surface_overlay,
                    palette.text_faint,
                    tr!("action_editor_test_run_status_queued"),
                    palette.text_faint,
                    palette.border_regular,
                    palette.shell,
                    ROW_OFF_OPACITY,
                ),
                RowStatus::Running => (
                    number.clone(),
                    palette.brand,
                    palette.shell,
                    tr!("action_editor_test_run_status_running"),
                    palette.brand,
                    palette.brand,
                    palette.shell,
                    1.0,
                ),
                RowStatus::Ok { ms } => (
                    "\u{2713}".to_owned(),
                    palette.success,
                    palette.shell,
                    tr!("action_editor_test_run_status_ms", ms = *ms as i64),
                    palette.success,
                    palette.border_regular,
                    palette.shell,
                    1.0,
                ),
                RowStatus::Failed { .. } => (
                    "\u{00d7}".to_owned(),
                    palette.danger,
                    palette.shell,
                    tr!("action_editor_test_run_status_failed"),
                    palette.danger,
                    palette.danger,
                    Rgba {
                        a: FAIL_TINT_ALPHA,
                        ..palette.danger
                    },
                    1.0,
                ),
                RowStatus::Skipped => (
                    number.clone(),
                    palette.surface_overlay,
                    palette.text_faint,
                    tr!("action_editor_test_run_status_skipped"),
                    palette.text_muted,
                    palette.border_regular,
                    palette.shell,
                    ROW_OFF_OPACITY,
                ),
            };

        let mark = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(MARK)
            .rounded(radius(Radius::Pill))
            .bg(mark_bg)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(mark_fg)
                    .child(mark_glyph),
            );

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(ROW_PAD_V, density))
            .px(scaled(ROW_PAD_H, density))
            .rounded(radius(Radius::Md))
            .bg(row_bg)
            .border(BORDER_THIN)
            .border_color(border)
            .opacity(opacity)
            .child(mark)
            .child(icon(row.icon, ROW_GLYPH, row.color))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(row.name.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(label_color)
                    .child(label),
            )
            .into_any_element()
    }

    fn render_test_run_banner(
        &self,
        palette: &ForgePalette,
        density: Density,
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
                        step = (*step as i64) + 1,
                        name = name.as_ref()
                    ),
                    Some(message),
                    palette,
                    density,
                ))
            }
            TestRunPhase::Done { errors } if *errors > 0 => Some(Self::test_run_banner(
                Icon::AlertTriangle,
                palette.warning,
                tr!(
                    "action_editor_test_run_completed",
                    count = self.rows.len() as i64,
                    errors = *errors as i64
                ),
                None,
                palette,
                density,
            )),
            TestRunPhase::Done { .. } if !self.rows.is_empty() => Some(Self::test_run_banner(
                Icon::CircleCheck,
                palette.success,
                tr!(
                    "action_editor_test_run_completed",
                    count = self.rows.len() as i64,
                    errors = 0
                ),
                None,
                palette,
                density,
            )),
            TestRunPhase::NotStarted => Some(Self::test_run_banner(
                Icon::InfoCircle,
                palette.text_muted,
                tr!("action_editor_test_run_notstarted"),
                None,
                palette,
                density,
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
    ) -> AnyElement {
        let heading = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(icon(glyph, BANNER_ICON, tint))
            .child(
                div()
                    .font_family(body_family())
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
                    .font_family(mono_family())
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
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
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
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(status),
            )
            .child(actions)
            .into_any_element()
    }
}

impl Render for TestRunModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        );

        let view = cx.entity();
        let card_overlay = overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-test-run-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel(cx));
            });

        let menu = self
            .trigger_menu
            .map(|pos| self.render_trigger_menu(pos, palette, cx));

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(card_overlay)
            .children(menu)
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

impl ScreenActionsView {
    pub(super) fn start_test_run(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        let selected = match self.detail.as_ref() {
            Some(detail) => detail
                .trigger_instances
                .iter()
                .position(|inst| inst.enabled)
                .or_else(|| (!detail.trigger_instances.is_empty()).then_some(0)),
            None => return,
        };
        self.launch_test_run(id, selected, cx);
    }

    fn launch_test_run(&mut self, id: ActionId, selected: Option<usize>, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let prepared = {
            let Some(detail) = self.detail.as_ref() else {
                return;
            };
            if detail.action.id != id {
                return;
            }
            let action = &detail.action;
            let action_name: SharedString = action.name.clone().into();
            let queue_id = action.queue_id;
            let admission = QueueAdmission::from(action);

            let rows: Vec<TestRunRow> = action
                .sub_actions
                .iter()
                .enumerate()
                .map(|(index, step)| self.test_run_row(index, step, &palette))
                .collect();

            let triggers: Vec<TestTriggerChoice> = detail
                .trigger_instances
                .iter()
                .map(|inst| {
                    let descriptor = self.trigger_registry.get(&inst.kind_id);
                    let kind_label = descriptor
                        .map(crate::kind_copy::trigger_name)
                        .unwrap_or_else(|| inst.kind_id.clone());
                    TestTriggerChoice {
                        name: crate::kind_copy::trigger_instance_name(&inst.name, descriptor)
                            .into(),
                        kind_label: kind_label.into(),
                    }
                })
                .collect();

            let selected_inst = selected.and_then(|i| detail.trigger_instances.get(i));
            let trigger_kind = selected_inst.map(|inst| inst.kind_id.clone());
            let declared = selected_inst.and_then(|inst| {
                let descriptor = self.trigger_registry.get(&inst.kind_id)?;
                let variables = self.trigger_registry.declared_variables(&inst.kind_id)?;
                Some((variables, descriptor.platform_contract()))
            });
            let (initial_args, note) = match (selected_inst, declared) {
                (Some(_), Some((variables, contract))) => (
                    synthesize_args(variables, &SynthesisSample::random(contract)),
                    None,
                ),
                (Some(_), None) => (ArgStack::new(), Some(TestRunNote::NoSchema)),
                (None, _) => (ArgStack::new(), Some(TestRunNote::NoTriggers)),
            };

            TestRunLaunch {
                action_id: id,
                action_name,
                queue_id,
                admission,
                rows,
                triggers,
                selected_trigger: selected,
                trigger_kind,
                initial_args,
                note,
            }
        };

        let bus = Arc::clone(&self.bus);
        let scheduler = self.scheduler.clone();
        let rt_handle = self.rt_handle.clone();
        let modal = cx.new(|cx| TestRunModal::new(prepared, bus, scheduler, rt_handle, cx));
        self._test_run_sub = Some(cx.subscribe(&modal, Self::on_test_run_event));
        self.test_run = Some(modal);
        cx.notify();
    }

    fn test_run_row(
        &self,
        index: usize,
        step: &SubActionStep,
        palette: &ForgePalette,
    ) -> TestRunRow {
        let (fallback_icon, fallback_title, _) =
            super::editor::sub_action_summary(step, &self.select_options);
        let runner = self.sub_action_registry.get(&step.kind_id);
        let name = runner
            .map(crate::kind_copy::step_name)
            .unwrap_or(fallback_title);
        let (icon, color) = super::editor::step_glyph(
            &step.kind_id,
            runner.map(|r| r.icon_name()).unwrap_or(fallback_icon),
            runner.map(|r| super::editor::sub_category_color(r.category(), palette)),
            palette,
        );
        TestRunRow {
            index,
            name: name.into(),
            icon,
            color,
            status: RowStatus::Queued,
        }
    }

    fn on_test_run_event(
        &mut self,
        _modal: Entity<TestRunModal>,
        event: &TestRunEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TestRunEvent::Relaunch {
                action_id,
                selected,
            } => self.launch_test_run(*action_id, *selected, cx),
            TestRunEvent::DetailChanged(id) => self.load_detail_for(*id, cx),
            TestRunEvent::Cancelled(id) => {
                self.test_run = None;
                self._test_run_sub = None;
                self.load_detail_for(*id, cx);
                cx.notify();
            }
            TestRunEvent::Failed(message) => {
                self.test_run = None;
                self._test_run_sub = None;
                cx.push_toast(
                    ToastKind::Error,
                    tr!("action_editor_test_failed", error = message.as_str()),
                );
                cx.notify();
            }
        }
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
            let engine = spawn_action_engine(
                Arc::clone(&bus),
                Catalog::from_provider(provider.as_ref()),
                provider.action_repo(),
                provider.history_repo(),
                Arc::new(owned_sub_actions(&[(OBS_SCENE, "obs")])),
                Arc::new(ActionCancelRegistry::new()),
            );
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
                    crate::user_text::resolve_user_text(&UserText::from(
                        integration_disabled_message(&obs)
                    ))
                    .inline()
                )
            )
        );
    }
}
