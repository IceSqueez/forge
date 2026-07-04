use std::sync::Arc;

use forge_types::{Action, ActionId};
use iced::Task;

use crate::actions::{
    ActionDetail, AddActionForm, AddActionMsg, AddSubActionForm, AddSubActionMsg,
    RemoveSubActionMsg, SubActionFormStep,
};
use crate::actions_field_form::{FieldEditMsg, apply_field_edit};
use crate::actions_nav::{NavFrame, persist_chain_mutation, resolve_chain};
use crate::message::{ActionEditorMsg, ActionsMsg, Message, MoveSubActionMsg};
use crate::runtime_view::RuntimeView;

pub fn add_action_update(
    state: &mut Option<AddActionForm>,
    rt: &RuntimeView,
    msg: AddActionMsg,
) -> Task<Message> {
    match msg {
        AddActionMsg::OpenRequested => {
            *state = Some(AddActionForm::new());
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    dp.queue_repo()
                        .list()
                        .await
                        .map(|qs| qs.into_iter().map(|q| (q.id, q.name)).collect())
                        .map_err(|e| e.to_string())
                },
                |r| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                        AddActionMsg::QueueOptionsLoaded(r),
                    )))
                },
            )
        }
        AddActionMsg::QueueOptionsLoaded(Ok(opts)) => {
            if let Some(form) = state.as_mut() {
                form.set_queue_options(opts);
            }
            Task::none()
        }
        AddActionMsg::QueueOptionsLoaded(Err(e)) => {
            if let Some(form) = state.as_mut() {
                form.error = Some(e);
            }
            Task::none()
        }
        AddActionMsg::NameChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.name = v;
            }
            Task::none()
        }
        AddActionMsg::GroupChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.group = v;
            }
            Task::none()
        }
        AddActionMsg::QueueSelected(name) => {
            if let Some(f) = state.as_mut() {
                f.select_queue_by_name(name);
            }
            Task::none()
        }
        AddActionMsg::DescriptionChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.description = v;
            }
            Task::none()
        }
        AddActionMsg::EnabledToggled(v) => {
            if let Some(f) = state.as_mut() {
                f.enabled = v;
            }
            Task::none()
        }
        AddActionMsg::ConcurrentToggled(v) => {
            if let Some(f) = state.as_mut() {
                f.concurrent = v;
            }
            Task::none()
        }
        AddActionMsg::BypassPauseToggled(v) => {
            if let Some(f) = state.as_mut() {
                f.bypass_pause = v;
            }
            Task::none()
        }
        AddActionMsg::RandomPickToggled(v) => {
            if let Some(f) = state.as_mut() {
                f.random_pick = v;
            }
            Task::none()
        }
        AddActionMsg::Cancel => {
            *state = None;
            Task::none()
        }
        AddActionMsg::Submit => {
            let Some(form) = state.as_ref() else {
                return Task::none();
            };
            if !form.is_valid() {
                return Task::none();
            }
            let Some(queue_id) = form.queue_id else {
                return Task::none();
            };
            let action = Action {
                id: ActionId::new(),
                name: form.name.trim().to_string(),
                group: if form.group.trim().is_empty() {
                    None
                } else {
                    Some(form.group.trim().to_string())
                },
                queue_id,
                enabled: form.enabled,
                concurrent: form.concurrent,
                bypass_pause: form.bypass_pause,
                execution_mode: if form.random_pick {
                    forge_types::ExecutionMode::RandomPick
                } else {
                    forge_types::ExecutionMode::Sequential
                },
                description: if form.description.trim().is_empty() {
                    None
                } else {
                    Some(form.description.trim().to_string())
                },
                sub_actions: vec![],
            };
            if let Some(f) = state.as_mut() {
                f.saving = true;
            }
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    dp.action_repo()
                        .save(&action)
                        .await
                        .map(|_| action.id)
                        .map_err(|e| e.to_string())
                },
                |r| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                        AddActionMsg::Saved(r),
                    )))
                },
            )
        }
        AddActionMsg::Saved(Ok(id)) => {
            *state = None;
            let load = Task::done(Message::Actions(ActionsMsg::LoadRequested));
            let select = Task::done(Message::Actions(ActionsMsg::ActionSelected(id)));
            load.chain(select)
        }
        AddActionMsg::Saved(Err(e)) => {
            if let Some(f) = state.as_mut() {
                f.saving = false;
                f.error = Some(e);
            }
            Task::none()
        }
    }
}

fn load_clips_task(rt: &RuntimeView) -> Task<Message> {
    let service = Arc::clone(&rt.actions);
    Task::perform(async move { service.list_clip_options().await }, |clips| {
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::ClipsLoaded(clips),
        )))
    })
}

fn load_queues_task(rt: &RuntimeView) -> Task<Message> {
    let dp = Arc::clone(&rt.backend);
    Task::perform(
        async move {
            dp.queue_repo()
                .list()
                .await
                .map(|qs| qs.into_iter().map(|q| (q.id, q.name)).collect::<Vec<_>>())
                .unwrap_or_default()
        },
        |opts| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::QueuesLoaded(opts),
            )))
        },
    )
}

fn load_trigger_instances_task(rt: &RuntimeView) -> Task<Message> {
    let dp = Arc::clone(&rt.backend);
    Task::perform(
        async move {
            dp.trigger_instance_repo()
                .list_all()
                .await
                .map(|instances| {
                    instances
                        .into_iter()
                        .map(|i| (i.id, i.name))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        },
        |opts| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::TriggerInstancesLoaded(opts),
            )))
        },
    )
}

fn load_scripts_task(rt: &RuntimeView) -> Task<Message> {
    let dp = Arc::clone(&rt.backend);
    Task::perform(
        async move {
            dp.list_enabled()
                .await
                .map(|records| records.into_iter().map(|r| r.name).collect::<Vec<_>>())
                .unwrap_or_default()
        },
        |names| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::ScriptNamesLoaded(names),
            )))
        },
    )
}

fn open_form_tasks(rt: &RuntimeView) -> Task<Message> {
    Task::batch([
        load_clips_task(rt),
        load_queues_task(rt),
        load_trigger_instances_task(rt),
        load_scripts_task(rt),
    ])
}

pub fn add_sub_action_update(
    state: &mut Option<AddSubActionForm>,
    rt: &RuntimeView,
    detail: Option<&ActionDetail>,
    nav_path: &[NavFrame],
    msg: AddSubActionMsg,
) -> Task<Message> {
    match msg {
        AddSubActionMsg::OpenRequested(action_id) => {
            *state = Some(AddSubActionForm::new(action_id));
            open_form_tasks(rt)
        }
        AddSubActionMsg::EditRequested(action_id, index) => {
            let mut form = AddSubActionForm::new(action_id);
            form.editing_index = Some(index);
            // The edited row lives in the chain the nav path currently points at,
            // which is the top-level chain when the path is empty.
            if let Some(d) = detail
                && d.action.id == action_id
                && let Some(step) = resolve_chain(&d.action.sub_actions, nav_path).get(index)
            {
                form.populate_from_step(step);
            }
            *state = Some(form);
            open_form_tasks(rt)
        }
        AddSubActionMsg::KindSelected(kind_id) => {
            if let Some(f) = state.as_mut() {
                let default_config = rt
                    .sub_action_registry
                    .get(&kind_id)
                    .map(|r| r.default_config())
                    .unwrap_or_default();
                f.selected_kind_id = Some(kind_id);
                f.seed_from_default(default_config);
                f.step = SubActionFormStep::FillForm;
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::BackToKindPicker => {
            if let Some(f) = state.as_mut() {
                f.step = SubActionFormStep::PickKind;
                f.selected_kind_id = None;
                f.overrides_buffer.clear();
                f.text_buffer.clear();
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::SearchChanged(s) => {
            if let Some(f) = state.as_mut() {
                f.search = s;
            }
            Task::none()
        }
        AddSubActionMsg::FieldChanged(key, variant) => {
            if let Some(f) = state.as_mut() {
                apply_field_edit(
                    &mut f.text_buffer,
                    &mut f.overrides_buffer,
                    FieldEditMsg::Set(key, variant),
                );
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::IntInputChanged(key, raw) => {
            if let Some(f) = state.as_mut() {
                apply_field_edit(
                    &mut f.text_buffer,
                    &mut f.overrides_buffer,
                    FieldEditMsg::IntInput(key, raw),
                );
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::FieldCleared(key) => {
            if let Some(f) = state.as_mut() {
                apply_field_edit(
                    &mut f.text_buffer,
                    &mut f.overrides_buffer,
                    FieldEditMsg::Clear(key),
                );
            }
            Task::none()
        }
        AddSubActionMsg::ClipsLoaded(clips) => {
            if let Some(f) = state.as_mut() {
                f.available_clips = clips;
            }
            Task::none()
        }
        AddSubActionMsg::QueuesLoaded(queues) => {
            if let Some(f) = state.as_mut() {
                f.available_queues = queues;
            }
            Task::none()
        }
        AddSubActionMsg::TriggerInstancesLoaded(instances) => {
            if let Some(f) = state.as_mut() {
                f.available_trigger_instances = instances;
            }
            Task::none()
        }
        AddSubActionMsg::ScriptNamesLoaded(names) => {
            if let Some(f) = state.as_mut() {
                f.available_scripts = names;
            }
            Task::none()
        }
        AddSubActionMsg::Cancel => {
            *state = None;
            Task::none()
        }
        AddSubActionMsg::Submit => {
            let Some(form) = state.as_ref() else {
                return Task::none();
            };
            let Some(kind_id) = form.selected_kind_id.clone() else {
                return Task::none();
            };
            if let Some(runner) = rt.sub_action_registry.get(&kind_id)
                && let Err(e) = runner.validate_config(&form.overrides_buffer)
            {
                if let Some(f) = state.as_mut() {
                    f.error = Some(e.to_string());
                }
                return Task::none();
            }
            let Some(step) = form.build_step() else {
                return Task::none();
            };
            let editing_index = form.editing_index;
            let Some(action) = detail.map(|d| d.action.clone()) else {
                return Task::none();
            };
            // Insert/replace the step in the chain the nav path points at, then
            // persist the whole action (round-trips nested chains untouched).
            let task =
                persist_chain_mutation(rt, &action, nav_path, move |chain| match editing_index {
                    Some(idx) if idx < chain.len() => chain[idx] = step,
                    _ => chain.push(step),
                });
            *state = None;
            task
        }
        AddSubActionMsg::Saved(Ok(())) => {
            let for_action_id = state.as_ref().map(|f| f.for_action_id);
            *state = None;
            match for_action_id {
                Some(id) => Task::done(Message::Actions(ActionsMsg::ActionSelected(id))),
                None => Task::none(),
            }
        }
        AddSubActionMsg::Saved(Err(e)) => {
            if let Some(f) = state.as_mut() {
                f.saving = false;
                f.error = Some(e);
            }
            Task::none()
        }
        AddSubActionMsg::DuplicateRequested(_action_id, index) => {
            let Some(action) = detail.map(|d| d.action.clone()) else {
                return Task::none();
            };
            persist_chain_mutation(rt, &action, nav_path, move |chain| {
                if index < chain.len() {
                    let copy = chain[index].clone();
                    chain.insert(index + 1, copy);
                }
            })
        }
        AddSubActionMsg::Duplicated(Ok(id)) => {
            Task::done(Message::Actions(ActionsMsg::ActionSelected(id)))
        }
        AddSubActionMsg::Duplicated(Err(e)) => {
            tracing::warn!(error = %e, "duplicate sub-action failed");
            Task::none()
        }
    }
}

pub fn remove_sub_action_update(
    detail: Option<&ActionDetail>,
    nav_path: &[NavFrame],
    rt: &RuntimeView,
    msg: RemoveSubActionMsg,
) -> Task<Message> {
    match msg {
        RemoveSubActionMsg::ConfirmAccepted(_action_id, index) => {
            let Some(action) = detail.map(|d| d.action.clone()) else {
                return Task::none();
            };
            persist_chain_mutation(rt, &action, nav_path, move |chain| {
                if index < chain.len() {
                    chain.remove(index);
                }
            })
        }
        RemoveSubActionMsg::Removed(Ok(())) => Task::none(),
        RemoveSubActionMsg::Removed(Err(e)) => {
            tracing::warn!(error = %e, "remove sub-action persist failed");
            Task::none()
        }
        // `Requested` (arm) and `ConfirmDismissed` are intercepted in
        // `actions::update` before reaching this fn — they only mutate
        // `ActionsState.pending_delete`, which this fn has no access to.
        RemoveSubActionMsg::Requested(..) | RemoveSubActionMsg::ConfirmDismissed => Task::none(),
    }
}

pub fn move_sub_action_update(
    detail: Option<&ActionDetail>,
    nav_path: &[NavFrame],
    rt: &RuntimeView,
    msg: MoveSubActionMsg,
) -> Task<Message> {
    let (index, to_bottom, to_top) = match msg {
        MoveSubActionMsg::Up(_, i) => (i.checked_sub(1).map(|t| (i, t)), false, false),
        MoveSubActionMsg::Down(_, i) => (Some((i, i + 1)), false, false),
        MoveSubActionMsg::ToTop(_, i) => (Some((i, 0)), false, true),
        MoveSubActionMsg::ToBottom(_, i) => (Some((i, 0)), true, false),
        MoveSubActionMsg::Moved(Ok(id)) => {
            return Task::done(Message::Actions(ActionsMsg::ActionSelected(id)));
        }
        MoveSubActionMsg::Moved(Err(e)) => {
            tracing::warn!(error = %e, "move sub-action failed");
            return Task::none();
        }
    };
    let Some((from, to)) = index else {
        return Task::none();
    };
    let Some(action) = detail.map(|d| d.action.clone()) else {
        return Task::none();
    };
    persist_chain_mutation(rt, &action, nav_path, move |chain| {
        let len = chain.len();
        if from >= len {
            return;
        }
        let target = if to_bottom {
            len - 1
        } else if to_top {
            0
        } else if to < len {
            to
        } else {
            return;
        };
        let item = chain.remove(from);
        chain.insert(target, item);
    })
}
