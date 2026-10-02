use super::*;
use crate::integration_switch::{IntegrationSwitch, integration_name};
use forge_components::{integration_disabled_notice, mono_family};
use forge_types::IntegrationId;

impl ScreenActionsView {
    #[must_use]
    pub fn with_integration_switch(
        mut self,
        switch: IntegrationSwitch,
        cx: &mut Context<Self>,
    ) -> Self {
        self.integrations = Some(switch.watch(cx, |this: &mut Self, off, _cx| {
            if let Some(integrations) = &mut this.integrations {
                integrations.replace(off);
            }
            this.recompute_step_health();
        }));
        self.recompute_step_health();
        self
    }

    pub(super) fn switched_off_step_owner(&self, step: &SubActionStep) -> Option<IntegrationId> {
        if !step.enabled {
            return None;
        }
        let integrations = self.integrations.as_ref()?;
        integrations
            .off_owner(self.sub_action_registry.owning_integration(&step.kind_id))
            .cloned()
    }

    pub(super) fn switched_off_trigger_owner(&self, kind_id: &str) -> Option<IntegrationId> {
        let integrations = self.integrations.as_ref()?;
        integrations
            .off_owner(self.trigger_registry.owning_integration(kind_id))
            .cloned()
    }

    pub(super) fn enable_integration(&mut self, owner: &IntegrationId, cx: &mut Context<Self>) {
        if let Some(integrations) = &self.integrations {
            integrations.enable(owner);
        }
        cx.notify();
    }

    pub(super) fn render_disabled_step_notice(
        &self,
        owner: &IntegrationId,
        id: SharedString,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = integration_name(owner).to_string();
        let message = div()
            .flex()
            .items_center()
            .whitespace_nowrap()
            .overflow_hidden()
            .child(tr!(
                "action_editor_step_disabled_prefix",
                name = name.clone()
            ))
            .child(
                div()
                    .font_family(mono_family())
                    .text_color(palette.random)
                    .child(tr!("integration_disable_reason")),
            );
        let owner = owner.clone();
        integration_disabled_notice(
            id,
            message,
            tr!("integration_disabled_enable", name = name),
            palette,
        )
        .on_enable(
            cx.listener(move |this, _: &ClickEvent, _, cx| this.enable_integration(&owner, cx)),
        )
        .into_any_element()
    }
}
