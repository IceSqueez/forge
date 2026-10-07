use super::cards::{add_row_button, empty_placeholder_card};
use super::step_presentation::variable_text;
use super::*;
use crate::integration_switch::integration_name;
use forge_components::{
    BORDER_THIN, Density, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, Spacing, body_family,
    health_tile, icon, mono_family, radius, row_card, spacing, tooltip_lines_builder, tr,
};
use forge_types::SubActionStep;
use gpui::{AnyElement, ClickEvent, Context, FontWeight, Rgba, SharedString, div, px};

fn analyzer_finding_message(finding: &analyzer::Finding) -> SharedString {
    let text = match finding {
        analyzer::Finding::UnknownVariable(name) => {
            tr!("action_editor_health_unknown_var", name = name.clone())
        }
        analyzer::Finding::ProducedLater(name) => {
            tr!("action_editor_health_produced_later", name = name.clone())
        }
        analyzer::Finding::IsolatedSibling(name) => {
            tr!("action_editor_health_isolated_sibling", name = name.clone())
        }
        analyzer::Finding::SomeTriggersOnly(name) => {
            tr!("action_editor_health_some_triggers", name = name.clone())
        }
        analyzer::Finding::LastRunFailed(message) => {
            tr!(
                "action_editor_health_last_run_failed",
                message = message.clone()
            )
        }
        analyzer::Finding::ControlFlowInConcurrentAction => {
            tr!("action_editor_health_control_flow_concurrent")
        }
        analyzer::Finding::IntegrationDisabled(owner) => {
            tr!(
                "action_editor_health_integration_disabled",
                name = integration_name(owner).to_string()
            )
        }
        analyzer::Finding::IntegrationFailed(owner) => {
            tr!(
                "action_editor_health_integration_failed",
                name = integration_name(owner).to_string()
            )
        }
    };
    SharedString::from(text)
}

impl ScreenActionsView {
    pub(super) fn render_sub_actions_section(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = nav::resolve_chain(&detail.action.sub_actions, &self.nav_path);
        let total = current.len();
        let at_root = self.nav_path.is_empty();
        let depth = self.nav_path.len();

        let header = if at_root {
            div().flex().items_center().child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(tr!("action_editor_sub_actions_count", count = total as i64)),
            )
        } else {
            div()
                .flex()
                .items_center()
                .child(self.render_breadcrumb(detail, palette, cx))
                .child(div().flex_1())
                .child(
                    div()
                        .font_family(mono_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.text_faint)
                        .child(total.to_string()),
                )
        };

        let mut steps_col = div().flex().flex_col();
        if current.is_empty() {
            let empty_label = if at_root {
                tr!("action_editor_no_steps")
            } else {
                tr!("action_editor_branch_empty")
            };
            steps_col = steps_col.child(empty_placeholder_card(
                Icon::Plus,
                palette.brand,
                empty_label,
                palette,
            ));
        }
        for (i, step) in current.iter().enumerate() {
            steps_col = steps_col.child(self.render_step_block(step, i, total, depth, palette, cx));
        }
        steps_col = steps_col.child(
            div()
                .pl(STEP_COL_W + spacing(Spacing::Xs, Density::Cozy))
                .pt(spacing(Spacing::Xs, Density::Cozy))
                .child(add_row_button(
                    "actions-add-step",
                    Icon::Plus,
                    tr!("action_editor_add_step"),
                    palette.brand,
                    palette,
                    cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.open_grid_picker(window, cx)
                    }),
                )),
        );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(header)
            .child(steps_col)
            .into_any_element()
    }

    fn render_health_dot(
        &self,
        health: &analyzer::StepHealth,
        i: usize,
        palette: &ForgePalette,
    ) -> AnyElement {
        let severity = health.severity();
        let color = match severity {
            analyzer::HealthSeverity::Green => palette.success,
            analyzer::HealthSeverity::Yellow => palette.warning,
            analyzer::HealthSeverity::Red => palette.random,
        };
        let gate_title = if health.disabled_integration().is_some() {
            Some(tr!("action_editor_health_disabled"))
        } else if health.failed_integration().is_some() {
            Some(tr!("action_editor_health_failed"))
        } else {
            None
        };
        let (glyph, color, title): (Icon, Rgba, SharedString) = match gate_title {
            Some(title) => (Icon::PlugOff, palette.warning, title.into()),
            None => (
                Icon::Heartbeat,
                color,
                match severity {
                    analyzer::HealthSeverity::Green => tr!("action_editor_health_ok").into(),
                    analyzer::HealthSeverity::Yellow => tr!("action_editor_health_warn").into(),
                    analyzer::HealthSeverity::Red => tr!("action_editor_health_error").into(),
                },
            ),
        };
        let tile = health_tile(glyph, color);
        let mut lines: Vec<SharedString> = vec![title];
        lines.extend(health.findings.iter().map(analyzer_finding_message));
        div()
            .id(SharedString::from(format!("actions-step-health-{i}")))
            .flex_none()
            .child(tile)
            .tooltip(tooltip_lines_builder(lines, palette))
            .into_any_element()
    }

    fn render_step_block(
        &self,
        step: &SubActionStep,
        i: usize,
        total: usize,
        depth: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_last = i + 1 == total;
        let (fallback_icon, fallback_title, detail_opt) = sub_action_summary(step);
        let runner = self.sub_action_registry.get(&step.kind_id);
        let title = runner
            .map(|r| r.label().to_owned())
            .unwrap_or(fallback_title);
        let (glyph, glyph_color) = step_glyph(
            &step.kind_id,
            runner.map(|r| r.icon_name()).unwrap_or(fallback_icon),
            runner.map(|r| sub_category_color(r.category(), palette)),
            palette,
        );
        let detail_str = detail_opt
            .or_else(|| runner.map(|r| r.summary().to_owned()))
            .unwrap_or_else(|| step.kind_id.clone());

        let circle = div()
            .flex()
            .items_center()
            .justify_center()
            .size(STEP_CIRCLE)
            .rounded(STEP_CIRCLE_RADIUS)
            .bg(palette.brand)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child((i + 1).to_string()),
            );
        let connector = div()
            .w(STEP_CONNECTOR_W)
            .h(if is_last { px(0.0) } else { STEP_CONNECTOR_H })
            .bg(palette.border_regular);
        let left_col = div()
            .flex()
            .flex_col()
            .items_center()
            .w(STEP_COL_W)
            .child(circle)
            .child(connector);

        let title_text = div()
            .flex_1()
            .font_family(body_family())
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(title);
        let health_dot = if depth == 0 {
            self.step_health
                .get(i)
                .map(|health| self.render_health_dot(health, i, palette))
        } else {
            None
        };
        let title_el = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(title_text)
            .children(health_dot);

        let enabled = step.enabled;
        let closed_gate = self.closed_step_gate(step);
        let meta = match &closed_gate {
            Some(gate) => div()
                .flex()
                .flex_col()
                .child(variable_text(&detail_str, palette))
                .child(
                    div()
                        .mt(STEP_NOTICE_MT)
                        .child(self.render_closed_gate_notice(
                            gate,
                            SharedString::from(format!("actions-step-enable-{depth}-{i}")),
                            palette,
                            cx,
                        )),
                )
                .into_any_element(),
            None => variable_text(&detail_str, palette).into_any_element(),
        };
        let mut card = row_card(title_el, palette)
            .leading(icon(glyph, CARD_GLYPH, glyph_color))
            .meta(meta)
            .trailing(self.render_step_controls(i, total, enabled, palette, cx))
            .idle_background(palette.elevated)
            .bordered(palette.border_regular, BORDER_THIN, radius(Radius::Md))
            .padding_xy(STEP_CARD_PAD_V, STEP_CARD_PAD_H)
            .on_click(
                SharedString::from(format!("actions-step-card-{i}")),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.open_edit_sub_action(i, cx)),
            );
        if closed_gate.is_some() {
            card = card.align_top();
        }
        if self.step_menu_open != Some(i) {
            card = card.trailing_reveal(SharedString::from(format!("actions-step-row-{i}")));
        }

        let step_row = div()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(left_col)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .when(!enabled, |el| el.opacity(STEP_DISABLED_OPACITY))
                    .child(card),
            );

        let block: AnyElement = match self.render_branch_affordances(step, i, depth, palette, cx) {
            Some(branches) => {
                let indented = div()
                    .pl(STEP_COL_W + spacing(Spacing::Xs, Density::Cozy))
                    .pt(spacing(Spacing::Xxs, Density::Cozy))
                    .child(branches);
                div()
                    .flex()
                    .flex_col()
                    .child(step_row)
                    .child(indented)
                    .into_any_element()
            }
            None => step_row.into_any_element(),
        };

        div()
            .w_full()
            .pb(if is_last { px(0.0) } else { STEP_GAP })
            .child(block)
            .into_any_element()
    }
}
