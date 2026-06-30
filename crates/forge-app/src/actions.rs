use forge_events::{Event, EventSource};
use forge_storage::ActionTelemetry;
use forge_types::ActionId;
use iced::Task;
use std::sync::Arc;

use crate::Message;
use crate::Screen;
use crate::message::{ActionEditorMsg, ActionsMsg, ToastMsg};
use crate::runtime_view::RuntimeView;
use crate::test_trigger::synthesize_test_event;
use crate::triggers_registry::TriggersRegistryMsg;

pub use forge_runtime::actions::{ActionDetail, ActionSummary};

pub use crate::actions_forms::{
    AddActionForm, AddActionMsg, AddSubActionForm, AddSubActionMsg, RemoveSubActionMsg,
    SubActionFormStep,
};
pub use crate::actions_telemetry::{action_stat, format_relative_time, telemetry_grid};
pub use crate::actions_trigger_kinds::{
    ActionsFilter, TriggerCategory, category_of, trigger_label_of,
};

#[derive(Debug, Clone)]
pub struct ActionsGroup {
    pub category: TriggerCategory,
    pub fired_24h: u32,
    pub actions: Vec<ActionSummary>,
}

pub fn group_summaries(summaries: Vec<ActionSummary>) -> Vec<ActionsGroup> {
    let mut by_category: std::collections::BTreeMap<TriggerCategory, Vec<ActionSummary>> =
        std::collections::BTreeMap::new();
    for summary in summaries {
        let category = summary
            .first_trigger_kind_id
            .as_deref()
            .map(category_of)
            .unwrap_or(TriggerCategory::Ungrouped);
        by_category.entry(category).or_default().push(summary);
    }
    by_category
        .into_iter()
        .map(|(category, mut actions)| {
            actions.sort_by_key(|a| a.name.to_lowercase());
            let fired_24h = actions.iter().map(|a| a.runs_24h).sum();
            ActionsGroup {
                category,
                fired_24h,
                actions,
            }
        })
        .collect()
}

#[derive(Default)]
pub struct ActionsState {
    pub tree: Vec<ActionsGroup>,
    pub selected: Option<ActionId>,
    pub detail: Option<ActionDetail>,
    pub loading: bool,
    pub search: String,
    pub filter: ActionsFilter,
    pub collapsed_groups: std::collections::HashSet<TriggerCategory>,
    pub add_action_modal: Option<AddActionForm>,
    pub trigger_picker: Option<crate::actions_trigger_picker::TriggerPickerState>,
    pub add_sub_action_modal: Option<AddSubActionForm>,
    pub telemetry: Option<ActionTelemetry>,
    pub telemetry_loading: bool,
    pub step_menu_open: Option<usize>,
    pub action_menu_open: Option<forge_types::ActionId>,
    pub renaming_action: Option<(forge_types::ActionId, String)>,
    pub last_selected_action: Option<(forge_types::ActionId, std::time::Instant)>,
    /// Drill-in path into nested sub-chains; empty = the action's own steps.
    pub nav_path: Vec<crate::actions_nav::NavFrame>,
    /// Buffered switch-case match edits keyed by (step index, case index),
    /// committed on submit so each keystroke is not a separate DB write.
    pub case_match_edits: std::collections::HashMap<(usize, usize), String>,
}

impl ActionsState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn total_actions(&self) -> usize {
        self.tree.iter().map(|g| g.actions.len()).sum()
    }

    pub fn visible_actions(&self) -> usize {
        self.tree
            .iter()
            .flat_map(|g| g.actions.iter())
            .filter(|a| self.action_passes_filter(a))
            .count()
    }

    pub fn action_passes_filter(&self, summary: &ActionSummary) -> bool {
        let category = summary
            .first_trigger_kind_id
            .as_deref()
            .map(category_of)
            .unwrap_or(TriggerCategory::Ungrouped);
        let filter_ok = match self.filter {
            ActionsFilter::All => true,
            ActionsFilter::Chat => category == TriggerCategory::Chat,
            ActionsFilter::Timers => category == TriggerCategory::Timer,
            ActionsFilter::Points => false,
        };
        let search_ok = if self.search.is_empty() {
            true
        } else {
            let q = self.search.to_lowercase();
            let label = summary
                .first_trigger_kind_id
                .as_deref()
                .map(trigger_label_of)
                .unwrap_or_default();
            summary.name.to_lowercase().contains(&q)
                || label.to_lowercase().contains(&q)
                || summary.queue_name.to_lowercase().contains(&q)
        };
        filter_ok && search_ok
    }
}

pub fn action_rename_input_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("forge:action_rename")
}

pub fn update(state: &mut ActionsState, rt: &RuntimeView, msg: ActionsMsg) -> Task<Message> {
    match msg {
        ActionsMsg::LoadRequested => {
            state.loading = true;
            let service = Arc::clone(&rt.actions);
            Task::perform(
                async move { service.list_summaries().await.map_err(|e| e.to_string()) },
                |r| Message::Actions(ActionsMsg::SummariesLoaded(r)),
            )
        }
        ActionsMsg::SummariesLoaded(Ok(summaries)) => {
            state.tree = group_summaries(summaries);
            state.loading = false;
            Task::none()
        }
        ActionsMsg::SummariesLoaded(Err(e)) => {
            state.loading = false;
            tracing::warn!(error = %e, "actions summaries load failed");
            Task::none()
        }
        ActionsMsg::ActionSelected(id) => {
            let already_loaded = state.selected == Some(id)
                && state
                    .detail
                    .as_ref()
                    .map(|d| d.action.id == id)
                    .unwrap_or(false);
            if already_loaded {
                return Task::none();
            }
            if state.selected != Some(id) {
                // Switching to a different action: nested navigation is per-action.
                state.nav_path.clear();
                state.case_match_edits.clear();
            }
            state.selected = Some(id);
            state.detail = None;
            state.telemetry = None;
            state.telemetry_loading = true;
            let service_detail = Arc::clone(&rt.actions);
            let service_tele = Arc::clone(&rt.actions);
            let detail_task = Task::perform(
                async move {
                    service_detail
                        .load_detail(id)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::Actions(ActionsMsg::DetailLoaded(r)),
            );
            let telemetry_task = Task::perform(
                async move {
                    service_tele
                        .load_telemetry(id)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::Actions(ActionsMsg::TelemetryLoaded(r)),
            );
            Task::batch([detail_task, telemetry_task])
        }
        ActionsMsg::DetailLoaded(Ok(detail)) => {
            state.detail = Some(detail);
            seed_case_buffer(state);
            Task::none()
        }
        ActionsMsg::DetailLoaded(Err(e)) => {
            state.detail = None;
            tracing::warn!(error = %e, "action detail load failed");
            Task::none()
        }
        ActionsMsg::ToggleEnabled(id, enabled) => {
            if let Some(detail) = state.detail.as_mut()
                && detail.action.id == id
            {
                detail.action.enabled = enabled;
            }
            for group in &mut state.tree {
                for summary in &mut group.actions {
                    if summary.id == id {
                        summary.enabled = enabled;
                    }
                }
            }
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    let Some(mut action) =
                        dp.action_repo().get(id).await.map_err(|e| e.to_string())?
                    else {
                        return Err("action not found".to_string());
                    };
                    action.enabled = enabled;
                    dp.action_repo()
                        .save(&action)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::Actions(ActionsMsg::EnabledToggled(r)),
            )
        }
        ActionsMsg::EnabledToggled(Ok(())) => Task::none(),
        ActionsMsg::EnabledToggled(Err(e)) => {
            tracing::warn!(error = %e, "toggle enabled persist failed");
            Task::none()
        }
        ActionsMsg::TestTrigger(id) => {
            let bus = Arc::clone(&rt.bus);
            let service = Arc::clone(&rt.actions);
            Task::perform(
                async move {
                    let detail = service.load_detail(id).await.map_err(|e| e.to_string())?;
                    let event = match detail.trigger_instances.first() {
                        Some(instance) => synthesize_test_event(instance),
                        None => Event::new(
                            EventSource::Core,
                            "test.trigger",
                            serde_json::json!({ "action_id": id.to_string() }),
                        ),
                    };
                    let event_id = event.id;
                    bus.publish(event);
                    bus.replay_and_publish(event_id)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| {
                    if let Err(e) = r {
                        tracing::warn!(error = %e, "test trigger failed");
                    }
                    Message::Noop
                },
            )
        }
        ActionsMsg::DeleteAction(id) => {
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move { dp.action_repo().delete(id).await.map_err(|e| e.to_string()) },
                |r| Message::Actions(ActionsMsg::ActionDeleted(r.map(|_| ()))),
            )
        }
        ActionsMsg::ActionDeleted(Ok(())) => {
            state.selected = None;
            state.detail = None;
            Task::done(Message::Actions(ActionsMsg::LoadRequested))
        }
        ActionsMsg::ActionDeleted(Err(e)) => {
            tracing::warn!(error = %e, "delete action failed");
            Task::none()
        }
        ActionsMsg::DuplicateAction(id) => {
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    let original = dp
                        .action_repo()
                        .get(id)
                        .await
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| "source action not found".to_string())?;
                    let mut copy = original.clone();
                    copy.id = forge_types::ActionId::new();
                    copy.name = format!("{} (copy)", original.name);
                    dp.action_repo()
                        .save(&copy)
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(copy.id)
                },
                |r| Message::Actions(ActionsMsg::ActionDuplicated(r)),
            )
        }
        ActionsMsg::ActionDuplicated(Ok(new_id)) => {
            tracing::info!(action_id = %new_id, "action duplicated");
            Task::done(Message::Actions(ActionsMsg::LoadRequested))
        }
        ActionsMsg::ActionDuplicated(Err(e)) => {
            tracing::warn!(error = %e, "duplicate action failed");
            Task::none()
        }
        ActionsMsg::RemoveTriggerInstance(action_id, instance_id) => {
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    dp.trigger_instance_repo()
                        .unlink_action(action_id, instance_id)
                        .await
                        .map(|_| action_id)
                        .map_err(|e| e.to_string())
                },
                |r| Message::Actions(ActionsMsg::TriggerInstanceRemoved(r)),
            )
        }
        ActionsMsg::TriggerInstanceRemoved(Ok(action_id)) => {
            state.detail = None;
            state.selected = None;
            Task::done(Message::Actions(ActionsMsg::ActionSelected(action_id)))
        }
        ActionsMsg::TriggerInstanceRemoved(Err(e)) => {
            tracing::warn!(error = %e, "unlink trigger instance failed");
            Task::none()
        }
        ActionsMsg::OpenAddActionModal => Task::done(Message::Actions(ActionsMsg::Editor(
            ActionEditorMsg::AddAction(AddActionMsg::OpenRequested),
        ))),
        ActionsMsg::OpenTriggerPicker(action_id) => {
            let existing_count = state
                .detail
                .as_ref()
                .filter(|d| d.action.id == action_id)
                .map(|d| d.trigger_instances.len() as i64)
                .unwrap_or(0);
            state.trigger_picker = Some(crate::actions_trigger_picker::TriggerPickerState {
                action_id,
                next_position: existing_count,
                level1: None,
                level2: None,
                available_instances: Vec::new(),
                is_loading: true,
            });
            let dp = Arc::clone(&rt.backend);
            let descriptor_infos: Vec<(String, String, String)> = rt
                .trigger_registry
                .all()
                .map(|d| {
                    let sub_label = crate::actions_trigger_picker::sub_group_label_for(d.id());
                    (d.id().to_owned(), d.label().to_owned(), sub_label)
                })
                .collect();
            Task::perform(
                async move {
                    let all_instances = dp
                        .trigger_instance_repo()
                        .list_all()
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(crate::actions_trigger_picker::build_picker_entries(
                        descriptor_infos,
                        all_instances,
                    ))
                },
                |r| {
                    Message::Actions(ActionsMsg::TriggerPickerMsg(
                        crate::actions_trigger_picker::TriggerPickerMsg::InstancesLoaded(r),
                    ))
                },
            )
        }
        ActionsMsg::TriggerPickerMsg(msg) => {
            crate::actions_trigger_picker::update(&mut state.trigger_picker, rt, msg)
        }
        ActionsMsg::TriggerInstanceAssigned(Ok(action_id)) => {
            state.trigger_picker = None;
            state.detail = None;
            state.selected = None;
            Task::done(Message::Actions(ActionsMsg::ActionSelected(action_id)))
        }
        ActionsMsg::TriggerInstanceAssigned(Err(e)) => {
            state.trigger_picker = None;
            Task::done(Message::Toast(crate::message::ToastMsg::Fired {
                kind: forge_widgets::ToastKind::Error,
                message: e,
                duration_ms: 3000,
            }))
        }
        ActionsMsg::TriggerChipClicked(instance_id) => {
            let is_default = state
                .detail
                .as_ref()
                .and_then(|d| d.trigger_instances.iter().find(|i| i.id == instance_id))
                .map(|i| !i.user_defined)
                .unwrap_or(false);
            if is_default {
                Task::done(Message::Toast(crate::message::ToastMsg::Fired {
                    kind: forge_widgets::ToastKind::Info,
                    message: "Default trigger \u{2014} read-only. Create a Custom instance to override values.".to_owned(),
                    duration_ms: 4000,
                }))
            } else {
                Task::batch([
                    Task::done(Message::Navigate(Screen::TriggersRegistry)),
                    Task::done(Message::TriggersRegistry(TriggersRegistryMsg::ScrollTo(
                        instance_id,
                    ))),
                ])
            }
        }
        ActionsMsg::SearchChanged(q) => {
            state.search = q;
            Task::none()
        }
        ActionsMsg::FilterChanged(f) => {
            state.filter = f;
            Task::none()
        }
        ActionsMsg::ToggleGroupCollapsed(cat) => {
            if state.collapsed_groups.contains(&cat) {
                state.collapsed_groups.remove(&cat);
            } else {
                state.collapsed_groups.insert(cat);
            }
            Task::none()
        }
        ActionsMsg::TelemetryLoaded(Ok(t)) => {
            state.telemetry = Some(t);
            state.telemetry_loading = false;
            Task::none()
        }
        ActionsMsg::TelemetryLoaded(Err(e)) => {
            state.telemetry = None;
            state.telemetry_loading = false;
            tracing::warn!(error = %e, "action telemetry load failed");
            Task::none()
        }
        ActionsMsg::ToggleStepMenu(i) => {
            state.step_menu_open = if state.step_menu_open == Some(i) {
                None
            } else {
                Some(i)
            };
            Task::none()
        }
        ActionsMsg::DismissStepMenu => {
            state.step_menu_open = None;
            Task::none()
        }
        ActionsMsg::ToggleActionMenu(id) => {
            state.action_menu_open = if state.action_menu_open == Some(id) {
                None
            } else {
                Some(id)
            };
            Task::none()
        }
        ActionsMsg::DismissActionMenu => {
            state.action_menu_open = None;
            Task::none()
        }
        ActionsMsg::RenameStarted(id) => {
            let current_name = state
                .tree
                .iter()
                .flat_map(|g| g.actions.iter())
                .find(|a| a.id == id)
                .map(|a| a.name.clone())
                .unwrap_or_default();
            state.renaming_action = Some((id, current_name));
            state.action_menu_open = None;
            iced::widget::operation::focus(action_rename_input_id())
        }
        ActionsMsg::RenameBufferChanged(buf) => {
            if let Some((_, name)) = state.renaming_action.as_mut() {
                *name = buf;
            }
            Task::none()
        }
        ActionsMsg::RenameCancel => {
            state.renaming_action = None;
            Task::none()
        }
        ActionsMsg::RenameSubmit => {
            let Some((id, name)) = state.renaming_action.clone() else {
                return Task::none();
            };
            let trimmed = name.trim().to_owned();
            if trimmed.is_empty() {
                state.renaming_action = None;
                return Task::none();
            }
            let already_taken = state
                .tree
                .iter()
                .flat_map(|g| g.actions.iter())
                .any(|a| a.id != id && a.name.eq_ignore_ascii_case(&trimmed));
            if already_taken {
                let toast_msg = format!("Name \u{201c}{trimmed}\u{201d} is already taken");
                return Task::done(Message::Toast(ToastMsg::Fired {
                    kind: forge_widgets::ToastKind::Error,
                    message: toast_msg,
                    duration_ms: 3000,
                }));
            }
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    let mut action = dp
                        .action_repo()
                        .get(id)
                        .await
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| "action not found".to_owned())?;
                    action.name = trimmed.clone();
                    dp.action_repo()
                        .save(&action)
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok::<_, String>((id, trimmed))
                },
                |r| Message::Actions(ActionsMsg::RenameSaved(r)),
            )
        }
        ActionsMsg::RenameSaved(Ok((id, new_name))) => {
            state.renaming_action = None;
            for group in &mut state.tree {
                let touched = group.actions.iter().any(|s| s.id == id);
                for summary in &mut group.actions {
                    if summary.id == id {
                        summary.name = new_name.clone();
                    }
                }
                if touched {
                    group.actions.sort_by_key(|a| a.name.to_lowercase());
                }
            }
            if let Some(detail) = state.detail.as_mut()
                && detail.action.id == id
            {
                detail.action.name = new_name;
            }
            Task::none()
        }
        ActionsMsg::RenameSaved(Err(e)) => {
            state.renaming_action = None;
            tracing::warn!(error = %e, "action rename failed");
            Task::none()
        }
        ActionsMsg::EnterBranch {
            step_index,
            chain_key,
            case_index,
        } => {
            if state.nav_path.len() >= crate::actions_nav::UI_MAX_NESTING_DEPTH {
                // At/over the authoring cap: only descend into a branch that
                // already has content (edit-but-don't-deepen). An empty branch
                // beyond the cap would only create new depth, so reject it.
                let has_content = state.detail.as_ref().is_some_and(|d| {
                    let chain =
                        crate::actions_nav::resolve_chain(&d.action.sub_actions, &state.nav_path);
                    chain.get(step_index).is_some_and(|s| {
                        crate::actions_nav::branch_step_count(s, &chain_key, case_index) > 0
                    })
                });
                if !has_content {
                    return Task::none();
                }
            }
            state.nav_path.push(crate::actions_nav::NavFrame::new(
                step_index, chain_key, case_index,
            ));
            state.case_match_edits.clear();
            seed_case_buffer(state);
            state.step_menu_open = None;
            Task::none()
        }
        ActionsMsg::BreadcrumbPop(depth) => {
            if depth < state.nav_path.len() {
                state.nav_path.truncate(depth);
            }
            state.case_match_edits.clear();
            seed_case_buffer(state);
            state.step_menu_open = None;
            Task::none()
        }
        ActionsMsg::BranchReload(id) => {
            state.detail = None;
            // Case-match buffers are re-seeded from persisted values on reload;
            // dropping them here avoids stale indices after structural case edits.
            state.case_match_edits.clear();
            Task::done(Message::Actions(ActionsMsg::ActionSelected(id)))
        }
        ActionsMsg::AddSwitchCase(step_index) => persist_switch_case_op(state, rt, move |chain| {
            if let Some(step) = chain.get_mut(step_index) {
                crate::actions_nav::append_empty_case(&mut step.config);
            }
        }),
        ActionsMsg::RemoveSwitchCase(step_index, case_index) => {
            persist_switch_case_op(state, rt, move |chain| {
                if let Some(step) = chain.get_mut(step_index) {
                    crate::actions_nav::remove_case(&mut step.config, case_index);
                }
            })
        }
        ActionsMsg::MoveSwitchCase(step_index, case_index, up) => {
            persist_switch_case_op(state, rt, move |chain| {
                if let Some(step) = chain.get_mut(step_index) {
                    crate::actions_nav::move_case(&mut step.config, case_index, up);
                }
            })
        }
        ActionsMsg::SwitchCaseMatchChanged(step_index, case_index, value) => {
            state
                .case_match_edits
                .insert((step_index, case_index), value);
            Task::none()
        }
        ActionsMsg::SwitchCaseMatchCommitted(step_index, case_index) => {
            let Some(value) = state
                .case_match_edits
                .get(&(step_index, case_index))
                .cloned()
            else {
                return Task::none();
            };
            persist_switch_case_op(state, rt, move |chain| {
                if let Some(step) = chain.get_mut(step_index) {
                    crate::actions_nav::set_case_match(&mut step.config, case_index, &value);
                }
            })
        }
        ActionsMsg::Editor(sub) => match sub {
            ActionEditorMsg::AddAction(m) => {
                crate::action_editor::add_action_update(&mut state.add_action_modal, rt, m)
            }
            ActionEditorMsg::AddSubAction(m) => {
                let task = crate::action_editor::add_sub_action_update(
                    &mut state.add_sub_action_modal,
                    rt,
                    state.detail.as_ref(),
                    &state.nav_path,
                    m,
                );
                if let Some(form) = state.add_sub_action_modal.as_mut()
                    && form.available_actions.is_empty()
                {
                    form.available_actions = state
                        .tree
                        .iter()
                        .flat_map(|g| g.actions.iter())
                        .map(|a| (a.id, a.name.clone()))
                        .collect();
                }
                task
            }
            ActionEditorMsg::RemoveSubAction(m) => crate::action_editor::remove_sub_action_update(
                state.detail.as_ref(),
                &state.nav_path,
                rt,
                m,
            ),
            ActionEditorMsg::MoveSubAction(m) => crate::action_editor::move_sub_action_update(
                state.detail.as_ref(),
                &state.nav_path,
                rt,
                m,
            ),
        },
    }
}

/// Seeds the case-match buffer with each switch case's persisted single-value
/// `match` for the chain currently in view, so the per-row inputs render against
/// owned `'a` state. Multi-value (imported) matches are skipped — they render
/// read-only and must never be clobbered by a single-value edit. Existing buffer
/// entries (in-flight edits) are preserved.
fn seed_case_buffer(state: &mut ActionsState) {
    let Some(detail) = state.detail.as_ref() else {
        return;
    };
    let chain = crate::actions_nav::resolve_chain(&detail.action.sub_actions, &state.nav_path);
    for (step_index, step) in chain.iter().enumerate() {
        for case_index in 0..crate::actions_nav::case_count(step) {
            if state
                .case_match_edits
                .contains_key(&(step_index, case_index))
            {
                continue;
            }
            if let Some(value) = crate::actions_nav::case_match_display(step, case_index) {
                state
                    .case_match_edits
                    .insert((step_index, case_index), value);
            }
        }
    }
}

/// Persists a mutation to the chain at the current nav path (where switch steps
/// live), then reloads the editor. A no-op when no action detail is loaded.
fn persist_switch_case_op(
    state: &ActionsState,
    rt: &RuntimeView,
    mutate: impl FnOnce(&mut Vec<forge_types::SubActionStep>),
) -> Task<Message> {
    let Some(action) = state.detail.as_ref().map(|d| d.action.clone()) else {
        return Task::none();
    };
    crate::actions_nav::persist_chain_mutation(rt, &action, &state.nav_path, mutate)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_runtime::actions::ActionsService;
    use forge_storage::DataProvider;
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{Action, ActionId, Queue, QueueId, SubActionStep};

    fn make_service(dp: Arc<dyn DataProvider>) -> ActionsService {
        ActionsService::new(
            dp.action_repo(),
            dp.queue_repo(),
            dp.history_repo(),
            dp.trigger_instance_repo(),
            dp.soundboard_clips_repo(),
        )
    }

    #[test]
    fn form_invalid_when_name_is_empty() {
        let mut form = AddActionForm::new();
        form.queue_id = Some(QueueId::new());
        assert!(!form.is_valid());
    }

    #[test]
    fn form_invalid_when_no_queue_selected() {
        let mut form = AddActionForm::new();
        form.name = "My action".to_string();
        assert!(!form.is_valid());
    }

    #[test]
    fn form_valid_when_name_and_queue_present() {
        let mut form = AddActionForm::new();
        form.name = "My action".to_string();
        form.queue_id = Some(QueueId::new());
        assert!(form.is_valid());
    }

    #[test]
    fn form_invalid_when_name_is_only_whitespace() {
        let mut form = AddActionForm::new();
        form.name = "   ".to_string();
        form.queue_id = Some(QueueId::new());
        assert!(!form.is_valid());
    }

    #[test]
    fn category_of_classifies_known_trigger_kinds() {
        for (kind, expected) in [
            ("twitch.chat.command", TriggerCategory::Chat),
            ("twitch.chat.message", TriggerCategory::Chat),
            ("twitch.support.subscriber", TriggerCategory::Subscriptions),
            (
                "twitch.support.resubscriber",
                TriggerCategory::Subscriptions,
            ),
            ("twitch.support.gift_sub", TriggerCategory::Subscriptions),
            ("twitch.support.cheer", TriggerCategory::Bits),
            ("twitch.channel.raid_received", TriggerCategory::Raids),
        ] {
            assert_eq!(category_of(kind), expected, "category_of({kind})");
        }
    }

    #[test]
    fn open_trigger_picker_state_is_loading() {
        let picker = crate::actions_trigger_picker::TriggerPickerState {
            action_id: ActionId::new(),
            next_position: 0,
            level1: None,
            level2: None,
            available_instances: Vec::new(),
            is_loading: true,
        };
        assert!(picker.is_loading);
        assert!(picker.available_instances.is_empty());
    }

    #[test]
    fn trigger_picker_next_position_uses_existing_trigger_count() {
        let picker = crate::actions_trigger_picker::TriggerPickerState {
            action_id: ActionId::new(),
            next_position: 3,
            level1: None,
            level2: None,
            available_instances: Vec::new(),
            is_loading: true,
        };
        assert_eq!(picker.next_position, 3);
    }

    const TEST_KEY: [u8; 32] = [0xab; 32];

    async fn open_backend() -> Arc<dyn DataProvider> {
        Arc::new(
            SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
                .await
                .unwrap(),
        )
    }

    async fn make_action(dp: &Arc<dyn DataProvider>, name: &str, group: Option<&str>) -> Action {
        let queue = Queue {
            id: QueueId::new(),
            name: "Default".to_string(),
            blocking: false,
        };
        dp.queue_repo().save(&queue).await.unwrap();
        Action {
            id: ActionId::new(),
            name: name.to_string(),
            group: group.map(str::to_string),
            queue_id: queue.id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![],
        }
    }

    #[tokio::test]
    async fn empty_db_yields_empty_tree() {
        let dp = open_backend().await;
        let tree = group_summaries(make_service(dp).list_summaries().await.unwrap());
        assert!(tree.is_empty());
    }

    #[tokio::test]
    async fn actions_without_triggers_land_in_ungrouped() {
        let dp = open_backend().await;
        let a1 = make_action(&dp, "!so", Some("Chat Commands")).await;
        let a2 = make_action(&dp, "HydrateCheck", Some("Timers")).await;
        dp.action_repo().save(&a1).await.unwrap();
        dp.action_repo().save(&a2).await.unwrap();

        let tree = group_summaries(make_service(dp).list_summaries().await.unwrap());
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].category, TriggerCategory::Ungrouped);
        assert_eq!(tree[0].actions.len(), 2);
    }

    #[tokio::test]
    async fn chat_trigger_produces_chat_group() {
        let dp = open_backend().await;
        let a = make_action(&dp, "!quote", None).await;
        dp.action_repo().save(&a).await.unwrap();
        let instance_id = dp
            .trigger_instance_repo()
            .upsert_default("twitch.chat.command", "Twitch Chat Command")
            .await
            .unwrap();
        dp.trigger_instance_repo()
            .link_action(a.id, instance_id, 0)
            .await
            .unwrap();

        let tree = group_summaries(make_service(dp).list_summaries().await.unwrap());
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].category, TriggerCategory::Chat);
    }

    #[tokio::test]
    async fn ungrouped_action_goes_into_ungrouped_group() {
        let dp = open_backend().await;
        let a = make_action(&dp, "!quote", None).await;
        dp.action_repo().save(&a).await.unwrap();

        let tree = group_summaries(make_service(dp).list_summaries().await.unwrap());
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].category, TriggerCategory::Ungrouped);
    }

    #[tokio::test]
    async fn load_action_detail_not_found_returns_error() {
        let dp = open_backend().await;
        let missing_id = ActionId::new();
        let result = make_service(dp).load_detail(missing_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn load_action_detail_found_returns_detail() {
        let dp = open_backend().await;
        let a = make_action(&dp, "!quote", Some("Chat Commands")).await;
        dp.action_repo().save(&a).await.unwrap();

        let detail = make_service(dp).load_detail(a.id).await.unwrap();
        assert_eq!(detail.action.name, "!quote");
        assert!(detail.trigger_instances.is_empty());
    }

    #[tokio::test]
    async fn linked_trigger_instance_appears_in_detail() {
        let dp = open_backend().await;
        let action = make_action(&dp, "!quote", None).await;
        dp.action_repo().save(&action).await.unwrap();

        let instance_id = dp
            .trigger_instance_repo()
            .upsert_default("twitch.chat.command", "Twitch Chat Command")
            .await
            .unwrap();
        dp.trigger_instance_repo()
            .link_action(action.id, instance_id, 0)
            .await
            .unwrap();

        let detail = make_service(dp).load_detail(action.id).await.unwrap();
        assert_eq!(detail.trigger_instances.len(), 1);
        assert_eq!(detail.trigger_instances[0].kind_id, "twitch.chat.command");
    }

    #[tokio::test]
    async fn non_command_trigger_instance_appears_in_detail() {
        let dp = open_backend().await;
        let action = make_action(&dp, "sub alert", None).await;
        dp.action_repo().save(&action).await.unwrap();

        let instance_id = dp
            .trigger_instance_repo()
            .upsert_default("twitch.support.subscriber", "Twitch Subscriber")
            .await
            .unwrap();
        dp.trigger_instance_repo()
            .link_action(action.id, instance_id, 0)
            .await
            .unwrap();

        let detail = make_service(dp).load_detail(action.id).await.unwrap();
        assert_eq!(detail.trigger_instances.len(), 1);
    }

    #[tokio::test]
    async fn load_clip_options_empty_db_returns_empty() {
        let dp = open_backend().await;
        let clips = make_service(dp).list_clip_options().await;
        assert!(clips.is_empty());
    }

    #[tokio::test]
    async fn save_sub_action_appends_send_chat() {
        use forge_types::Variant;
        let dp = open_backend().await;
        let action = make_action(&dp, "say hello", None).await;
        dp.action_repo().save(&action).await.unwrap();

        let step = SubActionStep {
            kind_id: "twitch.chat.send_message".to_owned(),
            config: std::collections::BTreeMap::from([
                (
                    "message".to_owned(),
                    Variant::String("Hello %user%!".to_owned()),
                ),
                ("target".to_owned(), Variant::String("twitch".to_owned())),
            ]),
            enabled: true,
            label: None,
        };
        make_service(Arc::clone(&dp))
            .save_sub_action(action.id, step, None)
            .await
            .unwrap();

        let loaded = dp.action_repo().get(action.id).await.unwrap().unwrap();
        assert_eq!(loaded.sub_actions.len(), 1);
        assert_eq!(loaded.sub_actions[0].kind_id, "twitch.chat.send_message");
    }

    #[tokio::test]
    async fn save_sub_action_appends_set_global() {
        use forge_types::Variant;
        let dp = open_backend().await;
        let action = make_action(&dp, "track", None).await;
        dp.action_repo().save(&action).await.unwrap();

        let step = SubActionStep {
            kind_id: "core.globals.set".to_owned(),
            config: std::collections::BTreeMap::from([
                ("name".to_owned(), Variant::String("counter".to_owned())),
                ("value".to_owned(), Variant::String("1".to_owned())),
            ]),
            enabled: true,
            label: None,
        };
        make_service(Arc::clone(&dp))
            .save_sub_action(action.id, step, None)
            .await
            .unwrap();

        let loaded = dp.action_repo().get(action.id).await.unwrap().unwrap();
        assert_eq!(loaded.sub_actions[0].kind_id, "core.globals.set");
    }

    #[tokio::test]
    async fn save_sub_action_delay_stores_ms_correctly() {
        use forge_types::Variant;
        let dp = open_backend().await;
        let action = make_action(&dp, "pause", None).await;
        dp.action_repo().save(&action).await.unwrap();

        let step = SubActionStep {
            kind_id: "core.logic.wait".to_owned(),
            config: std::collections::BTreeMap::from([("ms".to_owned(), Variant::Int(500))]),
            enabled: true,
            label: None,
        };
        make_service(Arc::clone(&dp))
            .save_sub_action(action.id, step, None)
            .await
            .unwrap();

        let loaded = dp.action_repo().get(action.id).await.unwrap().unwrap();
        assert_eq!(loaded.sub_actions.len(), 1);
        assert_eq!(loaded.sub_actions[0].kind_id, "core.logic.wait");
    }

    #[tokio::test]
    async fn remove_sub_action_removes_at_valid_index() {
        use forge_types::Variant;
        let dp = open_backend().await;
        let mut action = make_action(&dp, "multi", None).await;
        action.sub_actions = vec![
            SubActionStep {
                kind_id: "core.logic.wait".to_owned(),
                config: std::collections::BTreeMap::from([("ms".to_owned(), Variant::Int(100))]),
                enabled: true,
                label: None,
            },
            SubActionStep {
                kind_id: "core.log.write".to_owned(),
                config: std::collections::BTreeMap::from([
                    ("level".to_owned(), Variant::String("info".to_owned())),
                    ("message".to_owned(), Variant::String("done".to_owned())),
                ]),
                enabled: true,
                label: None,
            },
        ];
        dp.action_repo().save(&action).await.unwrap();

        make_service(Arc::clone(&dp))
            .remove_sub_action(action.id, 0)
            .await
            .unwrap();

        let loaded = dp.action_repo().get(action.id).await.unwrap().unwrap();
        assert_eq!(loaded.sub_actions.len(), 1);
        assert_eq!(loaded.sub_actions[0].kind_id, "core.log.write");
    }

    #[tokio::test]
    async fn remove_sub_action_out_of_range_leaves_action_unchanged() {
        use forge_types::Variant;
        let dp = open_backend().await;
        let mut action = make_action(&dp, "single", None).await;
        action.sub_actions = vec![SubActionStep {
            kind_id: "core.logic.wait".to_owned(),
            config: std::collections::BTreeMap::from([("ms".to_owned(), Variant::Int(250))]),
            enabled: true,
            label: None,
        }];
        dp.action_repo().save(&action).await.unwrap();

        make_service(Arc::clone(&dp))
            .remove_sub_action(action.id, 99)
            .await
            .unwrap();

        let loaded = dp.action_repo().get(action.id).await.unwrap().unwrap();
        assert_eq!(loaded.sub_actions.len(), 1);
    }
}
