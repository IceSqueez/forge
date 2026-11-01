use super::*;
use crate::integrations::integration_switch::{ClosedGate, IntegrationSwitch, integration_name};
use forge_components::{ActivePresentation, integration_disabled_notice};
use forge_types::IntegrationId;

impl ActionEditor {
    pub(in crate::automation::actions_screen) fn attach_integration_switch(
        &mut self,
        switch: IntegrationSwitch,
        cx: &mut Context<Self>,
    ) {
        self.integrations = Some(switch.watch(cx, |this: &mut Self, off, cx| {
            if let Some(integrations) = &mut this.integrations {
                integrations.replace(off, cx);
            }
            this.recompute_step_health();
        }));
        self.recompute_step_health();
    }

    pub(super) fn switched_off_step_owner(&self, step: &SubActionStep) -> Option<IntegrationId> {
        if !step.enabled {
            return None;
        }
        let integrations = self.integrations.as_ref()?;
        integrations
            .off_owner(
                self.deps
                    .sub_action_registry
                    .owning_integration(&step.kind_id),
            )
            .cloned()
    }

    fn failed_step_owner(&self, step: &SubActionStep) -> Option<IntegrationId> {
        if !step.enabled {
            return None;
        }
        let integrations = self.integrations.as_ref()?;
        integrations
            .failed_owner(
                self.deps
                    .sub_action_registry
                    .owning_integration(&step.kind_id),
            )
            .cloned()
    }

    pub(super) fn closed_step_gate(&self, step: &SubActionStep) -> Option<ClosedGate> {
        if let Some(owner) = self.switched_off_step_owner(step) {
            return Some(ClosedGate::SwitchedOff(owner));
        }
        self.failed_step_owner(step).map(ClosedGate::Failed)
    }

    pub(super) fn uncovered_trigger_category(
        &self,
        kind_id: &str,
    ) -> Option<forge_platform_core::IntegrationCategory> {
        self.integrations.as_ref()?.uncovered_category(kind_id)
    }

    pub(super) fn switched_off_trigger_owner(&self, kind_id: &str) -> Option<IntegrationId> {
        let integrations = self.integrations.as_ref()?;
        integrations
            .off_owner(self.deps.trigger_registry.owning_integration(kind_id))
            .cloned()
    }

    pub(super) fn enable_integration(&mut self, owner: &IntegrationId, cx: &mut Context<Self>) {
        if let Some(integrations) = &self.integrations {
            integrations.enable(owner);
        }
        cx.notify();
    }

    pub(super) fn render_closed_gate_notice(
        &self,
        gate: &ClosedGate,
        id: SharedString,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner = gate.owner().clone();
        let name = integration_name(&owner).to_string();
        let (message, action_label) = match gate {
            ClosedGate::SwitchedOff(_) => (
                div()
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
                            .font_family(cx.mono_family())
                            .text_color(palette.danger)
                            .child(tr!("integration_disable_reason")),
                    ),
                tr!("integration_disabled_enable", name = name),
            ),
            ClosedGate::Failed(_) => (
                div()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(tr!("action_editor_step_failed_notice", name = name)),
                tr!("integration_retry"),
            ),
        };
        integration_disabled_notice(id, message, action_label, palette)
            .on_enable(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.enable_integration(&owner, cx)),
            )
            .into_any_element()
    }
}

#[cfg(test)]
pub(in crate::automation::actions_screen) mod tests {
    use forge_components::ThemeId;
    use forge_runtime::ActionsService;
    use forge_runtime::QueueScheduler;
    use forge_soundboard::ClipLibrary;
    use forge_storage::MockQueueRepo;
    use forge_storage::MockSoundboardClipsRepo;
    use forge_storage::MockTriggerInstanceRepo;
    use forge_storage::{DataProvider, SoundboardClipsRepo};
    use forge_storage::{GlobalsRepo, QueueRepo, ScriptRepo};

    use super::*;
    use crate::test_support::{
        StubActions, StubEventLog, StubHistory, StubOverlays, action_running, lifecycle_switch,
        owned_sub_actions, owned_triggers, runtime, step, switch_lifecycle, test_backend,
    };
    use forge_components::Presentation;
    use forge_runtime::{LifecycleState, LifecycleStates};

    const OBS_SCENE: &str = "obs.scene.set";
    const OBS_SCENE_CHANGED: &str = "obs.scene.changed";
    const CORE_LOG: &str = "core.log";

    pub(in crate::automation::actions_screen) fn obs() -> IntegrationId {
        IntegrationId::new("obs")
    }

    fn obs_in(state: LifecycleState) -> LifecycleStates {
        LifecycleStates::from([(obs(), state)])
    }

    pub(in crate::automation::actions_screen) fn detail(steps: Vec<SubActionStep>) -> ActionDetail {
        ActionDetail {
            action: action_running("Scene", steps),
            trigger_instances: Vec::new(),
            sub_action_avg_ms: Vec::new(),
            last_step_outcomes: Vec::new(),
        }
    }

    pub(in crate::automation::actions_screen) struct Rig {
        pub(in crate::automation::actions_screen) view: Entity<ActionEditor>,
        pub(in crate::automation::actions_screen) host: Entity<ScreenActionsView>,
        lifecycle: Entity<crate::integrations::integration_lifecycle::IntegrationLifecycle>,
        _rt: tokio::runtime::Runtime,
    }

    impl Rig {
        pub(in crate::automation::actions_screen) fn open(
            cx: &mut gpui::TestAppContext,
            initial: LifecycleState,
            open: Option<ActionDetail>,
        ) -> Self {
            cx.update(|cx| {
                cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            });
            let rt = runtime();
            let (lifecycle, switch) = lifecycle_switch(cx, &rt, obs_in(initial));
            let action_repo: Arc<dyn ActionRepo> = Arc::new(StubActions);
            let queue_repo: Arc<dyn QueueRepo> = Arc::new(MockQueueRepo::new());
            let trigger_repo: Arc<dyn TriggerInstanceRepo> =
                Arc::new(MockTriggerInstanceRepo::new());
            let clips: Arc<dyn SoundboardClipsRepo> = Arc::new(MockSoundboardClipsRepo::new());
            let service = Arc::new(ActionsService::new(
                Arc::clone(&action_repo),
                Arc::clone(&queue_repo),
                Arc::new(StubHistory),
                Arc::clone(&trigger_repo),
            ));
            let (backend, _writes) = test_backend();
            let (bus, scheduler) = rt.block_on(async {
                let bus = EventBus::new(Arc::new(StubEventLog));
                let engine = crate::test_support::stub_action_engine(Arc::clone(&bus));
                let scheduler = QueueScheduler::spawn(engine, Arc::clone(&bus), Vec::new());
                (bus, scheduler)
            });
            let handle = rt.handle().clone();
            let host = cx.update(|cx| {
                cx.new(|cx| {
                    let view = ScreenActionsView::new(
                        super::ActionsDeps {
                            action_repo,
                            queue_repo,
                            actions_service: service,
                            trigger_instance_repo: trigger_repo,
                            script_repo: Arc::clone(&backend) as Arc<dyn ScriptRepo>,
                            clip_library: Arc::new(ClipLibrary::new(
                                clips,
                                backend.media_repo(),
                                Arc::new(forge_events::test_support::NullPublisher),
                            )),
                            globals_repo: Arc::clone(&backend) as Arc<dyn GlobalsRepo>,
                            overlay_repo: Arc::new(StubOverlays),
                            speak: None,
                            sub_action_registry: Arc::new(owned_sub_actions(&[(OBS_SCENE, "obs")])),
                            trigger_registry: Arc::new(owned_triggers(&[(
                                OBS_SCENE_CHANGED,
                                "obs",
                            )])),
                            rt_handle: handle,
                            bus,
                            scheduler,
                        },
                        Arc::clone(&backend) as Arc<dyn SettingsRepo>,
                        Arc::new(OverlayKindRegistry::new()),
                        None,
                        cx,
                    );
                    view.editor.update(cx, |editor, _| editor.detail = open);
                    view.with_integration_switch(switch, cx)
                })
            });
            let view = host.read_with(cx, |host, _| host.editor.clone());
            Self {
                view,
                host,
                lifecycle,
                _rt: rt,
            }
        }

        fn flagged(&self, cx: &mut gpui::TestAppContext) -> Vec<Option<IntegrationId>> {
            self.view.read_with(cx, |view, _| {
                view.step_health
                    .iter()
                    .map(|health| health.disabled_integration().cloned())
                    .collect()
            })
        }

        fn flagged_failed(&self, cx: &mut gpui::TestAppContext) -> Vec<Option<IntegrationId>> {
            self.view.read_with(cx, |view, _| {
                view.step_health
                    .iter()
                    .map(|health| health.failed_integration().cloned())
                    .collect()
            })
        }
    }

    #[gpui::test]
    fn attaching_the_switch_flags_an_open_action_at_once(cx: &mut gpui::TestAppContext) {
        let rig = Rig::open(
            cx,
            LifecycleState::Disabled,
            Some(detail(vec![step(CORE_LOG, true), step(OBS_SCENE, true)])),
        );

        assert_eq!(rig.flagged(cx), vec![None, Some(obs())]);
    }

    #[gpui::test]
    fn step_health_follows_the_integration_switching_off_and_back_on(
        cx: &mut gpui::TestAppContext,
    ) {
        let rig = Rig::open(
            cx,
            LifecycleState::Running,
            Some(detail(vec![step(OBS_SCENE, true)])),
        );

        let before = rig.flagged(cx);
        switch_lifecycle(cx, &rig.lifecycle, obs_in(LifecycleState::Stopping));
        let off = rig.flagged(cx);
        switch_lifecycle(cx, &rig.lifecycle, obs_in(LifecycleState::Running));
        let back_on = rig.flagged(cx);

        assert_eq!(
            (before, off, back_on),
            (vec![None], vec![Some(obs())], vec![None])
        );
    }

    #[gpui::test]
    fn only_an_enabled_step_owned_by_a_switched_off_integration_gets_the_inline_notice(
        cx: &mut gpui::TestAppContext,
    ) {
        let rig = Rig::open(cx, LifecycleState::Disabled, None);

        let owners = rig.view.read_with(cx, |view, _| {
            [
                step(OBS_SCENE, true),
                step(OBS_SCENE, false),
                step(CORE_LOG, true),
            ]
            .iter()
            .map(|step| view.switched_off_step_owner(step))
            .collect::<Vec<_>>()
        });

        assert_eq!(owners, vec![Some(obs()), None, None]);
    }

    #[gpui::test]
    fn a_linked_trigger_owned_by_a_switched_off_integration_is_marked(
        cx: &mut gpui::TestAppContext,
    ) {
        let rig = Rig::open(cx, LifecycleState::Disabled, None);

        let owners = rig.view.read_with(cx, |view, _| {
            [OBS_SCENE_CHANGED, "core.timer"].map(|kind| view.switched_off_trigger_owner(kind))
        });

        assert_eq!(owners, [Some(obs()), None]);
    }

    fn failed() -> LifecycleState {
        LifecycleState::Failed(crate::test_support::start_failure("token revoked"))
    }

    #[gpui::test]
    fn the_closed_gate_of_a_step_follows_its_owner_lifecycle(cx: &mut gpui::TestAppContext) {
        for (state, probe, expected, case) in [
            (
                LifecycleState::Disabled,
                step(OBS_SCENE, true),
                Some(ClosedGate::SwitchedOff(obs())),
                "a switched-off owner",
            ),
            (
                failed(),
                step(OBS_SCENE, true),
                Some(ClosedGate::Failed(obs())),
                "a failed owner",
            ),
            (
                failed(),
                step(OBS_SCENE, false),
                None,
                "a disabled step of a failed owner",
            ),
            (failed(), step(CORE_LOG, true), None, "an ownerless step"),
            (
                LifecycleState::Running,
                step(OBS_SCENE, true),
                None,
                "a running owner",
            ),
        ] {
            let rig = Rig::open(cx, state, None);

            let gate = rig
                .view
                .read_with(cx, |view, _| view.closed_step_gate(&probe));

            assert_eq!(gate, expected, "{case}");
        }
    }

    #[gpui::test]
    fn step_health_follows_the_integration_failing_and_recovering(cx: &mut gpui::TestAppContext) {
        let rig = Rig::open(
            cx,
            LifecycleState::Running,
            Some(detail(vec![step(CORE_LOG, true), step(OBS_SCENE, true)])),
        );

        let before = rig.flagged_failed(cx);
        switch_lifecycle(cx, &rig.lifecycle, obs_in(failed()));
        let failing = rig.flagged_failed(cx);
        switch_lifecycle(cx, &rig.lifecycle, obs_in(LifecycleState::Running));
        let recovered = rig.flagged_failed(cx);

        assert_eq!(
            (before, failing, recovered),
            (vec![None, None], vec![None, Some(obs())], vec![None, None])
        );
    }

    #[gpui::test]
    fn the_closed_gate_follows_the_integration_failing_after_the_screen_opened(
        cx: &mut gpui::TestAppContext,
    ) {
        let rig = Rig::open(cx, LifecycleState::Running, None);

        switch_lifecycle(cx, &rig.lifecycle, obs_in(failed()));
        let gate = rig
            .view
            .read_with(cx, |view, _| view.closed_step_gate(&step(OBS_SCENE, true)));

        assert_eq!(gate, Some(ClosedGate::Failed(obs())));
    }
}
