use std::collections::HashSet;

use forge_components::{ForgePalette, integration_inactive_badge, tr};
use forge_types::IntegrationId;
use gpui::{App, Context, Entity, IntoElement, SharedString, Subscription};

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
    failed: HashSet<IntegrationId>,
    _observer: Subscription,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClosedGate {
    SwitchedOff(IntegrationId),
    Failed(IntegrationId),
}

impl ClosedGate {
    pub fn owner(&self) -> &IntegrationId {
        match self {
            ClosedGate::SwitchedOff(owner) | ClosedGate::Failed(owner) => owner,
        }
    }
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
        let failed = self.lifecycle.read(cx).failed();
        let observer = cx.observe(&self.lifecycle, move |view, lifecycle, cx| {
            let off = lifecycle.read(cx).switched_off();
            apply(view, off, cx);
            cx.notify();
        });
        SwitchWatch {
            switch: self,
            off,
            failed,
            _observer: observer,
        }
    }
}

impl SwitchWatch {
    pub fn replace(&mut self, off: HashSet<IntegrationId>) {
        self.off = off;
    }

    pub fn sync_failed(&mut self, cx: &App) {
        self.failed = self.switch.lifecycle.read(cx).failed();
    }

    pub fn failed(&self) -> &HashSet<IntegrationId> {
        &self.failed
    }

    pub fn failed_owner<'a>(&self, owner: Option<&'a IntegrationId>) -> Option<&'a IntegrationId> {
        owner.filter(|id| self.failed.contains(*id))
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use forge_storage::{SettingsRepo, integration_enabled_key};
    use gpui::{AppContext, TestAppContext};

    use super::*;
    use crate::integration_supervisor::{LifecycleState, LifecycleStates};
    use crate::test_support::{
        SettingWrite, idle_supervisor, lifecycle_switch, pump, runtime, switch_lifecycle,
        test_backend,
    };

    fn obs() -> IntegrationId {
        IntegrationId::new("obs")
    }

    fn twitch() -> IntegrationId {
        IntegrationId::new("twitch")
    }

    fn states(entries: &[(IntegrationId, LifecycleState)]) -> LifecycleStates {
        entries.iter().cloned().collect()
    }

    fn drain(writes: &mut tokio::sync::mpsc::UnboundedReceiver<SettingWrite>) -> Vec<SettingWrite> {
        std::iter::from_fn(|| writes.try_recv().ok()).collect()
    }

    struct Host {
        watch: Option<SwitchWatch>,
        applied: Vec<HashSet<IntegrationId>>,
    }

    fn watching(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        initial: LifecycleStates,
    ) -> (Entity<IntegrationLifecycle>, Entity<Host>) {
        let (lifecycle, switch) = lifecycle_switch(cx, rt, initial);
        let host = cx.update(|cx| {
            cx.new(|cx| Host {
                watch: Some(switch.watch(cx, |host: &mut Host, off, _cx| host.applied.push(off))),
                applied: Vec::new(),
            })
        });
        (lifecycle, host)
    }

    #[gpui::test]
    fn a_watch_starts_from_the_integrations_already_switched_off(cx: &mut TestAppContext) {
        let rt = runtime();
        let (_lifecycle, host) = watching(
            cx,
            &rt,
            states(&[
                (obs(), LifecycleState::Disabled),
                (twitch(), LifecycleState::Running),
            ]),
        );

        host.read_with(cx, |host, _| {
            assert_eq!(
                host.watch.as_ref().map(SwitchWatch::off),
                Some(&HashSet::from([obs()]))
            );
        });
    }

    #[gpui::test]
    fn a_lifecycle_change_hands_the_watcher_the_new_switched_off_set(cx: &mut TestAppContext) {
        let rt = runtime();
        let (lifecycle, host) = watching(cx, &rt, states(&[(twitch(), LifecycleState::Running)]));

        switch_lifecycle(
            cx,
            &lifecycle,
            states(&[
                (twitch(), LifecycleState::Stopping),
                (obs(), LifecycleState::Starting),
            ]),
        );

        host.read_with(cx, |host, _| {
            assert_eq!(host.applied, vec![HashSet::from([twitch()])]);
        });
    }

    #[gpui::test]
    fn an_owner_counts_as_switched_off_only_when_it_is_in_the_off_set(cx: &mut TestAppContext) {
        let rt = runtime();
        let (_lifecycle, host) = watching(
            cx,
            &rt,
            states(&[
                (obs(), LifecycleState::Disabled),
                (twitch(), LifecycleState::Running),
            ]),
        );

        host.read_with(cx, |host, _| {
            let watch = host.watch.as_ref().expect("the host keeps its watch");
            let (obs, twitch) = (obs(), twitch());
            assert_eq!(
                [
                    watch.off_owner(Some(&obs)),
                    watch.off_owner(Some(&twitch)),
                    watch.off_owner(None),
                ],
                [Some(&obs), None, None]
            );
        });
    }

    fn failed() -> LifecycleState {
        LifecycleState::Failed("token revoked".to_owned())
    }

    fn failed_set(host: &Entity<Host>, cx: &mut TestAppContext) -> HashSet<IntegrationId> {
        host.read_with(cx, |host, _| {
            host.watch
                .as_ref()
                .expect("the host keeps its watch")
                .failed()
                .clone()
        })
    }

    #[gpui::test]
    fn a_watch_starts_from_the_integrations_already_failed(cx: &mut TestAppContext) {
        let rt = runtime();
        let (_lifecycle, host) = watching(
            cx,
            &rt,
            states(&[(obs(), failed()), (twitch(), LifecycleState::Disabled)]),
        );

        assert_eq!(failed_set(&host, cx), HashSet::from([obs()]));
    }

    #[gpui::test]
    fn syncing_after_a_lifecycle_change_picks_up_the_new_failed_set(cx: &mut TestAppContext) {
        let rt = runtime();
        let (lifecycle, host) = watching(
            cx,
            &rt,
            states(&[(obs(), failed()), (twitch(), LifecycleState::Running)]),
        );

        switch_lifecycle(
            cx,
            &lifecycle,
            states(&[(obs(), LifecycleState::Running), (twitch(), failed())]),
        );
        host.update(cx, |host, cx| {
            if let Some(watch) = &mut host.watch {
                watch.sync_failed(cx);
            }
        });

        assert_eq!(failed_set(&host, cx), HashSet::from([twitch()]));
    }

    #[gpui::test]
    fn an_owner_counts_as_failed_only_when_it_is_in_the_failed_set(cx: &mut TestAppContext) {
        let rt = runtime();
        let (_lifecycle, host) = watching(
            cx,
            &rt,
            states(&[(obs(), failed()), (twitch(), LifecycleState::Disabled)]),
        );

        host.read_with(cx, |host, _| {
            let watch = host.watch.as_ref().expect("the host keeps its watch");
            let (obs, twitch) = (obs(), twitch());
            assert_eq!(
                [
                    watch.failed_owner(Some(&obs)),
                    watch.failed_owner(Some(&twitch)),
                    watch.failed_owner(None),
                ],
                [Some(&obs), None, None]
            );
        });
    }

    #[gpui::test]
    fn enabling_through_the_switch_persists_the_integration_as_enabled(cx: &mut TestAppContext) {
        let rt = runtime();
        let (settings, mut writes) = test_backend();
        let supervisor = idle_supervisor(&rt, settings as Arc<dyn SettingsRepo>, &[obs()]);
        pump(&rt);
        drain(&mut writes);
        let switch = cx.update(|cx| {
            let lifecycle = cx.new(|_| IntegrationLifecycle::new(LifecycleStates::new()));
            IntegrationSwitch::new(lifecycle, supervisor)
        });

        switch.enable(&obs());
        pump(&rt);

        assert_eq!(
            drain(&mut writes),
            vec![(integration_enabled_key(&obs()), "true".to_owned())]
        );
    }

    #[gpui::test]
    fn enabling_an_integration_the_supervisor_does_not_manage_writes_nothing(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let (settings, mut writes) = test_backend();
        let supervisor = idle_supervisor(&rt, settings as Arc<dyn SettingsRepo>, &[obs()]);
        pump(&rt);
        drain(&mut writes);
        let switch = cx.update(|cx| {
            let lifecycle = cx.new(|_| IntegrationLifecycle::new(LifecycleStates::new()));
            IntegrationSwitch::new(lifecycle, supervisor)
        });

        switch.enable(&twitch());
        pump(&rt);

        assert_eq!(drain(&mut writes), Vec::<SettingWrite>::new());
    }
}
