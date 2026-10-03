use crate::actions_screen::parse_variable_segments;
use crate::integration_switch::integration_name;
use crate::presentation::ActivePresentation;
use forge_components::highlight::Language;
use forge_components::{
    Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, ModalSize, OverlayPosition, Radius,
    Spacing, body_family, error_row, fmt_relative_time, highlighted_text, icon,
    integration_disabled_reason, modal, mono_family, overlay, radius, spacing, status_dot,
    tooltip_builder, tr,
};
use forge_registry::{TriggerKindDescriptor, TriggerRegistry};
use forge_types::{
    ExecutionContext, ExecutionMetadata, ExecutionOutcome, IntegrationId, SubActionOutcome,
    SubActionTelemetry,
};
use gpui::{
    AnyElement, ClickEvent, Context, ElementId, EventEmitter, Pixels, Rgba, SharedString, Window,
    div, prelude::*, px,
};
use std::sync::Arc;

const MODAL_W: Pixels = px(880.0);
const BODY_H: Pixels = px(560.0);
const RAIL_W: Pixels = px(220.0);
const RAIL_ENTRY_GAP: Pixels = px(8.0);
const RAIL_ENTRY_PAD_V: Pixels = px(6.0);
const RAIL_ENTRY_PAD_H: Pixels = px(8.0);
const ROW_DOT: Pixels = px(7.0);
const STEP_DOT: Pixels = px(5.0);
const STEP_NEST_INDENT: Pixels = px(14.0);
const EMPTY_GLYPH: Pixels = px(26.0);
const CHIP_RADIUS: Pixels = px(6.0);
const HALF_BORDER: Pixels = px(0.5);
const RAIL_DISABLED_GLYPH: Pixels = px(10.0);

pub struct RunHistoryDismissed;

pub struct RunHistoryOpenIntegration(pub IntegrationId);

pub fn disabled_integration_of(ctx: &ExecutionContext) -> Option<&IntegrationId> {
    ctx.telemetry.iter().find_map(|step| match &step.outcome {
        SubActionOutcome::IntegrationDisabled(integration) => Some(integration),
        _ => None,
    })
}

fn disabled_reason_link(
    element_id: ElementId,
    integration: &IntegrationId,
    palette: &ForgePalette,
    cx: &mut Context<RunHistoryModal>,
) -> AnyElement {
    let target = integration.clone();
    div()
        .id(element_id)
        .flex_none()
        .cursor_pointer()
        .child(integration_disabled_reason(
            tr!(
                "action_editor_run_history_integration_disabled",
                name = integration_name(integration).to_string()
            ),
            palette,
        ))
        .tooltip(tooltip_builder(
            tr!(
                "action_editor_run_history_open_integration",
                name = integration_name(integration).to_string()
            ),
            palette,
        ))
        .on_click(
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.open_integration(target.clone(), cx)
            }),
        )
        .into_any_element()
}

enum Load {
    Pending,
    Failed(SharedString),
    Ready(Vec<ExecutionContext>),
}

pub struct RunHistoryModal {
    subtitle: SharedString,
    trigger_registry: Arc<TriggerRegistry>,
    load: Load,
    selected: usize,
}

impl EventEmitter<RunHistoryDismissed> for RunHistoryModal {}

impl EventEmitter<RunHistoryOpenIntegration> for RunHistoryModal {}

impl RunHistoryModal {
    pub fn new(subtitle: impl Into<SharedString>, trigger_registry: Arc<TriggerRegistry>) -> Self {
        Self {
            subtitle: subtitle.into(),
            trigger_registry,
            load: Load::Pending,
            selected: 0,
        }
    }

    pub fn set_runs(&mut self, runs: Vec<ExecutionContext>, cx: &mut Context<Self>) {
        self.load = Load::Ready(runs);
        self.selected = 0;
        cx.notify();
    }

    pub fn set_error(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.load = Load::Failed(message.into());
        self.selected = 0;
        cx.notify();
    }

    fn select_run(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = index;
        cx.notify();
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(RunHistoryDismissed);
    }

    pub fn open_integration(&mut self, integration: IntegrationId, cx: &mut Context<Self>) {
        cx.emit(RunHistoryOpenIntegration(integration));
    }

    fn render_loading(&self, palette: &ForgePalette) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .py(spacing(Spacing::Lg, Density::Cozy))
            .font_family(body_family())
            .text_size(FONT_SM)
            .text_color(palette.text_muted)
            .child(tr!("action_editor_run_history_loading"))
            .into_any_element()
    }

    fn render_failed(&self, message: &SharedString, palette: &ForgePalette) -> AnyElement {
        div()
            .w_full()
            .p(spacing(Spacing::Md, Density::Cozy))
            .child(error_row(message.clone(), palette))
            .into_any_element()
    }

    fn render_empty(&self, palette: &ForgePalette) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .py(spacing(Spacing::Lg, Density::Cozy))
            .child(icon(Icon::History, EMPTY_GLYPH, palette.text_faint))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_secondary)
                    .child(tr!("action_editor_run_history_empty_title")),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("action_editor_run_history_empty_hint")),
            )
            .into_any_element()
    }

    fn render_master_detail(
        &self,
        runs: &[ExecutionContext],
        selected: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = selected.min(runs.len().saturating_sub(1));

        let mut rail = div()
            .id("actions-history-rail")
            .flex_none()
            .w(RAIL_W)
            .h_full()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .pr(spacing(Spacing::Sm, Density::Cozy))
            .overflow_y_scroll()
            .border_r(HALF_BORDER)
            .border_color(palette.border_regular);
        for (index, ctx) in runs.iter().enumerate() {
            rail = rail.child(self.render_rail_entry(index, ctx, index == selected, palette, cx));
        }

        let detail = div()
            .id("actions-history-detail")
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .overflow_y_scroll()
            .pl(spacing(Spacing::Md, Density::Cozy))
            .child(self.render_detail(&runs[selected], palette, cx));

        div()
            .w_full()
            .h(BODY_H)
            .flex()
            .child(rail)
            .child(detail)
            .into_any_element()
    }

    fn render_rail_entry(
        &self,
        index: usize,
        ctx: &ExecutionContext,
        active: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dot_color = match &ctx.outcome {
            ExecutionOutcome::Success => palette.success,
            ExecutionOutcome::Failed(_) => palette.random,
            ExecutionOutcome::Cancelled => palette.text_muted,
        };
        let when = fmt_relative_time(Some(ctx.started_at));
        let duration = match ctx.completed_at {
            Some(done) => {
                let ms = (done - ctx.started_at).whole_milliseconds().max(0);
                tr!("action_editor_run_history_duration_ms", count = ms as i64)
            }
            None => "-".to_owned(),
        };
        let (bg, time_color) = if active {
            (palette.surface_overlay, palette.text_primary)
        } else {
            (gpui::transparent_black().into(), palette.text_secondary)
        };
        let hover_bg = palette.surface_overlay;

        div()
            .id(SharedString::from(format!("actions-history-rail-{index}")))
            .flex()
            .items_center()
            .gap(RAIL_ENTRY_GAP)
            .w_full()
            .py(RAIL_ENTRY_PAD_V)
            .px(RAIL_ENTRY_PAD_H)
            .rounded(radius(Radius::Sm))
            .bg(bg)
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_run(index, cx)))
            .child(status_dot(dot_color, ROW_DOT))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(time_color)
                    .child(when),
            )
            .children(
                disabled_integration_of(ctx)
                    .map(|_| icon(Icon::PlugOff, RAIL_DISABLED_GLYPH, palette.random)),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(duration),
            )
            .into_any_element()
    }

    fn render_detail(
        &self,
        ctx: &ExecutionContext,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let when = fmt_relative_time(Some(ctx.started_at));
        let duration = match ctx.completed_at {
            Some(done) => {
                let ms = (done - ctx.started_at).whole_milliseconds().max(0);
                tr!("action_editor_run_history_duration_ms", count = ms as i64)
            }
            None => "-".to_owned(),
        };
        let (badge_color, badge_label, error_message) = match &ctx.outcome {
            ExecutionOutcome::Success => (
                palette.success,
                tr!("action_editor_run_history_outcome_success"),
                None,
            ),
            ExecutionOutcome::Failed(message) => (
                palette.random,
                tr!("action_editor_run_history_outcome_failed"),
                Some(message.clone()),
            ),
            ExecutionOutcome::Cancelled => (
                palette.text_muted,
                tr!("action_editor_run_history_outcome_cancelled"),
                None,
            ),
        };

        let badge = match disabled_integration_of(ctx) {
            Some(integration) => disabled_reason_link(
                ElementId::Name("run-history-disabled-run".into()),
                integration,
                palette,
                cx,
            ),
            None => div()
                .flex_shrink_0()
                .py(px(1.0))
                .px(px(6.0))
                .rounded(CHIP_RADIUS)
                .bg(palette.surface_overlay)
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(badge_color)
                .child(badge_label)
                .into_any_element(),
        };

        let top = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(status_dot(badge_color, ROW_DOT))
            .child(
                div()
                    .flex_1()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(when),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(duration),
            )
            .child(badge);

        let step_failed = ctx.telemetry.iter().any(|step| step.outcome.is_failure());

        let mut col = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(top)
            .child(self.render_run_trigger(ctx, palette));
        if let Some(message) = error_message
            && !step_failed
        {
            col = col.child(
                div()
                    .pl(ROW_DOT + spacing(Spacing::Xs, Density::Cozy))
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.random)
                    .child(message),
            );
        }
        if !ctx.telemetry.is_empty() {
            let mut steps = div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xxs, Density::Cozy))
                .pt(spacing(Spacing::Xs, Density::Cozy))
                .mt(spacing(Spacing::Xs, Density::Cozy))
                .pl(ROW_DOT + spacing(Spacing::Xs, Density::Cozy))
                .border_t(HALF_BORDER)
                .border_color(palette.border_regular);
            for (index, step) in ctx.telemetry.iter().enumerate() {
                steps = steps.child(self.render_telemetry_row(index, step, palette, cx));
            }
            col = col.child(steps);
        }
        col.into_any_element()
    }

    fn render_run_trigger(&self, ctx: &ExecutionContext, palette: &ForgePalette) -> AnyElement {
        let (glyph, label): (Icon, SharedString) = match &ctx.metadata {
            ExecutionMetadata::Trigger { trigger_kind, .. } => {
                let descriptor = trigger_kind
                    .as_deref()
                    .and_then(|kind| self.trigger_registry.get(kind));
                let glyph = Icon::from_name(
                    descriptor
                        .map(TriggerKindDescriptor::icon_name)
                        .unwrap_or("bolt"),
                );
                let label = descriptor
                    .map(|d| SharedString::from(d.label().to_owned()))
                    .unwrap_or_else(|| tr!("action_editor_run_history_trigger_fallback").into());
                (glyph, label)
            }
            ExecutionMetadata::QuickAction { label, .. } => (
                Icon::from_name("layout-grid"),
                SharedString::from(label.clone()),
            ),
        };

        let header = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(icon(glyph, FONT_XS, palette.text_muted))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(label),
            );

        let mut section = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .pl(ROW_DOT + spacing(Spacing::Xs, Density::Cozy))
            .child(header);

        if !ctx.arg_stack_snapshot.is_empty() {
            let mut object = serde_json::Map::new();
            for (name, value) in &ctx.arg_stack_snapshot {
                object.insert(name.clone(), value.to_plain_json());
            }
            let json = serde_json::to_string_pretty(&serde_json::Value::Object(object))
                .unwrap_or_default();
            section = section.child(
                div()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .pl(spacing(Spacing::Xs, Density::Cozy))
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(highlighted_text(Language::Json, json, palette)),
            );
        }

        section.into_any_element()
    }

    fn render_telemetry_row(
        &self,
        index: usize,
        step: &SubActionTelemetry,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let nested = step.is_nested();
        let (status_color, status_label, message) = match &step.outcome {
            SubActionOutcome::Success => (
                palette.success,
                tr!("action_editor_run_history_step_ok"),
                None,
            ),
            SubActionOutcome::Failed(_) => (
                palette.random,
                tr!("action_editor_run_history_step_failed"),
                step.outcome.failure_reason(),
            ),
            SubActionOutcome::IntegrationDisabled(_) => (
                palette.random,
                tr!("action_editor_run_history_step_failed"),
                None,
            ),
            SubActionOutcome::Skipped(message) => (
                palette.text_muted,
                tr!("action_editor_run_history_step_skipped"),
                Some(message.clone()),
            ),
        };

        let marker = if nested {
            tr!("action_editor_run_history_step_nested").to_string()
        } else {
            format!("#{}", step.index + 1)
        };

        let line = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(
                div()
                    .flex_shrink_0()
                    .w(spacing(Spacing::Lg, Density::Cozy))
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(marker),
            )
            .child(status_dot(status_color, STEP_DOT))
            .child(
                div()
                    .flex_1()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_secondary)
                    .child(step.kind.clone()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(tr!(
                        "action_editor_run_history_duration_ms",
                        count = step.duration_ms as i64
                    )),
            )
            .child(match &step.outcome {
                SubActionOutcome::IntegrationDisabled(integration) => disabled_reason_link(
                    ElementId::NamedInteger("run-history-disabled-step".into(), index as u64),
                    integration,
                    palette,
                    cx,
                ),
                _ => div()
                    .flex_shrink_0()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(status_color)
                    .child(status_label)
                    .into_any_element(),
            });

        let mut row = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy));
        if nested {
            row = row.pl(STEP_NEST_INDENT);
        }
        row = row.child(line);

        for (name, value) in &step.args_in {
            row = row.child(self.render_io_var(
                tr!("action_editor_run_history_step_args_in"),
                name,
                value,
                palette.text_muted,
                palette,
            ));
        }

        for (name, value) in &step.produced {
            row = row.child(self.render_io_var(
                tr!("action_editor_run_history_step_produced"),
                name,
                value,
                palette.warning,
                palette,
            ));
        }

        if let Some(text) = message {
            row = row.child(
                div()
                    .pl(spacing(Spacing::Lg, Density::Cozy) + spacing(Spacing::Xs, Density::Cozy))
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(status_color)
                    .child(text),
            );
        }
        row.into_any_element()
    }

    fn render_io_var(
        &self,
        tag: impl Into<SharedString>,
        name: &str,
        value: &str,
        name_color: Rgba,
        palette: &ForgePalette,
    ) -> AnyElement {
        let io_indent = spacing(Spacing::Lg, Density::Cozy) + spacing(Spacing::Xs, Density::Cozy);
        let value = if value.is_empty() { "\"\"" } else { value };
        let multiline = value.contains('\n')
            || matches!(
                serde_json::from_str::<serde_json::Value>(value),
                Ok(serde_json::Value::Array(_) | serde_json::Value::Object(_))
            );

        let mut head = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(
                div()
                    .flex_shrink_0()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(tag.into()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(name_color)
                    .child(name.to_owned()),
            );

        if !multiline {
            let is_string = value.len() >= 2 && value.starts_with('"') && value.ends_with('"');
            let mut value_el = div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_wrap()
                .font_family(mono_family())
                .text_size(FONT_XXS);
            if is_string {
                for (chunk, is_var) in parse_variable_segments(value) {
                    let color = if is_var {
                        palette.warning
                    } else {
                        palette.success
                    };
                    value_el = value_el.child(div().text_color(color).child(chunk.to_owned()));
                }
            } else {
                value_el = value_el
                    .text_color(palette.text_secondary)
                    .child(value.to_owned());
            }
            head = head.child(value_el);
            return div()
                .pl(io_indent)
                .min_w(px(0.))
                .overflow_hidden()
                .child(head)
                .into_any_element();
        }

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .pl(io_indent)
            .min_w(px(0.))
            .overflow_hidden()
            .child(head)
            .child(
                div()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .pl(spacing(Spacing::Xs, Density::Cozy))
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_secondary)
                    .child(highlighted_text(Language::Json, value.to_owned(), palette)),
            )
            .into_any_element()
    }
}

impl Render for RunHistoryModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let palette = &palette;

        let has_runs = matches!(&self.load, Load::Ready(runs) if !runs.is_empty());
        let body = match &self.load {
            Load::Pending => self.render_loading(palette),
            Load::Failed(message) => self.render_failed(message, palette),
            Load::Ready(runs) if runs.is_empty() => self.render_empty(palette),
            Load::Ready(runs) => self.render_master_detail(runs, self.selected, palette, cx),
        };

        let mut card = modal(tr!("action_editor_run_history_title"), body, palette)
            .size(ModalSize::Lg)
            .header_icon(Icon::History, palette.brand)
            .subtitle(self.subtitle.clone())
            .on_close(
                "actions-history-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.dismiss(cx)),
            );
        if has_runs {
            card = card.width(MODAL_W);
        }

        let view = cx.entity();
        div().absolute().top_0().left_0().size_full().child(
            overlay(card, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("actions-history-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.dismiss(cx));
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::{ActionId, EventId};
    use gpui::{Entity, Subscription, TestAppContext, size};
    use time::OffsetDateTime;

    use super::*;
    use crate::test_support::{ClickArea, click_grid, install_presentation};

    fn run(outcomes: Vec<SubActionOutcome>) -> ExecutionContext {
        ExecutionContext {
            action_id: ActionId::new(),
            metadata: ExecutionMetadata::Trigger {
                event_id: EventId::new(),
                trigger_kind: None,
            },
            arg_stack_snapshot: BTreeMap::new(),
            started_at: OffsetDateTime::UNIX_EPOCH,
            completed_at: None,
            telemetry: outcomes
                .into_iter()
                .enumerate()
                .map(|(index, outcome)| SubActionTelemetry {
                    index,
                    kind: "stub.step".to_owned(),
                    started_at: OffsetDateTime::UNIX_EPOCH,
                    duration_ms: 0,
                    outcome,
                    args_in: BTreeMap::new(),
                    produced: BTreeMap::new(),
                })
                .collect(),
            outcome: ExecutionOutcome::Success,
        }
    }

    #[test]
    fn a_run_names_the_first_integration_that_was_switched_off() {
        let ctx = run(vec![
            SubActionOutcome::Success,
            SubActionOutcome::IntegrationDisabled(IntegrationId::new("obs")),
            SubActionOutcome::IntegrationDisabled(IntegrationId::new("twitch")),
        ]);

        assert_eq!(
            disabled_integration_of(&ctx),
            Some(&IntegrationId::new("obs"))
        );
    }

    #[test]
    fn a_run_no_switched_off_integration_touched_names_none() {
        for (outcomes, case) in [
            (
                vec![
                    SubActionOutcome::Success,
                    SubActionOutcome::Failed("integration disabled: obs".to_owned()),
                    SubActionOutcome::Skipped("condition false".to_owned()),
                ],
                "failed, skipped and successful steps",
            ),
            (Vec::new(), "a run with no step telemetry"),
        ] {
            assert_eq!(disabled_integration_of(&run(outcomes)), None, "{case}");
        }
    }

    struct Opened {
        targets: Vec<IntegrationId>,
        _sub: Subscription,
    }

    const WINDOW_W: f32 = 1200.0;
    const WINDOW_H: f32 = 800.0;
    const DETAIL_BAND: std::ops::Range<f32> = 120.0..260.0;
    const SCAN_STEP_X: f32 = 12.0;
    const SCAN_STEP_Y: f32 = 6.0;

    #[gpui::test]
    fn clicking_the_switched_off_integration_asks_to_open_that_integration(
        cx: &mut TestAppContext,
    ) {
        install_presentation(cx);
        let (modal, vcx) = cx.add_window_view(|_, _| {
            RunHistoryModal::new("Scene", Arc::new(TriggerRegistry::new()))
        });
        let opened: Entity<Opened> = vcx.update(|_, cx| {
            cx.new(|cx| Opened {
                targets: Vec::new(),
                _sub: cx.subscribe(
                    &modal,
                    |opened: &mut Opened, _, event: &RunHistoryOpenIntegration, _| {
                        opened.targets.push(event.0.clone());
                    },
                ),
            })
        });
        modal.update(vcx, |modal, cx| {
            modal.set_runs(
                vec![run(vec![SubActionOutcome::IntegrationDisabled(
                    IntegrationId::new("obs"),
                )])],
                cx,
            )
        });
        vcx.simulate_resize(size(px(WINDOW_W), px(WINDOW_H)));
        vcx.run_until_parked();

        click_grid(
            vcx,
            ClickArea {
                x: 0.0..WINDOW_W,
                y: DETAIL_BAND,
                step_x: SCAN_STEP_X,
                step_y: SCAN_STEP_Y,
            },
        );

        let targets = opened.read_with(vcx, |opened, _| opened.targets.clone());
        assert!(
            !targets.is_empty(),
            "no click reached the switched-off badge"
        );
        assert!(
            targets.iter().all(|id| *id == IntegrationId::new("obs")),
            "{targets:?}"
        );
    }
}
