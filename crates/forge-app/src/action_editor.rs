use std::sync::Arc;

use forge_types::{Action, ActionId};
use iced::Task;

use crate::actions::{
    ActionDetail, AddActionForm, AddActionMsg, AddSubActionForm, AddSubActionMsg,
    RemoveSubActionMsg, SubActionKindChoice,
};
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

fn log_level_id(level: &forge_types::LogLevel) -> &'static str {
    match level {
        forge_types::LogLevel::Trace => "trace",
        forge_types::LogLevel::Debug => "debug",
        forge_types::LogLevel::Info => "info",
        forge_types::LogLevel::Warn => "warn",
        forge_types::LogLevel::Error => "error",
    }
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
            let service = Arc::clone(&rt.actions);
            Task::perform(async move { service.list_clip_options().await }, |clips| {
                Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                    AddSubActionMsg::ClipsLoaded(clips),
                )))
            })
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
            let service = Arc::clone(&rt.actions);
            Task::perform(async move { service.list_clip_options().await }, |clips| {
                Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                    AddSubActionMsg::ClipsLoaded(clips),
                )))
            })
        }
        AddSubActionMsg::KindSelected(kind) => {
            if let Some(f) = state.as_mut() {
                f.kind = kind;
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::SendChatMessageChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.config.send_chat_message = v;
            }
            Task::none()
        }
        AddSubActionMsg::SendChatTargetChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.config.send_chat_target = v;
            }
            Task::none()
        }
        AddSubActionMsg::SetGlobalNameChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.config.set_global_name = v;
            }
            Task::none()
        }
        AddSubActionMsg::SetGlobalValueChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.config.set_global_value = v;
            }
            Task::none()
        }
        AddSubActionMsg::DelayMsChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.config.delay_ms = v;
            }
            Task::none()
        }
        AddSubActionMsg::LogLevelSelected(level) => {
            if let Some(f) = state.as_mut() {
                f.config.log_level = level;
            }
            Task::none()
        }
        AddSubActionMsg::LogMessageChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.config.log_message = v;
            }
            Task::none()
        }
        AddSubActionMsg::PlaySoundClipSelected(clip_id) => {
            if let Some(f) = state.as_mut() {
                f.config.play_sound_clip_id = Some(clip_id);
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::SpeakTextChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.config.speak_text = v;
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::SpeakVoiceOverrideChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.config.speak_voice_override = v;
            }
            Task::none()
        }
        AddSubActionMsg::ReadFilePathChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.config.read_file_path = v;
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::ReadFileTargetVarChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.config.read_file_target_var = v;
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::RandomIntMinChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.config.random_int_min = v;
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::RandomIntMaxChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.config.random_int_max = v;
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::RandomIntTargetVarChanged(v) => {
            if let Some(f) = state.as_mut() {
                f.config.random_int_target_var = v;
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::ClipsLoaded(clips) => {
            if let Some(f) = state.as_mut() {
                f.available_clips = clips;
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
            if !form.is_valid() {
                let error_msg = match form.kind {
                    SubActionKindChoice::SendChat => "Message is required.",
                    SubActionKindChoice::SetGlobal => "Variable name is required.",
                    SubActionKindChoice::Delay => "Milliseconds must be a non-negative integer.",
                    SubActionKindChoice::Log => "Log message is required.",
                    SubActionKindChoice::PlaySound => "Select a clip to play.",
                    SubActionKindChoice::Speak => "Speak text is required.",
                    SubActionKindChoice::ReadFile => "Path and target variable are required.",
                    SubActionKindChoice::RandomInt => {
                        "min, max (min \u{2264} max), and target variable are required."
                    }
                };
                if let Some(f) = state.as_mut() {
                    f.error = Some(error_msg.to_string());
                }
                return Task::none();
            }
            use forge_types::{SubActionStep, Variant};
            use std::collections::BTreeMap;
            let step = match form.kind {
                SubActionKindChoice::SendChat => SubActionStep {
                    kind_id: "twitch.chat.send_message".to_owned(),
                    config: BTreeMap::from([
                        (
                            "message".to_owned(),
                            Variant::String(form.config.send_chat_message.clone()),
                        ),
                        (
                            "target".to_owned(),
                            Variant::String(form.config.send_chat_target.clone()),
                        ),
                    ]),
                    enabled: true,
                    label: None,
                },
                SubActionKindChoice::SetGlobal => SubActionStep {
                    kind_id: "core.globals.set".to_owned(),
                    config: BTreeMap::from([
                        (
                            "name".to_owned(),
                            Variant::String(form.config.set_global_name.clone()),
                        ),
                        (
                            "value".to_owned(),
                            Variant::String(form.config.set_global_value.clone()),
                        ),
                    ]),
                    enabled: true,
                    label: None,
                },
                SubActionKindChoice::Delay => {
                    let ms = form.config.delay_ms.trim().parse::<i64>().unwrap_or(0);
                    SubActionStep {
                        kind_id: "core.logic.wait".to_owned(),
                        config: BTreeMap::from([("ms".to_owned(), Variant::Int(ms))]),
                        enabled: true,
                        label: None,
                    }
                }
                SubActionKindChoice::Log => SubActionStep {
                    kind_id: "core.log.write".to_owned(),
                    config: BTreeMap::from([
                        (
                            "level".to_owned(),
                            Variant::String(log_level_id(&form.config.log_level).to_owned()),
                        ),
                        (
                            "message".to_owned(),
                            Variant::String(form.config.log_message.clone()),
                        ),
                    ]),
                    enabled: true,
                    label: None,
                },
                SubActionKindChoice::PlaySound => SubActionStep {
                    kind_id: "soundboard.sound.play".to_owned(),
                    config: BTreeMap::from([(
                        "clip_id".to_owned(),
                        Variant::String(
                            form.config
                                .play_sound_clip_id
                                .unwrap_or_default()
                                .to_string(),
                        ),
                    )]),
                    enabled: true,
                    label: None,
                },
                SubActionKindChoice::Speak => {
                    let mut config = BTreeMap::new();
                    config.insert(
                        "text".to_owned(),
                        Variant::String(form.config.speak_text.clone()),
                    );
                    if !form.config.speak_voice_override.trim().is_empty() {
                        config.insert(
                            "voice_id_override".to_owned(),
                            Variant::String(form.config.speak_voice_override.trim().to_owned()),
                        );
                    }
                    SubActionStep {
                        kind_id: "tts.speak.text".to_owned(),
                        config,
                        enabled: true,
                        label: None,
                    }
                }
                SubActionKindChoice::ReadFile => SubActionStep {
                    kind_id: "core.file.read".to_owned(),
                    config: BTreeMap::from([
                        (
                            "path".to_owned(),
                            Variant::String(form.config.read_file_path.trim().to_owned()),
                        ),
                        (
                            "target_var".to_owned(),
                            Variant::String(form.config.read_file_target_var.trim().to_owned()),
                        ),
                    ]),
                    enabled: true,
                    label: None,
                },
                SubActionKindChoice::RandomInt => {
                    let min = form
                        .config
                        .random_int_min
                        .trim()
                        .parse::<i64>()
                        .unwrap_or(0);
                    let max = form
                        .config
                        .random_int_max
                        .trim()
                        .parse::<i64>()
                        .unwrap_or(0);
                    SubActionStep {
                        kind_id: "core.random.int".to_owned(),
                        config: BTreeMap::from([
                            ("min".to_owned(), Variant::Int(min)),
                            ("max".to_owned(), Variant::Int(max)),
                            (
                                "target_var".to_owned(),
                                Variant::String(
                                    form.config.random_int_target_var.trim().to_owned(),
                                ),
                            ),
                        ]),
                        enabled: true,
                        label: None,
                    }
                }
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
