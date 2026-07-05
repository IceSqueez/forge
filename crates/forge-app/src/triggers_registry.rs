use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use forge_registry::{KindPlatformContract, effective_config};
use forge_storage::StorageError;
use forge_types::{
    ActionId, PlatformId, PlatformScope, TriggerConfig, TriggerInstance, TriggerInstanceId, Variant,
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Task,
    widget::{Space, button, column, container, row, rule, scrollable, stack, text, text_input},
};

use forge_widgets::{
    ConfirmKind, ConfirmModalParams, ConfirmTone, ForgePalette, MenuItem, MenuPlacement, Radius,
    SheetHeader, SheetWidth, SideSheet, Spacing, ToastKind, category_chip, confirm_modal,
    destructive_button, empty_state,
    icons::{Icon, tabler_icon},
    menu_button, primary_button, radius, search_input, secondary_button, section_header,
    skeleton_row, sp, spf,
    tokens::{BORDER_THIN, FONT_SM, FONT_XS, FONT_XXS, FontRole, font},
    value_preview,
};

use crate::Message;
use crate::Screen;
use crate::actions_field_form::{DynamicOptions, FieldBuffers, FieldEditMsg, render_field};
use crate::message::ToastMsg;
use crate::runtime_view::RuntimeView;
use crate::triggers_create_form::{CreateInstanceFormMsg, CreateInstanceFormState};

#[derive(Debug, Clone)]
pub struct TriggerInstanceRow {
    pub id: TriggerInstanceId,
    pub name: String,
    pub kind_id: String,
    pub enabled: bool,
    pub used_in_count: usize,
    pub overrides: TriggerConfig,
    pub platform_scope: PlatformScope,
}

#[derive(Debug, Clone)]
pub struct InstanceUsage {
    pub action_id: ActionId,
    pub action_name: String,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum UsageFilter {
    #[default]
    All,
    Used,
    Unused,
}

#[derive(Debug, Clone)]
pub struct ConfirmDisable {
    pub instance_id: TriggerInstanceId,
    pub action_count: usize,
}

pub struct TriggersRegistryState {
    pub instances: Vec<TriggerInstanceRow>,
    /// `false` until the first `Loaded` resolves; distinguishes an in-flight
    /// initial load from a genuinely empty registry.
    pub loaded: bool,
    pub selected_id: Option<TriggerInstanceId>,
    pub pending_scroll_to: Option<TriggerInstanceId>,
    pub used_in: Vec<InstanceUsage>,
    pub search: String,
    pub platform_filter: Option<String>,
    pub usage_filter: UsageFilter,
    pub sheet_width: f32,
    pub confirm_disable: Option<ConfirmDisable>,
    pub pending_delete: Option<TriggerInstanceId>,
    pub create_form: Option<CreateInstanceFormState>,
    pub menu_open: Option<TriggerInstanceId>,
    // (id, draft name) of the row whose name is being edited inline.
    pub renaming: Option<(TriggerInstanceId, String)>,
    // Inline edit session for the selected instance's Configuration section.
    pub config_edit: Option<ConfigEditState>,
}

/// Working state for editing a selected instance's config in its sheet.
///
/// The buffers are seeded from the *effective* config so every field shows its
/// inherited default while being edited; on save the buffer is diffed against
/// the kind's default so only genuinely-changed keys persist as overrides.
pub struct ConfigEditState {
    pub instance_id: TriggerInstanceId,
    pub text_buffer: BTreeMap<String, String>,
    pub overrides_buffer: BTreeMap<String, Variant>,
    pub saving: bool,
}

impl Default for TriggersRegistryState {
    fn default() -> Self {
        Self {
            instances: Vec::new(),
            loaded: false,
            selected_id: None,
            pending_scroll_to: None,
            used_in: Vec::new(),
            search: String::new(),
            platform_filter: None,
            usage_filter: UsageFilter::All,
            sheet_width: 420.0,
            confirm_disable: None,
            pending_delete: None,
            create_form: None,
            menu_open: None,
            renaming: None,
            config_edit: None,
        }
    }
}

pub fn trigger_rename_input_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("forge:trigger_rename")
}

#[derive(Debug, Clone)]
pub enum TriggersRegistryMsg {
    LoadRequested,
    Loaded(Result<Vec<TriggerInstanceRow>, String>),
    SearchChanged(String),
    PlatformFilterChanged(Option<String>),
    UsageFilterChanged(UsageFilter),
    RowSelected(TriggerInstanceId),
    RowDeselected,
    UsedInLoaded(Result<Vec<InstanceUsage>, String>),
    EnableToggled(TriggerInstanceId, bool),
    DisableConfirmAccepted(TriggerInstanceId),
    DisableConfirmDismissed,
    SheetClosed,
    SheetResized(f32),
    SheetWidthLoaded(Option<f32>),
    DeleteRequested(TriggerInstanceId),
    DeleteConfirmAccepted(TriggerInstanceId),
    DeleteConfirmDismissed,
    DeleteResult(Result<(), String>),
    NavigateToAction(ActionId),
    ScrollTo(TriggerInstanceId),
    OpenCreateForm,
    CreateFormMsg(CreateInstanceFormMsg),
    MenuToggled(TriggerInstanceId),
    MenuDismissed,
    RenameStarted(TriggerInstanceId),
    RenameBufferChanged(String),
    RenameSubmit,
    RenameCancel,
    RenameSaved(Result<(TriggerInstanceId, String), String>),
    UseAsTemplate(TriggerInstanceId),
    TemplateCreated(Result<TriggerInstanceId, String>),
    ConfigEditStarted(TriggerInstanceId),
    ConfigFieldChanged(String, Variant),
    ConfigIntInputChanged(String, String),
    ConfigFieldReverted(String),
    ConfigEditCancelled,
    ConfigEditSubmit,
    ConfigEditSaved(Result<(), String>),
}

pub fn update(
    state: &mut TriggersRegistryState,
    rt: &RuntimeView,
    msg: TriggersRegistryMsg,
) -> Task<Message> {
    match msg {
        TriggersRegistryMsg::LoadRequested => {
            let dp = Arc::clone(&rt.backend);
            let dp_settings = Arc::clone(&rt.backend) as Arc<dyn forge_storage::SettingsRepo>;
            Task::batch([
                Task::perform(
                    async move {
                        let repo = dp.trigger_instance_repo();
                        let instances =
                            repo.list_user_defined().await.map_err(|e| e.to_string())?;
                        let mut rows = Vec::with_capacity(instances.len());
                        for inst in instances {
                            let count = repo
                                .actions_using(inst.id)
                                .await
                                .map(|v| v.len())
                                .unwrap_or(0);
                            rows.push(TriggerInstanceRow {
                                id: inst.id,
                                name: inst.name,
                                kind_id: inst.kind_id,
                                overrides: inst.overrides,
                                enabled: inst.enabled,
                                used_in_count: count,
                                platform_scope: inst.platform_scope,
                            });
                        }
                        Ok::<Vec<TriggerInstanceRow>, String>(rows)
                    },
                    |r| Message::TriggersRegistry(TriggersRegistryMsg::Loaded(r)),
                ),
                Task::perform(
                    async move {
                        crate::ui_settings::sheet_width(&*dp_settings, "trigger_editor").await
                    },
                    |r| {
                        Message::TriggersRegistry(TriggersRegistryMsg::SheetWidthLoaded(
                            r.ok().flatten(),
                        ))
                    },
                ),
            ])
        }
        TriggersRegistryMsg::Loaded(Ok(rows)) => {
            state.loaded = true;
            state.instances = rows;
            if let Some(pending) = state.pending_scroll_to.take()
                && state.instances.iter().any(|r| r.id == pending)
            {
                state.selected_id = Some(pending);
                return Task::done(Message::TriggersRegistry(TriggersRegistryMsg::RowSelected(
                    pending,
                )));
            }
            Task::none()
        }
        TriggersRegistryMsg::Loaded(Err(msg)) => {
            state.loaded = true;
            Task::done(Message::Toast(ToastMsg::Fired {
                kind: ToastKind::Error,
                message: msg,
                duration_ms: 5000,
                action: None,
            }))
        }
        TriggersRegistryMsg::SearchChanged(s) => {
            state.search = s;
            Task::none()
        }
        TriggersRegistryMsg::PlatformFilterChanged(f) => {
            state.platform_filter = f;
            Task::none()
        }
        TriggersRegistryMsg::UsageFilterChanged(f) => {
            state.usage_filter = f;
            Task::none()
        }
        TriggersRegistryMsg::RowSelected(id) => {
            state.selected_id = Some(id);
            state.used_in.clear();
            // Selecting a different row abandons any in-progress config edit.
            if state
                .config_edit
                .as_ref()
                .is_some_and(|c| c.instance_id != id)
            {
                state.config_edit = None;
            }
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    let action_ids = dp
                        .trigger_instance_repo()
                        .actions_using(id)
                        .await
                        .map_err(|e| e.to_string())?;
                    let action_repo = dp.action_repo();
                    let mut usages = Vec::with_capacity(action_ids.len());
                    for aid in action_ids {
                        let name = action_repo
                            .get(aid)
                            .await
                            .ok()
                            .flatten()
                            .map(|a| a.name)
                            .unwrap_or_else(|| "(unknown)".to_owned());
                        usages.push(InstanceUsage {
                            action_id: aid,
                            action_name: name,
                        });
                    }
                    Ok::<Vec<InstanceUsage>, String>(usages)
                },
                |r| Message::TriggersRegistry(TriggersRegistryMsg::UsedInLoaded(r)),
            )
        }
        TriggersRegistryMsg::RowDeselected | TriggersRegistryMsg::SheetClosed => {
            state.selected_id = None;
            state.used_in.clear();
            state.config_edit = None;
            Task::none()
        }
        TriggersRegistryMsg::UsedInLoaded(Ok(usages)) => {
            state.used_in = usages;
            Task::none()
        }
        TriggersRegistryMsg::UsedInLoaded(Err(msg)) => {
            Task::done(Message::Toast(ToastMsg::Fired {
                kind: ToastKind::Error,
                message: msg,
                duration_ms: 5000,
                action: None,
            }))
        }
        TriggersRegistryMsg::EnableToggled(id, enabled) => {
            if !enabled {
                let count = state
                    .instances
                    .iter()
                    .find(|r| r.id == id)
                    .map(|r| r.used_in_count)
                    .unwrap_or(0);
                if count > 0 {
                    state.confirm_disable = Some(ConfirmDisable {
                        instance_id: id,
                        action_count: count,
                    });
                    return Task::none();
                }
            }
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    dp.trigger_instance_repo()
                        .set_enabled(id, enabled)
                        .await
                        .map_err(|e| e.to_string())
                },
                move |r| match r {
                    Ok(()) => Message::TriggersRegistry(TriggersRegistryMsg::LoadRequested),
                    Err(e) => Message::Toast(ToastMsg::Fired {
                        kind: ToastKind::Error,
                        message: e,
                        duration_ms: 5000,
                        action: None,
                    }),
                },
            )
        }
        TriggersRegistryMsg::DisableConfirmAccepted(id) => {
            state.confirm_disable = None;
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    dp.trigger_instance_repo()
                        .set_enabled(id, false)
                        .await
                        .map_err(|e| e.to_string())
                },
                move |r| match r {
                    Ok(()) => Message::TriggersRegistry(TriggersRegistryMsg::LoadRequested),
                    Err(e) => Message::Toast(ToastMsg::Fired {
                        kind: ToastKind::Error,
                        message: e,
                        duration_ms: 5000,
                        action: None,
                    }),
                },
            )
        }
        TriggersRegistryMsg::DisableConfirmDismissed => {
            state.confirm_disable = None;
            Task::none()
        }
        TriggersRegistryMsg::SheetResized(w) => {
            state.sheet_width = w;
            let dp_settings = Arc::clone(&rt.backend) as Arc<dyn forge_storage::SettingsRepo>;
            Task::perform(
                async move {
                    crate::ui_settings::set_sheet_width(&*dp_settings, "trigger_editor", w).await
                },
                |_| Message::Noop,
            )
        }
        TriggersRegistryMsg::SheetWidthLoaded(w) => {
            if let Some(w) = w {
                state.sheet_width = w;
            }
            Task::none()
        }
        TriggersRegistryMsg::DeleteRequested(id) => {
            // Same rule as the sheet footer's `can_delete` gate: an instance
            // still referenced by an action never opens the confirm at all.
            // The row `X` / sheet footer button are also dimmed+inert in that
            // case (proactive gate, 4156dae), so this branch is normally
            // unreachable from the UI — the toast here is defense-in-depth
            // reactive feedback, same copy as the `DeleteResult(Err)` path
            // below, for any dispatch path that bypasses the disabled button.
            let can_delete = state
                .instances
                .iter()
                .find(|r| r.id == id)
                .map(|r| r.used_in_count == 0)
                .unwrap_or(false);
            if can_delete {
                state.pending_delete = Some(id);
                Task::none()
            } else {
                Task::done(Message::Toast(ToastMsg::Fired {
                    kind: ToastKind::Error,
                    message: forge_widgets::tr!("triggers_delete_reference_block"),
                    duration_ms: 5000,
                    action: None,
                }))
            }
        }
        TriggersRegistryMsg::DeleteConfirmAccepted(id) => {
            state.pending_delete = None;
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    dp.trigger_instance_repo()
                        .delete(id)
                        .await
                        .map(|_| ())
                        .map_err(|e| match e {
                            StorageError::ReferenceBlock { .. } => {
                                forge_widgets::tr!("triggers_delete_reference_block")
                            }
                            other => other.to_string(),
                        })
                },
                |r| Message::TriggersRegistry(TriggersRegistryMsg::DeleteResult(r)),
            )
        }
        TriggersRegistryMsg::DeleteConfirmDismissed => {
            state.pending_delete = None;
            Task::none()
        }
        TriggersRegistryMsg::DeleteResult(Ok(())) => {
            state.selected_id = None;
            state.used_in.clear();
            Task::done(Message::TriggersRegistry(
                TriggersRegistryMsg::LoadRequested,
            ))
        }
        TriggersRegistryMsg::DeleteResult(Err(msg)) => {
            Task::done(Message::Toast(ToastMsg::Fired {
                kind: ToastKind::Error,
                message: msg,
                duration_ms: 5000,
                action: None,
            }))
        }
        TriggersRegistryMsg::NavigateToAction(action_id) => {
            Task::done(Message::Navigate(Screen::ActionEditor(Some(action_id))))
        }
        TriggersRegistryMsg::ScrollTo(instance_id) => {
            if state.instances.iter().any(|r| r.id == instance_id) {
                state.selected_id = Some(instance_id);
                Task::done(Message::TriggersRegistry(TriggersRegistryMsg::RowSelected(
                    instance_id,
                )))
            } else {
                state.pending_scroll_to = Some(instance_id);
                Task::none()
            }
        }
        TriggersRegistryMsg::OpenCreateForm => {
            state.create_form = Some(CreateInstanceFormState::default());
            Task::none()
        }
        TriggersRegistryMsg::CreateFormMsg(sub) => {
            crate::triggers_create_form::update(&mut state.create_form, rt, sub)
        }
        TriggersRegistryMsg::MenuToggled(id) => {
            state.menu_open = if state.menu_open == Some(id) {
                None
            } else {
                Some(id)
            };
            Task::none()
        }
        TriggersRegistryMsg::MenuDismissed => {
            state.menu_open = None;
            Task::none()
        }
        TriggersRegistryMsg::RenameStarted(id) => {
            let current = state
                .instances
                .iter()
                .find(|r| r.id == id)
                .map(|r| r.name.clone())
                .unwrap_or_default();
            state.renaming = Some((id, current));
            state.menu_open = None;
            iced::widget::operation::focus(trigger_rename_input_id())
        }
        TriggersRegistryMsg::RenameBufferChanged(buf) => {
            if let Some((_, name)) = state.renaming.as_mut() {
                *name = buf;
            }
            Task::none()
        }
        TriggersRegistryMsg::RenameCancel => {
            state.renaming = None;
            Task::none()
        }
        TriggersRegistryMsg::RenameSubmit => {
            let Some((id, name)) = state.renaming.clone() else {
                return Task::none();
            };
            let trimmed = name.trim().to_owned();
            if trimmed.is_empty() {
                state.renaming = None;
                return Task::none();
            }
            let dp = Arc::clone(&rt.backend);
            // Re-saving the fetched instance under its own id renames in place:
            // the upsert-on-conflict keeps the id, so existing action links
            // (a separate join table keyed by that id) are untouched.
            Task::perform(
                async move {
                    let repo = dp.trigger_instance_repo();
                    let mut instance = repo
                        .get(id)
                        .await
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| "trigger instance not found".to_owned())?;
                    instance.name = trimmed.clone();
                    repo.save(&instance).await.map_err(|e| e.to_string())?;
                    Ok::<(TriggerInstanceId, String), String>((id, trimmed))
                },
                |r| Message::TriggersRegistry(TriggersRegistryMsg::RenameSaved(r)),
            )
        }
        TriggersRegistryMsg::RenameSaved(Ok((id, new_name))) => {
            state.renaming = None;
            if let Some(row) = state.instances.iter_mut().find(|r| r.id == id) {
                row.name = new_name;
            }
            Task::none()
        }
        TriggersRegistryMsg::RenameSaved(Err(msg)) => {
            state.renaming = None;
            Task::done(Message::Toast(ToastMsg::Fired {
                kind: ToastKind::Error,
                message: msg,
                duration_ms: 5000,
                action: None,
            }))
        }
        TriggersRegistryMsg::UseAsTemplate(id) => {
            state.menu_open = None;
            let Some(src) = state.instances.iter().find(|r| r.id == id) else {
                return Task::none();
            };
            let instance = TriggerInstance {
                id: TriggerInstanceId::new(),
                kind_id: src.kind_id.clone(),
                name: forge_widgets::tr!("triggers_template_copy_name", name = src.name.as_str()),
                overrides: src.overrides.clone(),
                enabled: src.enabled,
                user_defined: true,
                platform_scope: src.platform_scope.clone(),
            };
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    let new_id = instance.id;
                    dp.trigger_instance_repo()
                        .save(&instance)
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok::<TriggerInstanceId, String>(new_id)
                },
                |r| Message::TriggersRegistry(TriggersRegistryMsg::TemplateCreated(r)),
            )
        }
        TriggersRegistryMsg::TemplateCreated(Ok(id)) => Task::batch([
            Task::done(Message::TriggersRegistry(
                TriggersRegistryMsg::LoadRequested,
            )),
            Task::done(Message::TriggersRegistry(TriggersRegistryMsg::RowSelected(
                id,
            ))),
        ]),
        TriggersRegistryMsg::TemplateCreated(Err(msg)) => {
            Task::done(Message::Toast(ToastMsg::Fired {
                kind: ToastKind::Error,
                message: msg,
                duration_ms: 5000,
                action: None,
            }))
        }
        TriggersRegistryMsg::ConfigEditStarted(id) => {
            let Some(row) = state.instances.iter().find(|r| r.id == id) else {
                return Task::none();
            };
            let Some(descriptor) = rt.trigger_registry.get(&row.kind_id) else {
                return Task::none();
            };
            let default_cfg = descriptor.default_config();
            let effective = effective_config(&default_cfg, &row.overrides);
            let mut text_buffer = BTreeMap::new();
            for (k, v) in &effective {
                text_buffer.insert(
                    k.clone(),
                    crate::actions_field_form::variant_to_display_str(v),
                );
            }
            state.config_edit = Some(ConfigEditState {
                instance_id: id,
                text_buffer,
                overrides_buffer: effective,
                saving: false,
            });
            Task::none()
        }
        TriggersRegistryMsg::ConfigFieldChanged(key, variant) => {
            if let Some(edit) = state.config_edit.as_mut() {
                crate::actions_field_form::apply_field_edit(
                    &mut edit.text_buffer,
                    &mut edit.overrides_buffer,
                    FieldEditMsg::Set(key, variant),
                );
            }
            Task::none()
        }
        TriggersRegistryMsg::ConfigIntInputChanged(key, raw) => {
            if let Some(edit) = state.config_edit.as_mut() {
                crate::actions_field_form::apply_field_edit(
                    &mut edit.text_buffer,
                    &mut edit.overrides_buffer,
                    FieldEditMsg::IntInput(key, raw),
                );
            }
            Task::none()
        }
        TriggersRegistryMsg::ConfigFieldReverted(key) => {
            // Reverting = writing the field back to its schema default so the
            // save-time diff drops it from the sparse overrides. The default is
            // resolved from the kind descriptor; a key with no default is
            // cleared outright.
            let Some(edit_id) = state.config_edit.as_ref().map(|e| e.instance_id) else {
                return Task::none();
            };
            let default_v = state
                .instances
                .iter()
                .find(|r| r.id == edit_id)
                .and_then(|row| rt.trigger_registry.get(&row.kind_id))
                .map(|d| d.default_config())
                .and_then(|cfg| cfg.get(&key).cloned());
            if let Some(edit) = state.config_edit.as_mut() {
                let op = match default_v {
                    Some(v) => FieldEditMsg::Set(key, v),
                    None => FieldEditMsg::Clear(key),
                };
                crate::actions_field_form::apply_field_edit(
                    &mut edit.text_buffer,
                    &mut edit.overrides_buffer,
                    op,
                );
            }
            Task::none()
        }
        TriggersRegistryMsg::ConfigEditCancelled => {
            state.config_edit = None;
            Task::none()
        }
        TriggersRegistryMsg::ConfigEditSubmit => {
            let Some(edit) = state.config_edit.as_ref() else {
                return Task::none();
            };
            let id = edit.instance_id;
            let Some(row) = state.instances.iter().find(|r| r.id == id) else {
                return Task::none();
            };
            let Some(descriptor) = rt.trigger_registry.get(&row.kind_id) else {
                return Task::none();
            };
            let default_cfg = descriptor.default_config();
            let sparse: TriggerConfig = edit
                .overrides_buffer
                .iter()
                .filter(|(k, v)| default_cfg.get(*k) != Some(*v))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            if let Some(edit) = state.config_edit.as_mut() {
                edit.saving = true;
            }
            let dp = Arc::clone(&rt.backend);
            // Id-stable upsert: re-saving the fetched instance keeps its id, so
            // the action_trigger_instances join rows stay intact.
            Task::perform(
                async move {
                    let repo = dp.trigger_instance_repo();
                    let mut instance = repo
                        .get(id)
                        .await
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| "trigger instance not found".to_owned())?;
                    instance.overrides = sparse;
                    repo.save(&instance).await.map_err(|e| e.to_string())?;
                    Ok::<(), String>(())
                },
                |r| Message::TriggersRegistry(TriggersRegistryMsg::ConfigEditSaved(r)),
            )
        }
        TriggersRegistryMsg::ConfigEditSaved(Ok(())) => {
            state.config_edit = None;
            // Reload to refresh the row's overrides (and used-in counts); the
            // selection is preserved so the sheet re-renders in read mode.
            Task::done(Message::TriggersRegistry(
                TriggersRegistryMsg::LoadRequested,
            ))
        }
        TriggersRegistryMsg::ConfigEditSaved(Err(msg)) => {
            if let Some(edit) = state.config_edit.as_mut() {
                edit.saving = false;
            }
            Task::done(Message::Toast(ToastMsg::Fired {
                kind: ToastKind::Error,
                message: msg,
                duration_ms: 5000,
                action: None,
            }))
        }
    }
}

fn registry_loading_skeleton<'a>(palette: &ForgePalette) -> Element<'a, Message> {
    let rows: Vec<Element<'a, Message>> = (0..6)
        .map(|_| {
            container(skeleton_row(&[180.0, 90.0], palette))
                .padding([sp(Spacing::Sm), sp(Spacing::Md)])
                .width(Length::Fill)
                .into()
        })
        .collect();

    container(column(rows).spacing(spf(Spacing::Xxs)))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

pub fn view<'a>(
    state: &'a TriggersRegistryState,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;

    let search_lower = state.search.to_lowercase();
    let filtered: Vec<&TriggerInstanceRow> = state
        .instances
        .iter()
        .filter(|row| {
            let matches_search = search_lower.is_empty()
                || row.name.to_lowercase().contains(&search_lower)
                || row.kind_id.to_lowercase().contains(&search_lower);
            let matches_platform = state
                .platform_filter
                .as_deref()
                .map(|prefix| row.kind_id.starts_with(prefix))
                .unwrap_or(true);
            let matches_usage = match state.usage_filter {
                UsageFilter::All => true,
                UsageFilter::Used => row.used_in_count > 0,
                UsageFilter::Unused => row.used_in_count == 0,
            };
            matches_search && matches_platform && matches_usage
        })
        .collect();

    let filters_active = state.platform_filter.is_some()
        || state.usage_filter != UsageFilter::All
        || !state.search.is_empty();

    let header = registry_header(state, palette);

    let divider_style = move |_: &iced::Theme| rule::Style {
        color: p.border_regular,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    };

    let list_content: Element<'_, Message> = if !state.loaded {
        registry_loading_skeleton(palette)
    } else if state.instances.is_empty() && !filters_active {
        container(empty_state(
            forge_widgets::tr!("triggers_empty_title"),
            forge_widgets::tr!("triggers_empty_hint"),
            Some((
                forge_widgets::tr!("triggers_empty_create"),
                Message::TriggersRegistry(TriggersRegistryMsg::OpenCreateForm),
            )),
            palette,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if filtered.is_empty() {
        let clear_msg = Message::TriggersRegistry(TriggersRegistryMsg::PlatformFilterChanged(None));
        container(empty_state(
            forge_widgets::tr!("triggers_no_results_title"),
            forge_widgets::tr!("triggers_no_results_hint"),
            Some((forge_widgets::tr!("triggers_clear_filters"), clear_msg)),
            palette,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        let row_els: Vec<Element<'_, Message>> = filtered
            .iter()
            .map(|row| {
                let rename_buf = state
                    .renaming
                    .as_ref()
                    .filter(|(id, _)| *id == row.id)
                    .map(|(_, name)| name.as_str());
                instance_row(
                    row,
                    state.selected_id == Some(row.id),
                    state.menu_open == Some(row.id),
                    rename_buf,
                    palette,
                )
            })
            .collect();
        scrollable(column(row_els)).height(Length::Fill).into()
    };

    let sheet_open = state.selected_id.is_some();
    let sheet_title: Cow<'_, str> = state
        .selected_id
        .and_then(|id| state.instances.iter().find(|r| r.id == id))
        .map(|r| Cow::Borrowed(r.name.as_str()))
        .unwrap_or(Cow::Borrowed(""));

    let sheet_body: Element<'_, Message> = state
        .selected_id
        .and_then(|id| state.instances.iter().find(|r| r.id == id))
        .map(|row| sheet_body_for(row, &state.used_in, state.config_edit.as_ref(), rt, palette))
        .unwrap_or_else(|| Space::new().width(Length::Fill).height(Length::Fill).into());

    let sheet_icon_tint = state
        .selected_id
        .and_then(|id| state.instances.iter().find(|r| r.id == id))
        .map(|r| platform_dot_color(&r.kind_id, palette))
        .unwrap_or(palette.info);

    let sheet = SideSheet::new(sheet_body)
        .open(sheet_open)
        .palette(palette)
        .width(SheetWidth::new(
            state.sheet_width.clamp(280.0, 720.0),
            280.0,
            720.0,
        ))
        .resizable(true)
        .sheet_key("trigger_editor")
        .header_icon(Icon::Bolt, sheet_icon_tint)
        .header(SheetHeader {
            title: sheet_title,
            subtitle: None,
            on_close: Some(Message::TriggersRegistry(TriggersRegistryMsg::SheetClosed)),
        })
        .on_close(Message::TriggersRegistry(TriggersRegistryMsg::SheetClosed))
        .on_resize(|w| Message::TriggersRegistry(TriggersRegistryMsg::SheetResized(w)));

    let main_col: Element<'_, Message> = column![
        header,
        rule::horizontal(1.0).style(divider_style),
        list_content,
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into();

    let main_with_sheet: Element<'_, Message> = stack![main_col, sheet].into();

    let with_confirm_disable: Element<'_, Message> = if let Some(ref cd) = state.confirm_disable {
        let dialog = confirm_disable_dialog(cd, palette);
        stack![main_with_sheet, dialog].into()
    } else {
        main_with_sheet
    };

    let with_pending_delete: Element<'_, Message> = match state
        .pending_delete
        .and_then(|id| state.instances.iter().find(|r| r.id == id))
    {
        Some(row) => {
            let modal = confirm_modal(
                ConfirmModalParams {
                    kind: ConfirmKind::TriggerLink,
                    item_name: Cow::Borrowed(row.name.as_str()),
                    cascade_hint: None,
                    tone: ConfirmTone::Destructive,
                },
                Message::TriggersRegistry(TriggersRegistryMsg::DeleteConfirmAccepted(row.id)),
                Message::TriggersRegistry(TriggersRegistryMsg::DeleteConfirmDismissed),
                palette,
            );
            stack![with_confirm_disable, modal].into()
        }
        None => with_confirm_disable,
    };

    if let Some(ref form) = state.create_form {
        let overlay = crate::triggers_create_form::view(form, rt, palette);
        stack![with_pending_delete, overlay].into()
    } else {
        with_pending_delete
    }
}

fn registry_header<'a>(
    state: &'a TriggersRegistryState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;

    let search = container(search_input(
        forge_widgets::tr!("triggers_search_placeholder"),
        &state.search,
        |s| Message::TriggersRegistry(TriggersRegistryMsg::SearchChanged(s)),
        palette,
    ))
    .width(Length::Fixed(200.0));

    let is_filter = |prefix: &str| {
        state
            .platform_filter
            .as_deref()
            .is_some_and(|x| x == prefix)
    };
    let all_active = state.platform_filter.is_none();

    let lbl_filter_all = forge_widgets::tr!("triggers_filter_all");
    let lbl_filter_twitch = forge_widgets::tr!("triggers_filter_twitch");
    let lbl_filter_youtube = forge_widgets::tr!("triggers_filter_youtube");
    let lbl_filter_kick = forge_widgets::tr!("triggers_filter_kick");
    let lbl_filter_obs = forge_widgets::tr!("triggers_filter_obs");
    let lbl_filter_vtube = forge_widgets::tr!("triggers_filter_vtube");
    let lbl_filter_midi = forge_widgets::tr!("triggers_filter_midi");
    let lbl_filter_hotkey = forge_widgets::tr!("triggers_filter_hotkey");
    let lbl_filter_discord = forge_widgets::tr!("triggers_filter_discord");
    let lbl_filter_script = forge_widgets::tr!("triggers_filter_script");

    let chip = |label: &str, prefix: &'static str| {
        category_chip(
            palette,
            label,
            platform_dot_color(prefix, palette),
            is_filter(prefix),
            Message::TriggersRegistry(TriggersRegistryMsg::PlatformFilterChanged(Some(
                prefix.to_owned(),
            ))),
        )
    };

    let chip_twitch = chip(lbl_filter_twitch.as_str(), "twitch.");
    let chip_youtube = chip(lbl_filter_youtube.as_str(), "youtube.");
    let chip_kick = chip(lbl_filter_kick.as_str(), "kick.");
    let chip_obs = chip(lbl_filter_obs.as_str(), "obs.");
    let chip_vtube = chip(lbl_filter_vtube.as_str(), "vtube.");
    let chip_midi = chip(lbl_filter_midi.as_str(), "midi.");
    let chip_hotkey = chip(lbl_filter_hotkey.as_str(), "hotkey.");
    let chip_discord = chip(lbl_filter_discord.as_str(), "discord.");
    let chip_script = chip(lbl_filter_script.as_str(), "script.");
    let chip_all = category_chip(
        palette,
        lbl_filter_all.as_str(),
        p.text_secondary,
        all_active,
        Message::TriggersRegistry(TriggersRegistryMsg::PlatformFilterChanged(None)),
    );

    let platform_chips = column![
        row![chip_twitch, chip_youtube, chip_kick, chip_obs, chip_vtube]
            .spacing(spf(Spacing::Xxs))
            .align_y(Alignment::Center),
        row![chip_midi, chip_hotkey, chip_discord, chip_script, chip_all]
            .spacing(spf(Spacing::Xxs))
            .align_y(Alignment::Center),
    ]
    .spacing(spf(Spacing::Xxs));

    let usage_all_active = state.usage_filter == UsageFilter::All;
    let usage_used_active = state.usage_filter == UsageFilter::Used;
    let usage_unused_active = state.usage_filter == UsageFilter::Unused;

    let chip_u_all = usage_filter_chip(
        forge_widgets::tr!("triggers_usage_all"),
        usage_all_active,
        Message::TriggersRegistry(TriggersRegistryMsg::UsageFilterChanged(UsageFilter::All)),
        palette,
    );
    let chip_u_used = usage_filter_chip(
        forge_widgets::tr!("triggers_usage_used"),
        usage_used_active,
        Message::TriggersRegistry(TriggersRegistryMsg::UsageFilterChanged(UsageFilter::Used)),
        palette,
    );
    let chip_u_unused = usage_filter_chip(
        forge_widgets::tr!("triggers_usage_unused"),
        usage_unused_active,
        Message::TriggersRegistry(TriggersRegistryMsg::UsageFilterChanged(UsageFilter::Unused)),
        palette,
    );

    let usage_chips = row![chip_u_all, chip_u_used, chip_u_unused]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center);

    let make_divider_v = move || {
        container(Space::new().width(0.5).height(16.0))
            .width(0.5)
            .height(16.0)
            .style(move |_: &iced::Theme| container::Style {
                background: Some(Background::Color(p.border_regular)),
                ..container::Style::default()
            })
    };

    let breadcrumb_row = row![
        tabler_icon::<Message>(Icon::Home, 13.0, p.text_faint),
        tabler_icon::<Message>(Icon::ChevronRight, 11.0, p.text_faint),
        text(forge_widgets::tr!("triggers_breadcrumb_automation"))
            .size(FONT_SM)
            .color(p.text_muted),
        tabler_icon::<Message>(Icon::ChevronRight, 11.0, p.text_faint),
        text(forge_widgets::tr!("triggers_breadcrumb_triggers"))
            .size(FONT_SM)
            .color(p.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let create_btn = secondary_button(
        forge_widgets::tr!("triggers_open_create_btn"),
        Message::TriggersRegistry(TriggersRegistryMsg::OpenCreateForm),
        palette,
    );

    let right = row![
        platform_chips,
        make_divider_v(),
        usage_chips,
        make_divider_v(),
        search,
        make_divider_v(),
        create_btn,
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let inner =
        row![breadcrumb_row, Space::new().width(Length::Fill), right].align_y(Alignment::Center);

    container(inner)
        .width(Length::Fill)
        .padding([sp(Spacing::Xs), sp(Spacing::Md)])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.shell)),
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn usage_filter_chip<'a>(
    label: impl Into<std::borrow::Cow<'a, str>>,
    active: bool,
    on_press: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    forge_widgets::chip(
        label,
        forge_widgets::ChipGlyph::None,
        active,
        Some(on_press),
        palette,
    )
}

fn platform_dot_color(kind_id: &str, palette: &ForgePalette) -> Color {
    match kind_id.split('.').next().unwrap_or("") {
        "twitch" => palette.brand,
        "youtube" => palette.platform_youtube,
        "kick" => palette.platform_kick,
        "obs" => palette.success,
        "vtube" => palette.accent_teal,
        "midi" => palette.random,
        "hotkey" => palette.warning,
        "discord" => palette.info,
        "script" | "rhai" => palette.warning,
        "timer" => palette.warning,
        "server" => palette.info,
        "http" => palette.random,
        "audio" => palette.bits,
        "core" => palette.warning,
        _ => palette.info,
    }
}

fn instance_row<'a>(
    row: &'a TriggerInstanceRow,
    selected: bool,
    menu_open: bool,
    rename_buf: Option<&'a str>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let dot_color = platform_dot_color(&row.kind_id, palette);
    let dot_size = 7.0_f32;

    let dot = container(Space::new().width(dot_size).height(dot_size))
        .width(dot_size)
        .height(dot_size)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(if row.enabled {
                dot_color
            } else {
                Color {
                    a: 0.35,
                    ..dot_color
                }
            })),
            border: Border {
                radius: (dot_size / 2.0).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let name_el: Element<'_, Message> = if let Some(buf) = rename_buf {
        text_input("", buf)
            .id(trigger_rename_input_id())
            .on_input(|s| Message::TriggersRegistry(TriggersRegistryMsg::RenameBufferChanged(s)))
            .on_submit(Message::TriggersRegistry(TriggersRegistryMsg::RenameSubmit))
            .size(FONT_SM)
            .padding([2, sp(Spacing::Xs)])
            .width(Length::Fill)
            .style(move |_: &iced::Theme, _status| text_input::Style {
                background: Background::Color(p.shell),
                border: Border {
                    color: p.brand,
                    width: 0.5,
                    radius: radius(Radius::Sm).into(),
                },
                icon: p.text_muted,
                placeholder: p.text_muted,
                value: p.text_primary,
                selection: Color { a: 0.25, ..p.brand },
            })
            .into()
    } else {
        text(row.name.as_str())
            .size(FONT_SM)
            .color(if row.enabled {
                p.text_primary
            } else {
                p.text_muted
            })
            .font(font(FontRole::Body))
            .into()
    };

    let kind_meta = text(row.kind_id.as_str())
        .size(FONT_XS)
        .color(p.text_faint)
        .font(font(FontRole::Monospace));

    let usage_badge: Element<'_, Message> = if row.used_in_count > 0 {
        let label = forge_widgets::tr!("triggers_usage_badge", count = row.used_in_count as i64);
        container(
            text(label)
                .size(FONT_XS)
                .color(p.text_muted)
                .font(font(FontRole::Body)),
        )
        .padding([2, sp(Spacing::Xs)])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.surface_overlay)),
            border: Border {
                radius: radius(Radius::Sm).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        })
        .into()
    } else {
        Space::new().width(0).into()
    };

    let toggle_id = row.id;
    let toggle_enabled = row.enabled;

    let toggle_btn = forge_widgets::toggle_switch(
        toggle_enabled,
        None,
        Message::TriggersRegistry(TriggersRegistryMsg::EnableToggled(
            toggle_id,
            !toggle_enabled,
        )),
        palette,
    );

    let row_id = row.id;
    // A still-referenced instance can't be deleted (FK), so its Delete item is
    // disabled — same gate as the sheet footer's `can_delete`.
    let can_delete = row.used_in_count == 0;
    let menu_items: Vec<MenuItem<Message>> = vec![
        MenuItem::Item {
            label: forge_widgets::tr!("triggers_menu_rename"),
            icon: Some(Icon::InfoCircle),
            on_press: Message::TriggersRegistry(TriggersRegistryMsg::RenameStarted(row_id)),
            shortcut: None,
            color: None,
            disabled: false,
        },
        MenuItem::Item {
            label: forge_widgets::tr!("triggers_menu_template"),
            icon: Some(Icon::Copy),
            on_press: Message::TriggersRegistry(TriggersRegistryMsg::UseAsTemplate(row_id)),
            shortcut: None,
            color: None,
            disabled: false,
        },
        MenuItem::Divider,
        MenuItem::Item {
            label: forge_widgets::tr!("triggers_menu_delete"),
            icon: Some(Icon::X),
            on_press: Message::TriggersRegistry(TriggersRegistryMsg::DeleteRequested(row_id)),
            shortcut: None,
            color: Some(p.random),
            disabled: !can_delete,
        },
    ];
    let menu_btn = menu_button(
        Icon::DotsVertical,
        menu_open,
        Message::TriggersRegistry(TriggersRegistryMsg::MenuToggled(row_id)),
        Message::TriggersRegistry(TriggersRegistryMsg::MenuDismissed),
        menu_items,
        MenuPlacement::BottomRight,
        palette,
    );

    let controls = row![toggle_btn, menu_btn]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center);

    let scope_indicator: Element<'_, Message> = match &row.platform_scope {
        PlatformScope::Any => Space::new().width(0).into(),
        PlatformScope::Only(set) => {
            let platforms: Vec<PlatformId> = set.iter().copied().collect();
            if platforms.is_empty() {
                Space::new().width(0).into()
            } else if platforms.len() == 1 {
                let pid = platforms[0];
                let sdot_color = platform_id_color(pid, palette);
                let sdot_size = 5.0_f32;
                let sdot = container(Space::new().width(sdot_size).height(sdot_size))
                    .width(sdot_size)
                    .height(sdot_size)
                    .style(move |_: &iced::Theme| container::Style {
                        background: Some(Background::Color(sdot_color)),
                        border: Border {
                            radius: (sdot_size / 2.0).into(),
                            color: Color::TRANSPARENT,
                            width: 0.0,
                        },
                        ..container::Style::default()
                    });
                row![
                    sdot,
                    text(platform_id_name(pid))
                        .size(FONT_XS)
                        .color(p.text_muted)
                        .font(font(FontRole::Body)),
                ]
                .spacing(spf(Spacing::Xxs))
                .align_y(Alignment::Center)
                .into()
            } else {
                let mut dot_els: Vec<Element<'_, Message>> = platforms
                    .iter()
                    .take(2)
                    .map(|&pid| {
                        let sdot_color = platform_id_color(pid, palette);
                        let sdot_size = 5.0_f32;
                        container(Space::new().width(sdot_size).height(sdot_size))
                            .width(sdot_size)
                            .height(sdot_size)
                            .style(move |_: &iced::Theme| container::Style {
                                background: Some(Background::Color(sdot_color)),
                                border: Border {
                                    radius: (sdot_size / 2.0).into(),
                                    color: Color::TRANSPARENT,
                                    width: 0.0,
                                },
                                ..container::Style::default()
                            })
                            .into()
                    })
                    .collect();
                if platforms.len() > 2 {
                    dot_els.push(
                        text(format!("+{}", platforms.len() - 2))
                            .size(FONT_XS)
                            .color(p.text_faint)
                            .font(font(FontRole::Body))
                            .into(),
                    );
                }
                iced::widget::row(dot_els)
                    .spacing(spf(Spacing::Xxs))
                    .align_y(Alignment::Center)
                    .into()
            }
        }
    };

    let trailing = row![
        container(scope_indicator)
            .align_y(Alignment::Center)
            .padding([0, sp(Spacing::Xs)]),
        container(usage_badge)
            .align_y(Alignment::Center)
            .padding([0, sp(Spacing::Xs)]),
        container(controls).align_y(Alignment::Center),
    ]
    .align_y(Alignment::Center);

    forge_widgets::row_card(name_el, palette)
        .leading(container(dot).align_y(Alignment::Center))
        .meta(kind_meta)
        .trailing(trailing)
        .selected(selected)
        .idle_background(p.base)
        .padding([sp(Spacing::Xs), sp(Spacing::Md)])
        .on_press(Message::TriggersRegistry(TriggersRegistryMsg::RowSelected(
            row_id,
        )))
        .into()
}

fn sheet_body_for<'a>(
    row: &'a TriggerInstanceRow,
    used_in: &'a [InstanceUsage],
    config_edit: Option<&'a ConfigEditState>,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let mono = font(FontRole::Monospace);

    let kind_row = container(
        text(row.kind_id.as_str())
            .size(FONT_XS)
            .color(p.text_muted)
            .font(mono),
    )
    .padding([sp(Spacing::Xs), sp(Spacing::Md)]);

    let divider_style = move |_: &iced::Theme| rule::Style {
        color: p.border_regular,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    };

    let config_section = config_section_view(row, config_edit, rt, palette);

    let used_in_section: Element<'_, Message> = if !used_in.is_empty() {
        let hdr = section_header(
            forge_widgets::tr!("triggers_sheet_section_used_in"),
            Some(used_in.len() as u32),
            palette,
        );
        let usage_rows: Vec<Element<'_, Message>> = used_in
            .iter()
            .map(|u| {
                let aid = u.action_id;
                let p_row = p;
                button(
                    text(u.action_name.as_str())
                        .size(FONT_SM)
                        .font(font(FontRole::Body)),
                )
                .on_press(Message::TriggersRegistry(
                    TriggersRegistryMsg::NavigateToAction(aid),
                ))
                .padding([sp(Spacing::Xxs), sp(Spacing::Md)])
                .style(move |_: &iced::Theme, status| button::Style {
                    background: None,
                    text_color: if matches!(status, button::Status::Hovered) {
                        p_row.brand
                    } else {
                        p_row.text_secondary
                    },
                    border: Border::default(),
                    shadow: iced::Shadow::default(),
                    snap: false,
                })
                .width(Length::Fill)
                .into()
            })
            .collect();
        column(std::iter::once(hdr).chain(usage_rows).collect::<Vec<_>>()).into()
    } else {
        Space::new().width(0).height(0).into()
    };

    let can_delete = row.used_in_count == 0;
    let delete_id = row.id;

    let delete_lbl = forge_widgets::tr!("triggers_sheet_delete_btn");
    let footer = container(
        row![
            Space::new().width(Length::Fill),
            if can_delete {
                destructive_button(
                    delete_lbl.clone(),
                    Message::TriggersRegistry(TriggersRegistryMsg::DeleteRequested(delete_id)),
                    palette,
                )
            } else {
                container(
                    text(delete_lbl)
                        .size(FONT_SM)
                        .color(p.disabled)
                        .font(font(FontRole::Body)),
                )
                .padding([sp(Spacing::Sm), sp(Spacing::Md)])
                .into()
            },
        ]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .style(move |_: &iced::Theme| container::Style {
        border: Border {
            color: p.border_regular,
            width: BORDER_THIN,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    });

    let platform_section = sheet_platform_section(row, rt, palette);

    column![
        kind_row,
        rule::horizontal(1.0).style(divider_style),
        platform_section,
        config_section,
        rule::horizontal(1.0).style(divider_style),
        scrollable(used_in_section).height(Length::Fill),
        rule::horizontal(1.0).style(divider_style),
        footer,
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn config_section_view<'a>(
    row: &'a TriggerInstanceRow,
    config_edit: Option<&'a ConfigEditState>,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let mono = font(FontRole::Monospace);

    let Some(descriptor) = rt.trigger_registry.get(&row.kind_id) else {
        return container(
            text(forge_widgets::tr!("triggers_sheet_not_registered"))
                .size(FONT_XS)
                .color(p.text_faint)
                .font(font(FontRole::Body)),
        )
        .padding([sp(Spacing::Xs), sp(Spacing::Md)])
        .into();
    };

    let fields = descriptor.config_fields();
    if fields.is_empty() {
        return column![
            config_header(0, palette),
            container(
                text(forge_widgets::tr!("triggers_sheet_no_config"))
                    .size(FONT_XS)
                    .color(p.text_faint)
                    .font(font(FontRole::Body)),
            )
            .padding([sp(Spacing::Xs), sp(Spacing::Md)]),
        ]
        .into();
    }

    let default_cfg = descriptor.default_config();
    let editing = config_edit.filter(|c| c.instance_id == row.id);

    if let Some(edit) = editing {
        // EDIT MODE — every field type is rendered by the shared render_field.
        let buffers = FieldBuffers {
            text: &edit.text_buffer,
            overrides: &edit.overrides_buffer,
        };
        let options = DynamicOptions::new();
        let on_edit = |e: FieldEditMsg| {
            let m = match e {
                FieldEditMsg::Set(k, v) => TriggersRegistryMsg::ConfigFieldChanged(k, v),
                FieldEditMsg::IntInput(k, raw) => {
                    TriggersRegistryMsg::ConfigIntInputChanged(k, raw)
                }
                FieldEditMsg::Clear(k) => TriggersRegistryMsg::ConfigFieldReverted(k),
            };
            Message::TriggersRegistry(m)
        };

        let mut overridden_count = 0usize;
        let field_rows: Vec<Element<'a, Message>> = fields
            .iter()
            .map(|field| {
                let key = form_field_key(field);
                let is_overridden = edit.overrides_buffer.get(key) != default_cfg.get(key);
                if is_overridden {
                    overridden_count += 1;
                }
                let widget = render_field(field, &buffers, &options, palette, on_edit);
                row![
                    container(widget).width(Length::Fill),
                    revert_button(key, is_overridden, palette),
                ]
                .spacing(spf(Spacing::Xs))
                .align_y(Alignment::Center)
                .padding([sp(Spacing::Xxs), sp(Spacing::Md)])
                .into()
            })
            .collect();

        let sparse: TriggerConfig = edit
            .overrides_buffer
            .iter()
            .filter(|(k, v)| default_cfg.get(*k) != Some(*v))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let dirty = sparse != row.overrides;
        let footer = config_edit_footer(dirty && !edit.saving, palette);

        column(
            std::iter::once(config_header(overridden_count, palette))
                .chain(field_rows)
                .chain(std::iter::once(footer))
                .collect::<Vec<_>>(),
        )
        .into()
    } else {
        // READ MODE — calm summary; clicking a value opens the edit session.
        let effective = effective_config(&default_cfg, &row.overrides);
        let field_rows: Vec<Element<'a, Message>> = fields
            .iter()
            .map(|field| {
                let key = form_field_key(field);
                let label = form_field_label(field);
                let is_overridden = row.overrides.contains_key(key);
                let value = effective.get(key);

                let label_el = text(label)
                    .size(FONT_XS)
                    .color(p.text_secondary)
                    .font(font(FontRole::Body));

                let value_el: Element<'a, Message> = if let Some(v) = value {
                    if is_overridden {
                        value_preview::<Message>(palette, v)
                    } else {
                        text(variant_one_line(v))
                            .size(FONT_XS)
                            .color(p.text_faint)
                            .font(mono)
                            .into()
                    }
                } else {
                    text("—")
                        .size(FONT_XS)
                        .color(p.text_faint)
                        .font(mono)
                        .into()
                };

                let value_btn = button(container(value_el).width(Length::Fill))
                    .on_press(Message::TriggersRegistry(
                        TriggersRegistryMsg::ConfigEditStarted(row.id),
                    ))
                    .padding(0)
                    .width(Length::Fill)
                    .style(|_: &iced::Theme, _status| button::Style {
                        background: None,
                        border: Border::default(),
                        text_color: Color::TRANSPARENT,
                        shadow: iced::Shadow::default(),
                        snap: false,
                    });

                row![
                    container(label_el).width(Length::FillPortion(4)),
                    container(value_btn).width(Length::FillPortion(6)),
                ]
                .align_y(Alignment::Center)
                .padding([sp(Spacing::Xxs), sp(Spacing::Md)])
                .into()
            })
            .collect();

        column(
            std::iter::once(config_header(row.overrides.len(), palette))
                .chain(field_rows)
                .collect::<Vec<_>>(),
        )
        .into()
    }
}

fn config_header<'a>(overridden_count: usize, palette: &'a ForgePalette) -> Element<'a, Message> {
    let p = *palette;
    let mono = font(FontRole::Monospace);
    let right: Element<'a, Message> = if overridden_count > 0 {
        text(forge_widgets::tr!(
            "triggers_sheet_config_overridden",
            count = overridden_count as i64
        ))
        .size(FONT_XXS)
        .color(p.warning)
        .font(mono)
        .into()
    } else {
        text(forge_widgets::tr!("triggers_sheet_config_all_defaults"))
            .size(FONT_XXS)
            .color(p.text_faint)
            .font(mono)
            .into()
    };
    container(
        row![
            text(forge_widgets::tr!("triggers_sheet_section_configuration"))
                .size(FONT_XXS)
                .color(p.text_muted)
                .font(mono),
            Space::new().width(Length::Fill),
            right,
        ]
        .align_y(Alignment::Center),
    )
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .into()
}

fn revert_button<'a>(
    key: &str,
    overridden: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    if !overridden {
        // Reserve the slot so overridden and default rows stay column-aligned.
        return Space::new().width(18.0).height(18.0).into();
    }
    let key_owned = key.to_owned();
    button(tabler_icon::<Message>(Icon::X, 12.0, p.text_faint))
        .on_press(Message::TriggersRegistry(
            TriggersRegistryMsg::ConfigFieldReverted(key_owned),
        ))
        .padding(sp(Spacing::Xxs))
        .style(move |_: &iced::Theme, status| button::Style {
            background: match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Some(Background::Color(Color {
                        a: 0.08,
                        ..Color::WHITE
                    }))
                }
                _ => None,
            },
            border: Border {
                radius: radius(Radius::Sm).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .into()
}

fn config_edit_footer<'a>(can_save: bool, palette: &'a ForgePalette) -> Element<'a, Message> {
    let p = *palette;
    let cancel = secondary_button(
        forge_widgets::tr!("triggers_sheet_config_cancel"),
        Message::TriggersRegistry(TriggersRegistryMsg::ConfigEditCancelled),
        palette,
    );
    let save_lbl = forge_widgets::tr!("triggers_sheet_config_save");
    let save: Element<'a, Message> = if can_save {
        primary_button(
            save_lbl,
            Message::TriggersRegistry(TriggersRegistryMsg::ConfigEditSubmit),
            palette,
        )
    } else {
        container(
            text(save_lbl)
                .size(FONT_SM)
                .color(Color { a: 0.5, ..p.shell })
                .font(font(FontRole::Body)),
        )
        .padding([sp(Spacing::Xs), sp(Spacing::Md)])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(Color { a: 0.4, ..p.brand })),
            border: Border {
                radius: radius(Radius::Md).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        })
        .into()
    };
    container(
        row![cancel, Space::new().width(Length::Fill), save]
            .spacing(spf(Spacing::Xs))
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .into()
}

fn sheet_platform_section<'a>(
    row: &'a TriggerInstanceRow,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let mono = font(FontRole::Monospace);

    let Some(descriptor) = rt.trigger_registry.get(&row.kind_id) else {
        return Space::new().width(0).height(0).into();
    };

    match descriptor.platform_contract() {
        KindPlatformContract::Universal => Space::new().width(0).height(0).into(),

        KindPlatformContract::PlatformSpecific(pid) => {
            let dot_color = platform_id_color(pid, palette);
            let dot_size = 6.0_f32;
            let dot = container(Space::new().width(dot_size).height(dot_size))
                .width(dot_size)
                .height(dot_size)
                .style(move |_: &iced::Theme| container::Style {
                    background: Some(Background::Color(Color {
                        a: 0.6,
                        ..dot_color
                    })),
                    border: Border {
                        radius: (dot_size / 2.0).into(),
                        color: Color::TRANSPARENT,
                        width: 0.0,
                    },
                    ..container::Style::default()
                });
            let badge = container(
                row![
                    dot,
                    text(platform_id_name(pid))
                        .size(FONT_XS)
                        .color(p.text_muted)
                        .font(font(FontRole::Body)),
                ]
                .spacing(spf(Spacing::Xxs))
                .align_y(Alignment::Center),
            )
            .padding([2, sp(Spacing::Xs)])
            .style(move |_: &iced::Theme| container::Style {
                background: Some(Background::Color(p.surface_overlay)),
                border: Border {
                    radius: radius(Radius::Sm).into(),
                    color: Color::TRANSPARENT,
                    width: 0.0,
                },
                ..container::Style::default()
            });
            let will_fire_str = forge_widgets::tr!(
                "triggers_sheet_will_fire_on",
                platform = platform_id_name(pid)
            );
            let preview = text(will_fire_str)
                .size(FONT_XS)
                .color(p.text_faint)
                .font(mono);
            let divider_style = move |_: &iced::Theme| rule::Style {
                color: p.border_regular,
                radius: 0.0.into(),
                fill_mode: rule::FillMode::Full,
                snap: true,
            };
            column![
                section_header(
                    forge_widgets::tr!("triggers_sheet_section_platform"),
                    None,
                    palette
                ),
                container(badge).padding([sp(Spacing::Xxs), sp(Spacing::Md)]),
                container(preview).padding([2, sp(Spacing::Md)]),
                rule::horizontal(1.0).style(divider_style),
            ]
            .into()
        }

        KindPlatformContract::MultiPlatform => {
            let scope_text = platform_scope_text(&row.platform_scope);
            let will_fire_scope_str =
                forge_widgets::tr!("triggers_sheet_will_fire_on_scope", scope = scope_text);
            let preview = text(will_fire_scope_str)
                .size(FONT_XS)
                .color(p.text_faint)
                .font(mono);

            let any_platform_lbl = forge_widgets::tr!("triggers_sheet_any_platform");
            let scope_badge_el: Element<'_, Message> = match &row.platform_scope {
                PlatformScope::Any => container(
                    text(any_platform_lbl)
                        .size(FONT_XS)
                        .color(p.text_muted)
                        .font(font(FontRole::Body)),
                )
                .padding([2, sp(Spacing::Xs)])
                .style(move |_: &iced::Theme| container::Style {
                    background: Some(Background::Color(p.surface_overlay)),
                    border: Border {
                        radius: radius(Radius::Sm).into(),
                        color: Color::TRANSPARENT,
                        width: 0.0,
                    },
                    ..container::Style::default()
                })
                .into(),
                PlatformScope::Only(set) => {
                    let dot_els: Vec<Element<'_, Message>> = set
                        .iter()
                        .map(|&pid| {
                            let dot_color = platform_id_color(pid, palette);
                            let dot_size = 6.0_f32;
                            let dot = container(Space::new().width(dot_size).height(dot_size))
                                .width(dot_size)
                                .height(dot_size)
                                .style(move |_: &iced::Theme| container::Style {
                                    background: Some(Background::Color(dot_color)),
                                    border: Border {
                                        radius: (dot_size / 2.0).into(),
                                        color: Color::TRANSPARENT,
                                        width: 0.0,
                                    },
                                    ..container::Style::default()
                                });
                            row![
                                dot,
                                text(platform_id_name(pid))
                                    .size(FONT_XS)
                                    .color(p.text_muted)
                                    .font(font(FontRole::Body)),
                            ]
                            .spacing(spf(Spacing::Xxs))
                            .align_y(Alignment::Center)
                            .into()
                        })
                        .collect();
                    iced::widget::row(dot_els)
                        .spacing(spf(Spacing::Xs))
                        .align_y(Alignment::Center)
                        .into()
                }
            };
            let divider_style = move |_: &iced::Theme| rule::Style {
                color: p.border_regular,
                radius: 0.0.into(),
                fill_mode: rule::FillMode::Full,
                snap: true,
            };
            column![
                section_header(
                    forge_widgets::tr!("triggers_sheet_section_platform"),
                    None,
                    palette
                ),
                container(scope_badge_el).padding([sp(Spacing::Xxs), sp(Spacing::Md)]),
                container(preview).padding([2, sp(Spacing::Md)]),
                rule::horizontal(1.0).style(divider_style),
            ]
            .into()
        }
    }
}

fn platform_id_name(p: PlatformId) -> &'static str {
    match p {
        PlatformId::Twitch => "Twitch",
        PlatformId::YouTube => "YouTube",
        PlatformId::Kick => "Kick",
    }
}

fn platform_id_color(p: PlatformId, palette: &ForgePalette) -> Color {
    match p {
        PlatformId::Twitch => palette.platform_twitch,
        PlatformId::YouTube => palette.platform_youtube,
        PlatformId::Kick => palette.platform_kick,
    }
}

fn platform_scope_text(scope: &PlatformScope) -> String {
    match scope {
        PlatformScope::Any => "any platform".to_owned(),
        PlatformScope::Only(set) => {
            let names: Vec<&str> = set.iter().map(|p| platform_id_name(*p)).collect();
            names.join(", ")
        }
    }
}

fn confirm_disable_dialog<'a>(
    cd: &'a ConfirmDisable,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let id = cd.instance_id;

    let backdrop = button(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(Message::TriggersRegistry(
            TriggersRegistryMsg::DisableConfirmDismissed,
        ))
        .padding(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_: &iced::Theme, _status| button::Style {
            background: Some(Background::Color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.5,
            })),
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        });

    let body_text = forge_widgets::tr!(
        "triggers_confirm_disable_body",
        count = cd.action_count as i64
    );

    let card = container(
        column![
            text(body_text)
                .size(FONT_SM)
                .color(p.text_primary)
                .font(font(FontRole::Body)),
            row![
                secondary_button(
                    forge_widgets::tr!("triggers_confirm_disable_dismiss"),
                    Message::TriggersRegistry(TriggersRegistryMsg::DisableConfirmDismissed),
                    palette,
                ),
                Space::new().width(Length::Fill),
                destructive_button(
                    forge_widgets::tr!("triggers_confirm_disable_accept"),
                    Message::TriggersRegistry(TriggersRegistryMsg::DisableConfirmAccepted(id)),
                    palette,
                ),
            ]
            .spacing(spf(Spacing::Xs))
            .align_y(Alignment::Center),
        ]
        .spacing(spf(Spacing::Md))
        .padding(sp(Spacing::Lg)),
    )
    .max_width(400)
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(p.elevated)),
        border: Border {
            color: p.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Lg).into(),
        },
        ..container::Style::default()
    });

    let centered = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    stack![backdrop, centered].into()
}

fn form_field_key(field: &forge_registry::FormField) -> &'static str {
    match field {
        forge_registry::FormField::Text { key, .. } => key,
        forge_registry::FormField::TextArea { key, .. } => key,
        forge_registry::FormField::Integer { key, .. } => key,
        forge_registry::FormField::Toggle { key, .. } => key,
        forge_registry::FormField::Select { key, .. } => key,
        forge_registry::FormField::DynamicSelect { key, .. } => key,
        forge_registry::FormField::Optional { key, .. } => key,
        forge_registry::FormField::SubChain { key, .. } => key,
        forge_registry::FormField::CaseList { key, .. } => key,
    }
}

fn form_field_label(field: &forge_registry::FormField) -> &'static str {
    match field {
        forge_registry::FormField::Text { label, .. } => label,
        forge_registry::FormField::TextArea { label, .. } => label,
        forge_registry::FormField::Integer { label, .. } => label,
        forge_registry::FormField::Toggle { label, .. } => label,
        forge_registry::FormField::Select { label, .. } => label,
        forge_registry::FormField::DynamicSelect { label, .. } => label,
        forge_registry::FormField::Optional { label, .. } => label,
        forge_registry::FormField::SubChain { label, .. } => label,
        forge_registry::FormField::CaseList { label, .. } => label,
    }
}

fn variant_one_line(v: &forge_types::Variant) -> String {
    match v {
        forge_types::Variant::Int(n) => n.to_string(),
        forge_types::Variant::Float(f) => f.to_string(),
        forge_types::Variant::Bool(b) => b.to_string(),
        forge_types::Variant::String(s) => s.clone(),
        forge_types::Variant::Datetime(dt) => dt.to_string(),
        forge_types::Variant::Array(a) => format!("[{} items]", a.len()),
        forge_types::Variant::Object(m) => format!("{{{} keys}}", m.len()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_row(kind_id: &str, used_in_count: usize) -> TriggerInstanceRow {
        TriggerInstanceRow {
            id: TriggerInstanceId::new(),
            name: "Test".to_owned(),
            kind_id: kind_id.to_owned(),
            enabled: true,
            used_in_count,
            overrides: Default::default(),
            platform_scope: PlatformScope::Any,
        }
    }

    #[test]
    fn confirm_disable_stores_action_count() {
        let mut state = TriggersRegistryState::default();
        let row = make_row("twitch.chat.command", 2);
        let id = row.id;
        state.instances.push(row);

        let count = state
            .instances
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.used_in_count)
            .unwrap_or(0);
        assert_eq!(count, 2);
        state.confirm_disable = Some(ConfirmDisable {
            instance_id: id,
            action_count: count,
        });
        assert_eq!(state.confirm_disable.as_ref().unwrap().action_count, 2);
    }

    #[test]
    fn variant_one_line_formats_types() {
        use forge_types::Variant;
        assert_eq!(variant_one_line(&Variant::Int(42)), "42");
        assert_eq!(variant_one_line(&Variant::Bool(true)), "true");
        assert_eq!(variant_one_line(&Variant::String("hi".to_owned())), "hi");
        assert_eq!(
            variant_one_line(&Variant::Array(vec![Variant::Int(1)])),
            "[1 items]"
        );
    }

    #[test]
    fn platform_scope_text_labels_any_and_only_variants() {
        assert_eq!(platform_scope_text(&PlatformScope::Any), "any platform");
        let mut set = std::collections::BTreeSet::new();
        set.insert(PlatformId::YouTube);
        let scope = PlatformScope::only(set).unwrap();
        assert_eq!(platform_scope_text(&scope), "YouTube");
    }
}
