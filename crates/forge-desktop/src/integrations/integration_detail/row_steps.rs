use super::IntegrationDetail;
use crate::integrations::builtin_sections::StepClick;
use crate::integrations::integration_quick_actions::enqueue_builtin_step;
use forge_components::{PushToast, ToastKind, tr};
use forge_runtime::QuickActionOrigin;
use gpui::Context;

impl IntegrationDetail {
    pub(super) fn run_row_step(&mut self, click: &StepClick, cx: &mut Context<Self>) {
        let row = click.row.to_string();
        let kind = click.step.kind_id.clone();
        if !self.live.row_steps_in_flight.try_begin(&row, &kind) {
            return;
        }
        cx.notify();
        let origin = QuickActionOrigin {
            builtin_id: self.ports.status.id().as_str().to_owned(),
            label: row.clone(),
        };
        enqueue_builtin_step(
            origin,
            self.services.action_engine.clone(),
            &self.services.rt_handle,
            click.step.clone(),
            move |detail: &mut Self, result, cx| {
                detail.live.row_steps_in_flight.finish(&row, &kind);
                if let Err(err) = result {
                    tracing::warn!(error = %err, "content row action failed");
                    cx.push_toast(
                        ToastKind::Error,
                        tr!("integration_row_action_failed", error = err.text()),
                    );
                }
                cx.notify();
            },
            cx,
        );
    }
}
