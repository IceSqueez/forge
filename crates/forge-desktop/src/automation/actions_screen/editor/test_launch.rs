use super::*;
use crate::automation::actions_screen::test_run::{
    RowStatus, TestRunEvent, TestRunLaunch, TestRunModal, TestRunNote, TestRunRow,
    TestTriggerChoice,
};
use forge_registry::{SynthesisSample, synthesize_args};
use forge_runtime::QueueAdmission;
use forge_types::ArgStack;
impl ActionEditor {
    pub(in crate::automation::actions_screen) fn start_test_run(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        let selected = match self.detail.as_ref() {
            Some(detail) => detail
                .trigger_instances
                .iter()
                .position(|inst| inst.enabled)
                .or_else(|| (!detail.trigger_instances.is_empty()).then_some(0)),
            None => return,
        };
        self.launch_test_run(id, selected, cx);
    }

    fn launch_test_run(&mut self, id: ActionId, selected: Option<usize>, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let prepared = {
            let Some(detail) = self.detail.as_ref() else {
                return;
            };
            if detail.action.id != id {
                return;
            }
            let action = &detail.action;
            let action_name: SharedString = action.name.clone().into();
            let queue_id = action.queue_id;
            let admission = QueueAdmission::from(action);

            let rows: Vec<TestRunRow> = action
                .sub_actions
                .iter()
                .enumerate()
                .map(|(index, step)| self.test_run_row(index, step, &palette))
                .collect();

            let triggers: Vec<TestTriggerChoice> = detail
                .trigger_instances
                .iter()
                .map(|inst| {
                    let descriptor = self.deps.trigger_registry.get(&inst.kind_id);
                    let kind_label = descriptor
                        .map(crate::shared::kind_copy::trigger_name)
                        .unwrap_or_else(|| inst.kind_id.clone());
                    TestTriggerChoice {
                        name: crate::shared::kind_copy::trigger_instance_name(
                            &inst.name, descriptor,
                        )
                        .into(),
                        kind_label: kind_label.into(),
                    }
                })
                .collect();

            let selected_inst = selected.and_then(|i| detail.trigger_instances.get(i));
            let trigger_kind = selected_inst.map(|inst| inst.kind_id.clone());
            let declared = selected_inst.and_then(|inst| {
                let descriptor = self.deps.trigger_registry.get(&inst.kind_id)?;
                let variables = self
                    .deps
                    .trigger_registry
                    .declared_variables(&inst.kind_id)?;
                Some((variables, descriptor.platform_contract()))
            });
            let (initial_args, note) = match (selected_inst, declared) {
                (Some(inst), Some((variables, contract))) => (
                    self.deps.trigger_registry.with_chat_event(
                        &inst.kind_id,
                        synthesize_args(variables, &SynthesisSample::random(contract)),
                    ),
                    None,
                ),
                (Some(_), None) => (ArgStack::new(), Some(TestRunNote::NoSchema)),
                (None, _) => (ArgStack::new(), Some(TestRunNote::NoTriggers)),
            };

            TestRunLaunch {
                action_id: id,
                action_name,
                queue_id,
                admission,
                rows,
                triggers,
                selected_trigger: selected,
                trigger_kind,
                initial_args,
                note,
            }
        };

        let bus = Arc::clone(&self.deps.bus);
        let scheduler = self.deps.scheduler.clone();
        let rt_handle = self.deps.rt_handle.clone();
        let modal = cx.new(|cx| TestRunModal::new(prepared, bus, scheduler, rt_handle, cx));
        self.modals._test_run_sub = Some(cx.subscribe(&modal, Self::on_test_run_event));
        self.modals.test_run = Some(modal);
        cx.notify();
    }

    pub(super) fn hand_off_test_run(&mut self, cx: &mut App) {
        if let Some(modal) = self.modals.test_run.take() {
            modal.update(cx, |modal, _| modal.continue_in_background());
        }
    }

    fn test_run_row(
        &self,
        index: usize,
        step: &SubActionStep,
        palette: &ForgePalette,
    ) -> TestRunRow {
        let runner = self.deps.sub_action_registry.get(&step.kind_id);
        let kind_name = runner
            .map(crate::shared::kind_copy::step_name)
            .unwrap_or_else(|| tr!("action_editor_kind_sub_action"));
        let name = row_name(step.label.as_deref(), kind_name);
        let (icon, color) = super::editor::step_glyph(
            &step.kind_id,
            runner
                .map(|r| r.icon_name())
                .unwrap_or(super::editor::UNKNOWN_STEP_ICON),
            runner.map(|r| {
                super::editor::StepGroup::of(&self.deps.sub_action_registry, r).color(palette)
            }),
            palette,
        );
        TestRunRow {
            index,
            name: name.into(),
            icon,
            color,
            status: RowStatus::Queued,
        }
    }

    fn on_test_run_event(
        &mut self,
        modal: Entity<TestRunModal>,
        event: &TestRunEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TestRunEvent::Relaunch {
                action_id,
                selected,
            } => {
                modal.update(cx, |modal, _| modal.continue_in_background());
                self.launch_test_run(*action_id, *selected, cx);
            }
            TestRunEvent::DetailChanged(id) => self.load_detail_for(*id, cx),
            TestRunEvent::Closed(id) => {
                self.modals.test_run = None;
                self.modals._test_run_sub = None;
                self.load_detail_for(*id, cx);
                cx.notify();
            }
            TestRunEvent::Failed(message) => {
                self.modals.test_run = None;
                self.modals._test_run_sub = None;
                cx.push_toast(
                    ToastKind::Error,
                    tr!("action_editor_test_failed", error = message.as_str()),
                );
                cx.notify();
            }
        }
    }
}

fn row_name(label: Option<&str>, kind_name: String) -> String {
    label
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map_or(kind_name, str::to_owned)
}

#[cfg(test)]
mod row_name_tests {
    use super::row_name;

    #[test]
    fn a_step_label_names_the_row_unless_it_is_blank() {
        for (label, expected) in [
            (
                Some("Show alert, named cheerer"),
                "Show alert, named cheerer",
            ),
            (Some("  padded  "), "padded"),
            (Some("   "), "Send to Overlay"),
            (None, "Send to Overlay"),
        ] {
            assert_eq!(row_name(label, "Send to Overlay".to_owned()), expected);
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_components::{Presentation, ThemeId, ToastKind};
    use forge_storage::{
        DataProvider, MockQueueRepo, MockSoundboardClipsRepo, MockTriggerInstanceRepo,
    };

    use super::*;
    use crate::automation::actions_screen::test_run::tests::{WatchedRig, press_run_again, toasts};
    use crate::test_support::{ActionsRepos, StubActions, actions_deps, test_backend};

    fn editor_over(cx: &mut gpui::TestAppContext, rig: &WatchedRig) -> Entity<ActionEditor> {
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
        });
        let (backend, _writes) = test_backend();
        let mut deps = actions_deps(
            &rig.rt,
            ActionsRepos {
                actions: Arc::new(StubActions),
                queues: Arc::new(MockQueueRepo::new()),
                triggers: Arc::new(MockTriggerInstanceRepo::new()),
                scripts: backend.script_repo(),
                clips: Arc::new(MockSoundboardClipsRepo::new()),
                media: backend.media_repo(),
                globals: backend.globals_repo(),
                sub_actions: SubActionRegistry::new(),
                trigger_kinds: TriggerRegistry::new(),
            },
        );
        deps.bus = Arc::clone(&rig.bus);
        deps.scheduler = rig.scheduler.clone();
        let detail = ActionDetail {
            action: rig.action.clone(),
            trigger_instances: Vec::new(),
            sub_action_avg_ms: Vec::new(),
            last_step_outcomes: Vec::new(),
        };
        let selected = Some(rig.action.id);
        cx.update(|cx| {
            cx.new(|cx| {
                let mut editor = ActionEditor::new(
                    deps,
                    backend.settings_repo(),
                    Arc::new(OverlayKindRegistry::new()),
                    Arc::new(AtomicU64::new(0)),
                    selected,
                    cx,
                );
                editor.show_detail(detail);
                editor
            })
        })
    }

    fn running_test(
        cx: &mut gpui::TestAppContext,
        rig: &WatchedRig,
        editor: &Entity<ActionEditor>,
    ) -> Entity<TestRunModal> {
        editor.update(cx, |editor, cx| editor.start_test_run(cx));
        let modal = editor
            .read_with(cx, |editor, _| editor.test_run_modal())
            .expect("the test run opens");
        rig.settle_until_step_runs(cx, &modal);
        modal
    }

    fn toast_kinds(cx: &mut gpui::TestAppContext) -> Vec<ToastKind> {
        toasts(cx).into_iter().map(|(kind, _)| kind).collect()
    }

    #[gpui::test]
    fn running_the_test_again_mid_run_opens_a_new_run_and_reports_the_old_one_in_a_toast(
        cx: &mut gpui::TestAppContext,
    ) {
        let rig = WatchedRig::new(cx);
        let editor = editor_over(cx, &rig);
        let first = running_test(cx, &rig, &editor);

        first.update(cx, press_run_again);
        let second = editor
            .read_with(cx, |editor, _| editor.test_run_modal())
            .expect("the relaunched test run is showing");
        rig.open_gate();
        rig.settle_until(cx, |cx| !toasts(cx).is_empty());

        assert_eq!(
            (second.entity_id() != first.entity_id(), toast_kinds(cx)),
            (true, vec![ToastKind::Success])
        );
    }

    #[gpui::test]
    fn leaving_the_editor_mid_run_reports_the_test_end_in_a_toast(cx: &mut gpui::TestAppContext) {
        let rig = WatchedRig::new(cx);
        let editor = editor_over(cx, &rig);
        running_test(cx, &rig, &editor);

        drop(editor);
        cx.run_until_parked();
        rig.open_gate();
        rig.settle_until(cx, |cx| !toasts(cx).is_empty());

        assert_eq!(toast_kinds(cx), vec![ToastKind::Success]);
    }
}
