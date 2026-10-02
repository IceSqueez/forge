use std::collections::HashSet;

use forge_components::{ForgePalette, integration_inactive_badge, tr};
use forge_types::IntegrationId;
use gpui::{Context, Entity, IntoElement, SharedString, Subscription};

use crate::integration_catalog::declaration_of;
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::integration_supervisor::IntegrationSupervisor;

#[derive(Clone)]
pub struct IntegrationSwitch {
    lifecycle: Entity<IntegrationLifecycle>,
    supervisor: IntegrationSupervisor,
}

pub struct SwitchWatch {
    switch: IntegrationSwitch,
    off: HashSet<IntegrationId>,
    _observer: Subscription,
}

impl IntegrationSwitch {
    pub fn new(lifecycle: Entity<IntegrationLifecycle>, supervisor: IntegrationSupervisor) -> Self {
        Self {
            lifecycle,
            supervisor,
        }
    }

    pub fn enable(&self, id: &IntegrationId) {
        if let Some(slot) = self.supervisor.slot(id) {
            slot.request_enable();
        }
    }

    pub fn watch<V: 'static>(
        self,
        cx: &mut Context<V>,
        apply: impl Fn(&mut V, HashSet<IntegrationId>, &mut Context<V>) + 'static,
    ) -> SwitchWatch {
        let off = self.lifecycle.read(cx).switched_off();
        let observer = cx.observe(&self.lifecycle, move |view, lifecycle, cx| {
            let off = lifecycle.read(cx).switched_off();
            apply(view, off, cx);
            cx.notify();
        });
        SwitchWatch {
            switch: self,
            off,
            _observer: observer,
        }
    }
}

impl SwitchWatch {
    pub fn replace(&mut self, off: HashSet<IntegrationId>) {
        self.off = off;
    }

    pub fn off(&self) -> &HashSet<IntegrationId> {
        &self.off
    }

    pub fn switch(&self) -> IntegrationSwitch {
        self.switch.clone()
    }

    pub fn off_owner<'a>(&self, owner: Option<&'a IntegrationId>) -> Option<&'a IntegrationId> {
        owner.filter(|id| self.off.contains(*id))
    }

    pub fn is_off(&self, id: &IntegrationId) -> bool {
        self.off.contains(id)
    }

    pub fn enable(&self, id: &IntegrationId) {
        self.switch.enable(id);
    }
}

pub fn integration_name(id: &IntegrationId) -> SharedString {
    match declaration_of(id) {
        Some(declaration) => SharedString::new_static(declaration.brand_name),
        None => SharedString::from(id.as_str().to_owned()),
    }
}

pub fn inactive_badge(owner: &IntegrationId, palette: &ForgePalette) -> impl IntoElement {
    integration_inactive_badge(
        tr!(
            "integration_inactive_badge",
            name = integration_name(owner).to_string()
        ),
        palette,
    )
}
