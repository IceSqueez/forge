use super::*;
use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS, FONT_XXS,
    MenuItem, Radius, Spacing, context_menu, ghost_button_with_icon, menu_header, menu_item, modal,
    primary_button, radius, spacing,
};
use forge_events::{Event, EventsError};
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

impl TestRunModal {
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
}

fn done_step_index(event: &Event) -> Option<usize> {
    let raw = event.payload.get("step_index")?.as_u64()?;
    if raw == usize::MAX as u64 {
        return None;
    }
    Some(raw as usize)
}

fn row_status_from_done(event: &Event) -> RowStatus {
    let ms = event
        .payload
        .get("duration_ms")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let message = event
        .payload
        .get("message")
        .and_then(|v| v.as_str())
        .map(SharedString::from);
    match event.payload.get("outcome").and_then(|v| v.as_str()) {
        Some("failed") => RowStatus::Failed {
            message: message.unwrap_or_else(|| tr!("action_editor_test_run_default_error").into()),
        },
        Some("skipped") => RowStatus::Skipped,
        _ => RowStatus::Ok { ms },
    }
}

enum FireOutcome {
    StartTimeout,
    Repaint,
    Error(String),
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
            let bypass_pause = action.bypass_pause;

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
                    let kind_label = self
                        .trigger_registry
                        .get(&inst.kind_id)
                        .map(|desc| desc.label().to_owned())
                        .unwrap_or_else(|| inst.kind_id.clone());
                    TestTriggerChoice {
                        name: inst.name.clone().into(),
                        kind_label: kind_label.into(),
                    }
                })
                .collect();

            let selected_inst = selected.and_then(|i| detail.trigger_instances.get(i));
            let trigger_kind = selected_inst.map(|inst| inst.kind_id.clone());
            let (initial_args, note) = match selected_inst {
                Some(inst) => match self
                    .trigger_registry
                    .get(&inst.kind_id)
                    .and_then(|desc| desc.output_schema())
                {
                    Some(schema) => (super::test_trigger::synthesize_args(&schema), None),
                    None => (ArgStack::new(), Some(TestRunNote::NoSchema)),
                },
                None => (ArgStack::new(), Some(TestRunNote::NoTriggers)),
            };

            (
                action_name,
                queue_id,
                bypass_pause,
                rows,
                triggers,
                trigger_kind,
                initial_args,
                note,
            )
        };
        let (action_name, queue_id, bypass_pause, rows, triggers, trigger_kind, initial_args, note) =
            prepared;

        let subscription = self.bus.subscribe();
        let bridge = cx.spawn(async move |this, cx| {
            let mut sub = subscription;
            loop {
                match sub.recv().await {
                    Ok(event) => match this.update(cx, |this, cx| this.on_test_event(&event, cx)) {
                        Ok(true) | Err(_) => break,
                        Ok(false) => {}
                    },
                    Err(EventsError::LaggingReceiver) => continue,
                    Err(_) => break,
                }
            }
        });

        let scheduler = self.scheduler.clone();
        let bus = Arc::clone(&self.bus);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(
                super::test_trigger::dispatch_test_run(
                    &scheduler,
                    &bus,
                    id,
                    queue_id,
                    bypass_pause,
                    trigger_kind,
                    initial_args,
                )
                .await,
            );
        });
        let fire = cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.on_test_fired(id, result, cx));
            }
        });

        self.test_run = Some(TestRunModal {
            action_id: id,
            action_name,
            rows,
            triggers,
            selected_trigger: selected,
            note,
            trigger_menu: None,
            root: None,
            top_run_ids: HashMap::new(),
            phase: TestRunPhase::Awaiting,
            _bridge: bridge,
            _fire: fire,
            _timeout: None,
        });
        cx.notify();
    }

    fn test_run_row(
        &self,
        index: usize,
        step: &SubActionStep,
        palette: &ForgePalette,
    ) -> TestRunRow {
        let (fallback_icon, fallback_title, _) = super::editor::sub_action_summary(step);
        let runner = self.sub_action_registry.get(&step.kind_id);
        let name = runner
            .map(|r| r.label().to_owned())
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

    fn run_test_again(&mut self, cx: &mut Context<Self>) {
        let Some((id, selected)) = self
            .test_run
            .as_ref()
            .map(|run| (run.action_id, run.selected_trigger))
        else {
            return;
        };
        self.launch_test_run(id, selected, cx);
    }

    fn pick_test_trigger(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(id) = self.test_run.as_ref().map(|run| run.action_id) else {
            return;
        };
        self.launch_test_run(id, Some(index), cx);
    }

    fn toggle_trigger_menu(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(run) = self.test_run.as_mut() {
            run.trigger_menu = if run.trigger_menu.is_some() {
                None
            } else {
                Some(pos)
            };
            cx.notify();
        }
    }

    fn close_trigger_menu(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = self.test_run.as_mut() {
            run.trigger_menu = None;
            cx.notify();
        }
    }

    fn cancel_test_run(&mut self, cx: &mut Context<Self>) {
        let refresh_id = self.test_run.as_ref().map(|run| run.action_id);
        self.test_run = None;
        if let Some(id) = refresh_id {
            self.load_detail_for(id, cx);
        }
        cx.notify();
    }

    fn on_test_fired(&mut self, id: ActionId, result: Result<(), String>, cx: &mut Context<Self>) {
        let outcome = {
            let Some(run) = self.test_run.as_ref() else {
                return;
            };
            if run.action_id != id {
                return;
            }
            match result {
                Ok(()) => {
                    if matches!(run.phase, TestRunPhase::Awaiting) {
                        FireOutcome::StartTimeout
                    } else {
                        FireOutcome::Repaint
                    }
                }
                Err(message) => FireOutcome::Error(message),
            }
        };
        match outcome {
            FireOutcome::StartTimeout => self.start_test_timeout(cx),
            FireOutcome::Repaint => cx.notify(),
            FireOutcome::Error(message) => {
                self.test_run = None;
                cx.push_toast(
                    ToastKind::Error,
                    tr!("action_editor_test_failed", error = message.as_str()),
                );
                cx.notify();
            }
        }
    }

    fn start_test_timeout(&mut self, cx: &mut Context<Self>) {
        let timeout = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TRIGGER_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| this.on_test_timeout(cx));
        });
        if let Some(run) = self.test_run.as_mut() {
            run._timeout = Some(timeout);
        }
        cx.notify();
    }

    fn on_test_timeout(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = self.test_run.as_mut()
            && matches!(run.phase, TestRunPhase::Awaiting)
        {
            run.phase = TestRunPhase::NotStarted;
            cx.notify();
        }
    }

    fn on_test_event(&mut self, event: &Event, cx: &mut Context<Self>) -> bool {
        let mut refresh_after: Option<ActionId> = None;
        let handled = {
            let Some(run) = self.test_run.as_mut() else {
                return true;
            };
            match event.kind.as_str() {
                "action.start" => {
                    if run.root.is_some() {
                        return false;
                    }
                    let ours = event.payload.get("action_id").and_then(|v| v.as_str())
                        == Some(run.action_id.to_string().as_str());
                    if !ours {
                        return false;
                    }
                    run.root = Some(event.id);
                    if !run.phase.is_terminal() || matches!(run.phase, TestRunPhase::NotStarted) {
                        run.phase = TestRunPhase::Running;
                    }
                    cx.notify();
                    false
                }
                "action.skipped" => {
                    let ours = event.payload.get("action_id").and_then(|v| v.as_str())
                        == Some(run.action_id.to_string().as_str());
                    if ours && matches!(run.phase, TestRunPhase::Awaiting) {
                        run.phase = TestRunPhase::NotStarted;
                        cx.notify();
                        return true;
                    }
                    false
                }
                "subaction.run" => {
                    let Some(root) = run.root else {
                        return false;
                    };
                    if event.caused_by != Some(root) {
                        return false;
                    }
                    if let Some(index) = done_step_index(event) {
                        run.top_run_ids.insert(event.id, index);
                        if let Some(row) = run.rows.get_mut(index) {
                            row.status = RowStatus::Running;
                        }
                        cx.notify();
                    }
                    false
                }
                "subaction.done" => {
                    let Some(root) = run.root else {
                        return false;
                    };
                    let is_disabled_top = event.caused_by == Some(root);
                    let is_executed_top = event
                        .caused_by
                        .is_some_and(|c| run.top_run_ids.contains_key(&c));
                    if !is_disabled_top && !is_executed_top {
                        return false;
                    }
                    if let Some(index) = done_step_index(event)
                        && let Some(row) = run.rows.get_mut(index)
                    {
                        row.status = row_status_from_done(event);
                        cx.notify();
                    }
                    false
                }
                "action.done" => {
                    if event.caused_by != run.root {
                        return false;
                    }
                    let failed =
                        event.payload.get("outcome").and_then(|v| v.as_str()) == Some("failed");
                    if failed {
                        let step = run
                            .rows
                            .iter()
                            .rposition(|r| matches!(r.status, RowStatus::Failed { .. }))
                            .unwrap_or(run.rows.len().saturating_sub(1));
                        run.phase = TestRunPhase::Halted { step };
                    } else {
                        run.phase = TestRunPhase::Done {
                            errors: run.errors(),
                        };
                    }
                    refresh_after = Some(run.action_id);
                    cx.notify();
                    true
                }
                _ => false,
            }
        };
        if let Some(id) = refresh_after {
            self.load_detail_for(id, cx);
        }
        handled
    }

    pub(super) fn render_test_run_modal(
        &self,
        run: &TestRunModal,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let halted = matches!(run.phase, TestRunPhase::Halted { .. });
        let header_tint = if halted {
            palette.random
        } else {
            palette.success
        };

        let subtitle = match run.selected_trigger.and_then(|i| run.triggers.get(i)) {
            Some(choice) => tr!(
                "action_editor_test_run_subtitle_trigger",
                name = choice.kind_label.as_ref()
            ),
            None => tr!("action_editor_test_run_subtitle_none"),
        };

        let body = self.render_test_run_body(run, palette, cx);
        let footer = self.render_test_run_footer(run, palette, cx);

        let card = modal(
            tr!(
                "action_editor_test_run_title",
                name = run.action_name.as_ref()
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
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_test_run(cx)),
        );

        let view = cx.entity();
        let card_overlay = overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-test-run-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_test_run(cx));
            });

        let menu = run
            .trigger_menu
            .map(|pos| self.render_trigger_menu(run, pos, palette, cx));

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(card_overlay)
            .children(menu)
            .into_any_element()
    }

    fn render_trigger_selector(
        &self,
        run: &TestRunModal,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label: SharedString = run
            .selected_trigger
            .and_then(|i| run.triggers.get(i))
            .map(|choice| choice.kind_label.clone())
            .unwrap_or_else(|| tr!("action_editor_test_run_subtitle_none").into());
        let hover = palette.surface_overlay;

        div()
            .id("actions-test-trigger-select")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .py(SELECT_PAD_V)
            .px(spacing(Spacing::Sm, Density::Cozy))
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
                    .font_family(DEFAULT_BODY_FAMILY)
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
        run: &TestRunModal,
        pos: Point<Pixels>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let mut items: Vec<MenuItem> =
            vec![menu_header(tr!("action_editor_test_run_trigger_pick"))];
        for (i, choice) in run.triggers.iter().enumerate() {
            let mut entry = menu_item(
                SharedString::from(format!("actions-test-trigger-{i}")),
                format!("{} · {}", choice.name, choice.kind_label),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_test_trigger(i, cx)),
            )
            .icon(Icon::Bolt);
            if run.selected_trigger == Some(i) {
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

    fn render_test_run_body(
        &self,
        run: &TestRunModal,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let progress_color = match run.phase {
            TestRunPhase::Halted { .. } => palette.random,
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
                    .w(relative(run.progress_fraction()))
                    .bg(progress_color),
            );

        let rows: AnyElement = if run.rows.is_empty() {
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .p(EMPTY_PAD)
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(tr!("action_editor_test_run_empty"))
                .into_any_element()
        } else {
            let mut col = div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, Density::Cozy));
            for row in &run.rows {
                col = col.child(self.render_test_run_row(row, palette));
            }
            col.into_any_element()
        };

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy));
        if run.triggers.len() > 1 {
            body = body.child(self.render_trigger_selector(run, palette, cx));
        }
        body = body.child(progress).child(rows);
        if let Some(note) = &run.note {
            let text = match note {
                TestRunNote::NoSchema => tr!("action_editor_test_run_note_no_schema"),
                TestRunNote::NoTriggers => tr!("action_editor_test_run_note_no_triggers"),
            };
            body = body.child(
                div()
                    .w_full()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(text),
            );
        }
        if let Some(banner) = self.render_test_run_banner(run, palette) {
            body = body.child(banner);
        }
        body.into_any_element()
    }

    fn render_test_run_row(&self, row: &TestRunRow, palette: &ForgePalette) -> AnyElement {
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
                    0.55,
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
                    palette.random,
                    palette.shell,
                    tr!("action_editor_test_run_status_failed"),
                    palette.random,
                    palette.random,
                    Rgba {
                        a: FAIL_TINT_ALPHA,
                        ..palette.random
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
                    0.55,
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
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(mark_fg)
                    .child(mark_glyph),
            );

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .py(ROW_PAD_V)
            .px(ROW_PAD_H)
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
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(row.name.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(label_color)
                    .child(label),
            )
            .into_any_element()
    }

    fn render_test_run_banner(
        &self,
        run: &TestRunModal,
        palette: &ForgePalette,
    ) -> Option<AnyElement> {
        match &run.phase {
            TestRunPhase::Halted { step } => {
                let row = run.rows.get(*step);
                let name = row
                    .map(|r| r.name.clone())
                    .unwrap_or_else(|| SharedString::from(""));
                let message = row
                    .and_then(|r| match &r.status {
                        RowStatus::Failed { message, .. } => Some(message.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| tr!("action_editor_test_run_default_error").into());
                Some(self.test_run_banner(
                    Icon::CircleX,
                    palette.random,
                    tr!(
                        "action_editor_test_run_failed_banner",
                        step = (*step as i64) + 1,
                        name = name.as_ref()
                    ),
                    Some(message),
                    palette,
                ))
            }
            TestRunPhase::Done { errors } if *errors > 0 => Some(self.test_run_banner(
                Icon::AlertTriangle,
                palette.warning,
                tr!(
                    "action_editor_test_run_completed",
                    count = run.rows.len() as i64,
                    errors = *errors as i64
                ),
                None,
                palette,
            )),
            TestRunPhase::Done { .. } if !run.rows.is_empty() => Some(self.test_run_banner(
                Icon::CircleCheck,
                palette.success,
                tr!(
                    "action_editor_test_run_completed",
                    count = run.rows.len() as i64,
                    errors = 0
                ),
                None,
                palette,
            )),
            TestRunPhase::NotStarted => Some(self.test_run_banner(
                Icon::InfoCircle,
                palette.text_muted,
                tr!("action_editor_test_run_notstarted"),
                None,
                palette,
            )),
            _ => None,
        }
    }

    fn test_run_banner(
        &self,
        glyph: Icon,
        tint: Rgba,
        title: String,
        message: Option<SharedString>,
        palette: &ForgePalette,
    ) -> AnyElement {
        let heading = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(icon(glyph, BANNER_ICON, tint))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(tint)
                    .child(title),
            );

        let mut banner = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .py(spacing(Spacing::Sm, Density::Cozy))
            .px(spacing(Spacing::Sm, Density::Cozy))
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
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_secondary)
                    .child(message),
            );
        }
        banner.into_any_element()
    }

    fn render_test_run_footer(
        &self,
        run: &TestRunModal,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status = match run.phase {
            TestRunPhase::Halted { .. } => tr!("action_editor_test_run_foot_halted"),
            TestRunPhase::Done { .. } => tr!("action_editor_test_run_foot_finished"),
            TestRunPhase::NotStarted => tr!("action_editor_test_run_foot_notstarted"),
            _ => tr!("action_editor_test_run_foot_simulating"),
        };

        let mut actions = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy));
        if run.phase.is_terminal() {
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
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_test_run(cx)),
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
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(status),
            )
            .child(actions)
            .into_any_element()
    }
}
