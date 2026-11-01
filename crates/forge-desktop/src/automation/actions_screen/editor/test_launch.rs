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
                (Some(_), Some((variables, contract))) => (
                    synthesize_args(variables, &SynthesisSample::random(contract)),
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
        _modal: Entity<TestRunModal>,
        event: &TestRunEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TestRunEvent::Relaunch {
                action_id,
                selected,
            } => self.launch_test_run(*action_id, *selected, cx),
            TestRunEvent::DetailChanged(id) => self.load_detail_for(*id, cx),
            TestRunEvent::Cancelled(id) => {
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
