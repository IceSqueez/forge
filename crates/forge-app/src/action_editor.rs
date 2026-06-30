use std::sync::Arc;

use forge_types::{Action, ActionId};
use iced::Task;

use crate::actions::{
    ActionDetail, AddActionForm, AddActionMsg, AddSubActionForm, AddSubActionMsg,
    RemoveSubActionMsg, SubActionFormStep,
};
use crate::actions_field_form::{FieldEditMsg, apply_field_edit};
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
            if let Some(d) = detail
                && d.action.id == action_id
                && let Some(step) = d.action.sub_actions.get(index)
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
            let action_id = form.for_action_id;
            let editing_index = form.editing_index;
            if let Some(f) = state.as_mut() {
                f.saving = true;
            }
            let service = Arc::clone(&rt.actions);
            Task::perform(
                async move {
                    service
                        .save_sub_action(action_id, step, editing_index)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::Saved(r),
                    )))
                },
            )
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
        AddSubActionMsg::DuplicateRequested(action_id, index) => {
            let service = Arc::clone(&rt.actions);
            Task::perform(
                async move {
                    service
                        .duplicate_sub_action(action_id, index)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::Duplicated(r),
                    )))
                },
            )
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
    selected: Option<ActionId>,
    rt: &RuntimeView,
    msg: RemoveSubActionMsg,
) -> Task<Message> {
    match msg {
        RemoveSubActionMsg::Requested(action_id, index) => {
            let service = Arc::clone(&rt.actions);
            Task::perform(
                async move {
                    service
                        .remove_sub_action(action_id, index)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::RemoveSubAction(
                        RemoveSubActionMsg::Removed(r),
                    )))
                },
            )
        }
        RemoveSubActionMsg::Removed(Ok(())) => match selected {
            Some(id) => Task::done(Message::Actions(ActionsMsg::ActionSelected(id))),
            None => Task::none(),
        },
        RemoveSubActionMsg::Removed(Err(e)) => {
            tracing::warn!(error = %e, "remove sub-action persist failed");
            Task::none()
        }
    }
}

pub fn move_sub_action_update(
    rt: &RuntimeView,
    total: usize,
    msg: MoveSubActionMsg,
) -> Task<Message> {
    match msg {
        MoveSubActionMsg::Up(action_id, i) => {
            if i == 0 {
                return Task::none();
            }
            let service = Arc::clone(&rt.actions);
            Task::perform(
                async move {
                    service
                        .move_sub_action(action_id, i, i - 1)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::MoveSubAction(
                        MoveSubActionMsg::Moved(r),
                    )))
                },
            )
        }
        MoveSubActionMsg::Down(action_id, i) => {
            if total == 0 || i + 1 >= total {
                return Task::none();
            }
            let service = Arc::clone(&rt.actions);
            Task::perform(
                async move {
                    service
                        .move_sub_action(action_id, i, i + 1)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::MoveSubAction(
                        MoveSubActionMsg::Moved(r),
                    )))
                },
            )
        }
        MoveSubActionMsg::ToTop(action_id, i) => {
            if i == 0 {
                return Task::none();
            }
            let service = Arc::clone(&rt.actions);
            Task::perform(
                async move {
                    service
                        .move_sub_action(action_id, i, 0)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::MoveSubAction(
                        MoveSubActionMsg::Moved(r),
                    )))
                },
            )
        }
        MoveSubActionMsg::ToBottom(action_id, i) => {
            if total == 0 || i + 1 >= total {
                return Task::none();
            }
            let last = total - 1;
            let service = Arc::clone(&rt.actions);
            Task::perform(
                async move {
                    service
                        .move_sub_action(action_id, i, last)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::MoveSubAction(
                        MoveSubActionMsg::Moved(r),
                    )))
                },
            )
        }
        MoveSubActionMsg::Moved(Ok(id)) => {
            Task::done(Message::Actions(ActionsMsg::ActionSelected(id)))
        }
        MoveSubActionMsg::Moved(Err(e)) => {
            tracing::warn!(error = %e, "move sub-action failed");
            Task::none()
        }
    }
}
