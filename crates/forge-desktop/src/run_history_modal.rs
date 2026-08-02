use crate::actions_screen::parse_variable_segments;
use crate::presentation::ActivePresentation;
use forge_components::{
    Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, ModalSize, OverlayPosition, Radius,
    Spacing, body_family, fmt_relative_time, icon, json_highlighted, modal, mono_family, overlay,
    radius, spacing, status_dot, tr,
};
use forge_registry::{TriggerKindDescriptor, TriggerRegistry};
use forge_types::{
    ExecutionContext, ExecutionMetadata, ExecutionOutcome, SubActionOutcome, SubActionTelemetry,
};
use gpui::{
    AnyElement, ClickEvent, Context, EventEmitter, Pixels, Rgba, SharedString, Window, div,
    prelude::*, px,
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

pub struct RunHistoryDismissed;

pub struct RunHistoryModal {
    subtitle: SharedString,
    trigger_registry: Arc<TriggerRegistry>,
    runs: Option<Vec<ExecutionContext>>,
    selected: usize,
}

impl EventEmitter<RunHistoryDismissed> for RunHistoryModal {}

impl RunHistoryModal {
    pub fn new(subtitle: impl Into<SharedString>, trigger_registry: Arc<TriggerRegistry>) -> Self {
        Self {
            subtitle: subtitle.into(),
            trigger_registry,
            runs: None,
            selected: 0,
        }
    }

    pub fn set_runs(&mut self, runs: Vec<ExecutionContext>, cx: &mut Context<Self>) {
        self.runs = Some(runs);
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
            .child(self.render_detail(&runs[selected], palette));

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

    fn render_detail(&self, ctx: &ExecutionContext, palette: &ForgePalette) -> AnyElement {
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

        let badge = div()
            .flex_shrink_0()
            .py(px(1.0))
            .px(px(6.0))
            .rounded(CHIP_RADIUS)
            .bg(palette.surface_overlay)
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(badge_color)
            .child(badge_label);

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

        let step_failed = ctx
            .telemetry
            .iter()
            .any(|step| matches!(step.outcome, SubActionOutcome::Failed(_)));

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
            for step in &ctx.telemetry {
                steps = steps.child(self.render_telemetry_row(step, palette));
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
                    .child(json_highlighted(json, palette)),
            );
        }

        section.into_any_element()
    }

    fn render_telemetry_row(
        &self,
        step: &SubActionTelemetry,
        palette: &ForgePalette,
    ) -> AnyElement {
        let nested = step.is_nested();
        let (status_color, status_label, message) = match &step.outcome {
            SubActionOutcome::Success => (
                palette.success,
                tr!("action_editor_run_history_step_ok"),
                None,
            ),
            SubActionOutcome::Failed(message) => (
                palette.random,
                tr!("action_editor_run_history_step_failed"),
                Some(message.clone()),
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
            .child(
                div()
                    .flex_shrink_0()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(status_color)
                    .child(status_label),
            );

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
                    .child(json_highlighted(value.to_owned(), palette)),
            )
            .into_any_element()
    }
}

impl Render for RunHistoryModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let palette = &palette;

        let has_runs = matches!(&self.runs, Some(runs) if !runs.is_empty());
        let body = match &self.runs {
            None => self.render_loading(palette),
            Some(runs) if runs.is_empty() => self.render_empty(palette),
            Some(runs) => self.render_master_detail(runs, self.selected, palette, cx),
        };

        let mut card = modal(tr!("action_editor_run_history_title"), body, palette)
            .size(ModalSize::Lg)
            .header_icon(Icon::History, palette.brand)
            .subtitle(self.subtitle.clone())
            .kbd_hint(tr!("actions_esc_hint"))
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
