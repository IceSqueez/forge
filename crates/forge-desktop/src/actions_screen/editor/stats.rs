use super::*;
use forge_components::{FONT_SM, FONT_XXS, ForgePalette, Radius, mono_family, radius, tr};
use forge_types::ExecutionOutcome;
use gpui::{AnyElement, ClickEvent, Context, Rgba, SharedString, div};

impl ScreenActionsView {
    pub(super) fn render_stats_row(
        &self,
        telemetry: &ActionTelemetry,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let last_fired = fmt_relative_time(telemetry.last_fired_at);
        let runs = fmt_number(telemetry.runs_today as f64, 0);
        let avg = match telemetry.avg_duration_ms {
            Some(ms) => tr!("action_stat_avg_ms", count = ms as i64),
            None => tr!("action_stat_avg_none"),
        };
        let errors = telemetry.errors_7d;
        let error_color = if errors > 0 {
            palette.random
        } else {
            palette.success
        };

        let (exec_value, exec_color): (SharedString, Rgba) = match &self.last_outcome {
            Some(ExecutionOutcome::Failed(_)) => (
                tr!("action_editor_run_history_outcome_failed").into(),
                palette.random,
            ),
            Some(ExecutionOutcome::Cancelled) => ("-".into(), palette.text_muted),
            Some(ExecutionOutcome::Success) | None => ("0".into(), palette.text_primary),
        };

        div()
            .flex()
            .gap(STAT_GAP)
            .py(STAT_PAD_V)
            .px(CARD_PAD_H)
            .rounded(radius(Radius::Md))
            .bg(palette.base)
            .border(HALF_BORDER)
            .border_color(palette.border_regular)
            .child(self.render_stat_cell(
                tr!("action_stat_last_fired"),
                last_fired,
                palette.text_primary,
                palette,
            ))
            .child(self.render_history_link_cell(
                "actions-runs-history-link",
                tr!("action_stat_runs_today"),
                runs,
                palette.brand,
                palette,
                cx,
            ))
            .child(self.render_stat_cell(
                tr!("action_stat_avg_time"),
                avg,
                palette.success,
                palette,
            ))
            .child(self.render_history_link_cell(
                "actions-execution-history-link",
                tr!("action_stat_execution"),
                exec_value,
                exec_color,
                palette,
                cx,
            ))
            .child(self.render_history_link_cell(
                "actions-errors-history-link",
                tr!("action_stat_errors_7d"),
                errors.to_string(),
                error_color,
                palette,
                cx,
            ))
            .into_any_element()
    }

    fn render_stat_cell(
        &self,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
        value_color: Rgba,
        palette: &ForgePalette,
    ) -> AnyElement {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(STAT_VALUE_GAP)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(label.into()),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_SM)
                    .text_color(value_color)
                    .child(value.into()),
            )
            .into_any_element()
    }

    fn render_history_link_cell(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
        value_color: Rgba,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_start()
            .gap(STAT_VALUE_GAP)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(label.into()),
            )
            .child(
                div()
                    .id(id)
                    .cursor_pointer()
                    .font_family(mono_family())
                    .text_size(FONT_SM)
                    .text_color(value_color)
                    .underline()
                    .text_decoration_1()
                    .text_decoration_color(palette.border_input)
                    .child(value.into())
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.open_history_modal(cx)),
                    ),
            )
            .into_any_element()
    }
}
