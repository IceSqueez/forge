use forge_components::{ForgePalette, integration_disabled_notice, tr};
use forge_types::IntegrationId;
use gpui::{AnyElement, ClickEvent, Context, prelude::*};

use super::SoundboardView;
use crate::integration_switch::{IntegrationSwitch, SwitchWatch, integration_name};

pub(crate) fn hotkeys_integration() -> IntegrationId {
    forge_hotkey::HOTKEY_INTEGRATION.id
}

impl SoundboardView {
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
        }));
        self
    }

    pub(super) fn integration_switch(&self) -> Option<IntegrationSwitch> {
        self.integrations.as_ref().map(SwitchWatch::switch)
    }

    pub fn hotkeys_switched_off(&self) -> bool {
        self.integrations
            .as_ref()
            .is_some_and(|integrations| integrations.is_off(&hotkeys_integration()))
    }

    fn enable_hotkeys(&mut self, cx: &mut Context<Self>) {
        if let Some(integrations) = &self.integrations {
            integrations.enable(&hotkeys_integration());
        }
        cx.notify();
    }

    pub(super) fn render_hotkeys_notice(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.hotkeys_switched_off() {
            return None;
        }
        let name = integration_name(&hotkeys_integration()).to_string();
        Some(
            integration_disabled_notice(
                "sb-hotkeys-enable",
                tr!("soundboard_hotkeys_disabled", name = name.clone()),
                tr!("integration_disabled_enable", name = name),
                palette,
            )
            .on_enable(cx.listener(|this, _: &ClickEvent, _, cx| this.enable_hotkeys(cx)))
            .into_any_element(),
        )
    }
}
