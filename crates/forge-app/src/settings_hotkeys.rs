use std::collections::BTreeMap;
use std::sync::Arc;

use forge_hotkey::{HotkeyClient, HotkeyCombo, HotkeyId};
use forge_storage::DataProvider;
use forge_types::{Action, ActionId, PlatformScope, TriggerInstance, TriggerInstanceId, Variant};
use forge_widgets::{
    ForgePalette,
    icons::{Icon, tabler_icon},
    key_capture,
    sections::section_header,
    tokens::{BORDER_THIN, FONT_SM, FontRole, Radius, Spacing, font, radius, spf},
};
use iced::{
    Background, Border, Color, Element, Length, Task,
    widget::{Space, button, column, container, pick_list, row, scrollable, text},
};

use crate::Message;
use crate::runtime_view::RuntimeView;

#[derive(Debug, Clone)]
pub struct HotkeyBinding {
    pub hotkey_id: HotkeyId,
    pub combo: String,
    pub action_id: Option<ActionId>,
    pub action_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConflictModal {
    pub combo: String,
    pub existing_hotkey_id: Option<HotkeyId>,
}

#[derive(Default)]
pub struct SettingsHotkeysState {
    pub bindings: Vec<HotkeyBinding>,
    pub actions_list: Vec<(ActionId, String)>,
    pub captured_combo: Option<String>,
    pub selected_action_name: Option<String>,
    pub bind_error: Option<String>,
    pub bindings_loading: bool,
    pub bind_in_progress: bool,
    pub conflict_modal: Option<ConflictModal>,
}

#[derive(Debug, Clone)]
pub enum SettingsHotkeysMsg {
    Enter,
    ComboCaptured(String),
    ComboReset,
    ActionPicked(String),
    ActionsLoaded(Result<Vec<(ActionId, String)>, String>),
    BindingsLoaded(Result<Vec<HotkeyBinding>, String>),
    BindClicked,
    BindResult(Result<HotkeyId, String>),
    UnbindClicked(HotkeyId),
    UnbindResult(Result<(), String>),
    ConflictReplace,
    ConflictCancel,
    ReplaceResult(Result<HotkeyId, String>),
    DismissError,
}

pub fn update(
    state: &mut SettingsHotkeysState,
    rt: &RuntimeView,
    msg: SettingsHotkeysMsg,
) -> Task<Message> {
    match msg {
        SettingsHotkeysMsg::Enter => {
            state.bindings_loading = true;
            let backend = Arc::clone(&rt.backend);
            let actions_task = Task::perform(async move { load_actions(backend).await }, |r| {
                Message::SettingsHotkeys(SettingsHotkeysMsg::ActionsLoaded(r))
            });
            let client_opt = rt.hotkey_client.clone();
            let backend2 = Arc::clone(&rt.backend);
            let bindings_task = Task::perform(
                async move { load_bindings(client_opt, backend2).await },
                |r| Message::SettingsHotkeys(SettingsHotkeysMsg::BindingsLoaded(r)),
            );
            Task::batch([actions_task, bindings_task])
        }

        SettingsHotkeysMsg::ComboCaptured(combo) => {
            state.captured_combo = Some(combo);
            state.bind_error = None;
            Task::none()
        }

        SettingsHotkeysMsg::ComboReset => {
            state.captured_combo = None;
            state.bind_error = None;
            Task::none()
        }

        SettingsHotkeysMsg::ActionPicked(name) => {
            state.selected_action_name = Some(name);
            Task::none()
        }

        SettingsHotkeysMsg::ActionsLoaded(Ok(list)) => {
            state.actions_list = list;
            Task::none()
        }

        SettingsHotkeysMsg::ActionsLoaded(Err(e)) => {
            state.bind_error = Some(forge_widgets::tr!(
                "settings_hotkeys_error_load_actions",
                error = e.as_str()
            ));
            Task::none()
        }

        SettingsHotkeysMsg::BindingsLoaded(Ok(bindings)) => {
            state.bindings = bindings;
            state.bindings_loading = false;
            Task::none()
        }

        SettingsHotkeysMsg::BindingsLoaded(Err(e)) => {
            state.bind_error = Some(forge_widgets::tr!(
                "settings_hotkeys_error_load_bindings",
                error = e.as_str()
            ));
            state.bindings_loading = false;
            Task::none()
        }

        SettingsHotkeysMsg::BindClicked => {
            let Some(combo_str) = state.captured_combo.clone() else {
                state.bind_error = Some(forge_widgets::tr!("settings_hotkeys_error_no_combo"));
                return Task::none();
            };
            let action_id = state
                .selected_action_name
                .as_ref()
                .and_then(|name| state.actions_list.iter().find(|(_, n)| n == name))
                .map(|(id, _)| *id);
            let Some(action_id) = action_id else {
                state.bind_error = Some(forge_widgets::tr!("settings_hotkeys_error_no_action"));
                return Task::none();
            };
            let Some(client) = rt.hotkey_client.clone() else {
                state.bind_error = Some(forge_widgets::tr!("settings_hotkeys_error_unavailable"));
                return Task::none();
            };
            state.bind_in_progress = true;
            state.bind_error = None;
            let backend = Arc::clone(&rt.backend);
            Task::perform(
                async move { do_bind(client, backend, combo_str, action_id).await },
                |r| Message::SettingsHotkeys(SettingsHotkeysMsg::BindResult(r)),
            )
        }

        SettingsHotkeysMsg::BindResult(Ok(_id)) => {
            state.bind_in_progress = false;
            state.captured_combo = None;
            state.selected_action_name = None;
            let client = rt.hotkey_client.clone();
            let backend = Arc::clone(&rt.backend);
            Task::perform(async move { load_bindings(client, backend).await }, |r| {
                Message::SettingsHotkeys(SettingsHotkeysMsg::BindingsLoaded(r))
            })
        }

        SettingsHotkeysMsg::BindResult(Err(e)) => {
            state.bind_in_progress = false;
            if let Some(combo) = already_registered_combo(&e) {
                let existing_id = state
                    .bindings
                    .iter()
                    .find(|b| b.combo == combo)
                    .map(|b| b.hotkey_id);
                state.conflict_modal = Some(ConflictModal {
                    combo,
                    existing_hotkey_id: existing_id,
                });
            } else {
                state.bind_error = Some(e);
            }
            Task::none()
        }

        SettingsHotkeysMsg::UnbindClicked(hotkey_id) => {
            let Some(client) = rt.hotkey_client.clone() else {
                return Task::none();
            };
            Task::perform(
                async move {
                    client
                        .unregister(hotkey_id)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::SettingsHotkeys(SettingsHotkeysMsg::UnbindResult(r)),
            )
        }

        SettingsHotkeysMsg::UnbindResult(Ok(())) => {
            let client = rt.hotkey_client.clone();
            let backend = Arc::clone(&rt.backend);
            Task::perform(async move { load_bindings(client, backend).await }, |r| {
                Message::SettingsHotkeys(SettingsHotkeysMsg::BindingsLoaded(r))
            })
        }

        SettingsHotkeysMsg::UnbindResult(Err(e)) => {
            state.bind_error = Some(forge_widgets::tr!(
                "settings_hotkeys_error_unbind",
                error = e.as_str()
            ));
            Task::none()
        }

        SettingsHotkeysMsg::ConflictReplace => {
            let Some(modal) = state.conflict_modal.take() else {
                return Task::none();
            };
            let Some(existing_id) = modal.existing_hotkey_id else {
                state.bind_error = Some(forge_widgets::tr!(
                    "settings_hotkeys_error_conflict_not_found"
                ));
                return Task::none();
            };
            let Some(combo_str) = state.captured_combo.clone() else {
                return Task::none();
            };
            let action_id = state
                .selected_action_name
                .as_ref()
                .and_then(|name| state.actions_list.iter().find(|(_, n)| n == name))
                .map(|(id, _)| *id);
            let Some(action_id) = action_id else {
                return Task::none();
            };
            let Some(client) = rt.hotkey_client.clone() else {
                return Task::none();
            };
            let backend = Arc::clone(&rt.backend);
            state.bind_in_progress = true;
            Task::perform(
                async move { do_replace(client, backend, existing_id, combo_str, action_id).await },
                |r| Message::SettingsHotkeys(SettingsHotkeysMsg::ReplaceResult(r)),
            )
        }

        SettingsHotkeysMsg::ConflictCancel => {
            state.conflict_modal = None;
            Task::none()
        }

        SettingsHotkeysMsg::ReplaceResult(Ok(_id)) => {
            state.bind_in_progress = false;
            state.captured_combo = None;
            state.selected_action_name = None;
            let client = rt.hotkey_client.clone();
            let backend = Arc::clone(&rt.backend);
            Task::perform(async move { load_bindings(client, backend).await }, |r| {
                Message::SettingsHotkeys(SettingsHotkeysMsg::BindingsLoaded(r))
            })
        }

        SettingsHotkeysMsg::ReplaceResult(Err(e)) => {
            state.bind_in_progress = false;
            state.bind_error = Some(forge_widgets::tr!(
                "settings_hotkeys_error_replace",
                error = e.as_str()
            ));
            Task::none()
        }

        SettingsHotkeysMsg::DismissError => {
            state.bind_error = None;
            Task::none()
        }
    }
}

pub fn view<'a>(
    state: &'a SettingsHotkeysState,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let portal_status = rt.hotkey_client.as_ref().and_then(|c| c.portal_available());

    let action_names: Vec<String> = state
        .actions_list
        .iter()
        .map(|(_, name)| name.clone())
        .collect();

    let capture_widget: Element<'a, Message> = key_capture(palette)
        .value(state.captured_combo.as_deref())
        .on_captured(|s| Message::SettingsHotkeys(SettingsHotkeysMsg::ComboCaptured(s)))
        .on_reset(|| Message::SettingsHotkeys(SettingsHotkeysMsg::ComboReset))
        .into();

    let can_bind = state.captured_combo.is_some()
        && state.selected_action_name.is_some()
        && !state.bind_in_progress;

    let bind_btn = {
        let label = text(forge_widgets::tr!("settings_hotkeys_bind_btn"))
            .size(FONT_SM)
            .font(font(FontRole::Body))
            .color(palette.text_primary);
        let btn = button(
            container(label)
                .padding([8_u16, 14_u16])
                .style(move |_| container::Style::default()),
        )
        .style(move |_, _| {
            let bg = if can_bind {
                palette.brand
            } else {
                palette.surface_overlay
            };
            let text_col = if can_bind {
                palette.shell
            } else {
                palette.text_faint
            };
            button::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: radius(Radius::Sm).into(),
                },
                text_color: text_col,
                shadow: Default::default(),
                ..button::Style::default()
            }
        });
        if can_bind {
            btn.on_press(Message::SettingsHotkeys(SettingsHotkeysMsg::BindClicked))
        } else {
            btn
        }
    };

    let action_picker = pick_list(action_names, state.selected_action_name.clone(), |name| {
        Message::SettingsHotkeys(SettingsHotkeysMsg::ActionPicked(name))
    })
    .placeholder(forge_widgets::tr!("settings_hotkeys_select_action"))
    .text_size(FONT_SM)
    .width(Length::Fixed(240.0));

    let bind_row = row![capture_widget, action_picker, bind_btn]
        .spacing(spf(Spacing::Sm))
        .align_y(iced::Alignment::Center);

    let bind_section = column![
        section_header(
            forge_widgets::tr!("settings_hotkeys_bind_section"),
            None,
            palette
        ),
        bind_row,
        bind_error_el(state, palette),
    ]
    .spacing(spf(Spacing::Sm));

    let bindings_list = bindings_list_view(&state.bindings, palette);

    let registered_section = column![
        section_header(
            forge_widgets::tr!("settings_hotkeys_registered_section"),
            Some(state.bindings.len() as u32),
            palette,
        ),
        bindings_list,
    ]
    .spacing(spf(Spacing::Sm));

    let backend_label = portal_status_label(portal_status);
    let backend_section = column![
        section_header(
            forge_widgets::tr!("settings_hotkeys_backend_section"),
            None,
            palette,
        ),
        container(
            text(backend_label)
                .size(FONT_SM)
                .font(font(FontRole::Monospace))
                .color(palette.text_secondary),
        )
        .padding([6_u16, 0_u16]),
    ]
    .spacing(spf(Spacing::Sm));

    let scope_subtitle = text(forge_widgets::tr!("settings_hotkeys_scope_subtitle"))
        .size(FONT_SM)
        .color(palette.text_muted);

    let body = column![
        scope_subtitle,
        Space::new().height(spf(Spacing::Xs)),
        bind_section,
        Space::new().height(spf(Spacing::Md)),
        registered_section,
        Space::new().height(spf(Spacing::Md)),
        backend_section,
    ]
    .padding([16_u16, 20_u16])
    .spacing(spf(Spacing::Xs))
    .width(Length::Fill);

    let scrollable_body = scrollable(body).height(Length::Fill);

    let page: Element<'a, Message> = container(scrollable_body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    if let Some(modal) = &state.conflict_modal {
        iced::widget::stack![page, conflict_overlay(modal, palette)].into()
    } else {
        page
    }
}

fn bind_error_el<'a>(
    state: &'a SettingsHotkeysState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    if let Some(err) = &state.bind_error {
        let err_row = row![
            tabler_icon(Icon::AlertTriangle, 13.0, palette.warning),
            text(err.as_str())
                .size(FONT_SM)
                .font(font(FontRole::Body))
                .color(palette.warning),
            Space::new().width(Length::Fill),
            button(text("\u{00D7}").size(FONT_SM).color(palette.text_muted))
                .on_press(Message::SettingsHotkeys(SettingsHotkeysMsg::DismissError))
                .style(|_, _| button::Style {
                    background: Some(Background::Color(Color::TRANSPARENT)),
                    border: Border::default(),
                    text_color: palette.text_muted,
                    ..button::Style::default()
                }),
        ]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::Alignment::Center);

        container(err_row)
            .padding([6_u16, 10_u16])
            .style(move |_| container::Style {
                background: Some(Background::Color(Color {
                    a: 0.1,
                    ..palette.warning
                })),
                border: Border {
                    color: palette.warning,
                    width: BORDER_THIN,
                    radius: radius(Radius::Sm).into(),
                },
                ..container::Style::default()
            })
            .into()
    } else {
        Space::new().height(0).into()
    }
}

fn bindings_list_view<'a>(
    bindings: &'a [HotkeyBinding],
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    if bindings.is_empty() {
        return container(
            text(forge_widgets::tr!("settings_hotkeys_no_bindings"))
                .size(FONT_SM)
                .font(font(FontRole::Body))
                .color(palette.text_faint),
        )
        .padding([8_u16, 0_u16])
        .into();
    }

    let rows: Vec<Element<'a, Message>> =
        bindings.iter().map(|b| binding_row(b, palette)).collect();

    column(rows).spacing(2.0).into()
}

fn binding_row<'a>(binding: &'a HotkeyBinding, palette: &'a ForgePalette) -> Element<'a, Message> {
    let combo_text = text(binding.combo.as_str())
        .size(FONT_SM)
        .font(font(FontRole::Monospace))
        .color(palette.text_primary);

    let action_text = if let Some(name) = binding.action_name.as_deref() {
        text(name)
            .size(FONT_SM)
            .font(font(FontRole::Body))
            .color(palette.text_secondary)
    } else {
        text("\u{2014}")
            .size(FONT_SM)
            .font(font(FontRole::Body))
            .color(palette.text_faint)
    };

    let hotkey_id = binding.hotkey_id;
    let remove_btn = button(tabler_icon(Icon::X, 13.0, palette.text_muted))
        .on_press(Message::SettingsHotkeys(SettingsHotkeysMsg::UnbindClicked(
            hotkey_id,
        )))
        .style(|_, _| button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border::default(),
            text_color: palette.text_muted,
            ..button::Style::default()
        });

    let row_content = row![
        combo_text,
        Space::new().width(spf(Spacing::Sm)),
        tabler_icon(Icon::ArrowRight, 11.0, palette.text_faint),
        Space::new().width(spf(Spacing::Sm)),
        action_text,
        Space::new().width(Length::Fill),
        remove_btn,
    ]
    .align_y(iced::Alignment::Center);

    container(row_content)
        .padding([6_u16, 8_u16])
        .style(move |_| container::Style {
            background: Some(Background::Color(palette.surface_overlay)),
            border: Border {
                color: palette.border_regular,
                width: BORDER_THIN,
                radius: radius(Radius::Sm).into(),
            },
            ..container::Style::default()
        })
        .width(Length::Fill)
        .into()
}

fn conflict_overlay<'a>(
    modal: &'a ConflictModal,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let backdrop = button(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(Message::SettingsHotkeys(SettingsHotkeysMsg::ConflictCancel))
        .style(|_, _| button::Style {
            background: Some(Background::Color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.5,
            })),
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            ..button::Style::default()
        })
        .width(Length::Fill)
        .height(Length::Fill);

    let combo_display = text(modal.combo.as_str())
        .size(FONT_SM)
        .font(font(FontRole::Monospace))
        .color(palette.warning);

    let body_text = row![
        text(forge_widgets::tr!("settings_hotkeys_conflict_body_prefix"))
            .size(FONT_SM)
            .font(font(FontRole::Body))
            .color(palette.text_secondary),
        combo_display,
        text(forge_widgets::tr!("settings_hotkeys_conflict_body_suffix"))
            .size(FONT_SM)
            .font(font(FontRole::Body))
            .color(palette.text_secondary),
    ];

    let cancel_btn = button(
        text(forge_widgets::tr!("common_cancel"))
            .size(FONT_SM)
            .font(font(FontRole::Body))
            .color(palette.text_secondary),
    )
    .on_press(Message::SettingsHotkeys(SettingsHotkeysMsg::ConflictCancel))
    .padding([8_u16, 14_u16])
    .style(|_, _| button::Style {
        background: Some(Background::Color(palette.surface_overlay)),
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Sm).into(),
        },
        text_color: palette.text_secondary,
        ..button::Style::default()
    });

    let replace_btn = button(
        text(forge_widgets::tr!("settings_hotkeys_replace_btn"))
            .size(FONT_SM)
            .font(font(FontRole::Body))
            .color(palette.shell),
    )
    .on_press(Message::SettingsHotkeys(
        SettingsHotkeysMsg::ConflictReplace,
    ))
    .padding([8_u16, 14_u16])
    .style(|_, _| button::Style {
        background: Some(Background::Color(palette.warning)),
        border: Border::default(),
        text_color: palette.shell,
        ..button::Style::default()
    });

    let btn_row = row![cancel_btn, replace_btn]
        .spacing(spf(Spacing::Sm))
        .align_y(iced::Alignment::Center);

    let card = container(
        column![body_text, Space::new().height(spf(Spacing::Md)), btn_row]
            .spacing(spf(Spacing::Xs)),
    )
    .padding([20_u16, 24_u16])
    .max_width(460.0)
    .style(move |_| container::Style {
        background: Some(Background::Color(palette.elevated)),
        border: Border {
            color: palette.border_regular,
            width: BORDER_THIN,
            radius: radius(Radius::Lg).into(),
        },
        ..container::Style::default()
    });

    let centered = container(iced::widget::opaque(card))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center);

    iced::widget::stack![backdrop, centered].into()
}

fn portal_status_label(available: Option<bool>) -> String {
    match available {
        Some(true) => "Portal (Wayland GlobalShortcuts) — active".to_owned(),
        Some(false) => "Evdev / X11 fallback — active".to_owned(),
        None => "N/A (Windows / macOS native)".to_owned(),
    }
}

fn already_registered_combo(err: &str) -> Option<String> {
    if err.contains("already registered:") {
        err.split("already registered:")
            .nth(1)
            .map(|s| s.trim().trim_matches('"').to_owned())
    } else {
        None
    }
}

async fn load_actions(backend: Arc<dyn DataProvider>) -> Result<Vec<(ActionId, String)>, String> {
    let actions: Vec<Action> = backend
        .action_repo()
        .list()
        .await
        .map_err(|e| e.to_string())?;
    Ok(actions.into_iter().map(|a| (a.id, a.name)).collect())
}

async fn load_bindings(
    client: Option<Arc<HotkeyClient>>,
    backend: Arc<dyn DataProvider>,
) -> Result<Vec<HotkeyBinding>, String> {
    let registered = client
        .as_ref()
        .map(|c| c.registered_combos())
        .unwrap_or_default();

    let instances = backend
        .trigger_instance_repo()
        .list_all()
        .await
        .map_err(|e| e.to_string())?;

    let combo_to_action: std::collections::HashMap<String, (ActionId, String)> = {
        let mut map = std::collections::HashMap::new();
        for instance in &instances {
            if instance.kind_id != "hotkey.global.pressed" {
                continue;
            }
            let Some(Variant::String(combo)) = instance.overrides.get("combo") else {
                continue;
            };
            let action_ids = backend
                .trigger_instance_repo()
                .actions_using(instance.id)
                .await
                .map_err(|e| e.to_string())?;
            if let Some(aid) = action_ids.into_iter().next() {
                let action = backend
                    .action_repo()
                    .get(aid)
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(a) = action {
                    map.insert(combo.clone(), (a.id, a.name));
                }
            }
        }
        map
    };

    let bindings = registered
        .into_iter()
        .map(|(id, combo)| {
            let combo_str = combo.as_str().to_owned();
            let (action_id, action_name) = combo_to_action
                .get(&combo_str)
                .map(|(aid, name)| (Some(*aid), Some(name.clone())))
                .unwrap_or((None, None));
            HotkeyBinding {
                hotkey_id: id,
                combo: combo_str,
                action_id,
                action_name,
            }
        })
        .collect();

    Ok(bindings)
}

async fn cleanup_stale_combo_instances(
    backend: &Arc<dyn DataProvider>,
    combo_str: &str,
) -> Result<(), String> {
    let instances = backend
        .trigger_instance_repo()
        .list_all()
        .await
        .map_err(|e| e.to_string())?;

    for instance in instances {
        if instance.kind_id != "hotkey.global.pressed" {
            continue;
        }
        let Some(Variant::String(existing_combo)) = instance.overrides.get("combo") else {
            continue;
        };
        if existing_combo != combo_str {
            continue;
        }
        let action_ids = backend
            .trigger_instance_repo()
            .actions_using(instance.id)
            .await
            .map_err(|e| e.to_string())?;
        for aid in action_ids {
            backend
                .trigger_instance_repo()
                .unlink_action(aid, instance.id)
                .await
                .map_err(|e| e.to_string())?;
        }
        backend
            .trigger_instance_repo()
            .delete(instance.id)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

async fn do_bind(
    client: Arc<HotkeyClient>,
    backend: Arc<dyn DataProvider>,
    combo_str: String,
    action_id: ActionId,
) -> Result<HotkeyId, String> {
    let combo = HotkeyCombo::parse(&combo_str).map_err(|e| e.to_string())?;
    let id = client.register(combo).await.map_err(|e| e.to_string())?;

    cleanup_stale_combo_instances(&backend, &combo_str).await?;

    let mut overrides = BTreeMap::new();
    overrides.insert("combo".to_owned(), Variant::String(combo_str.clone()));

    let instance = TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: "hotkey.global.pressed".to_owned(),
        name: combo_str.clone(),
        overrides,
        enabled: true,
        user_defined: true,
        platform_scope: PlatformScope::default(),
    };
    backend
        .trigger_instance_repo()
        .save(&instance)
        .await
        .map_err(|e| e.to_string())?;
    backend
        .trigger_instance_repo()
        .link_action(action_id, instance.id, 0)
        .await
        .map_err(|e| e.to_string())?;

    Ok(id)
}

async fn do_replace(
    client: Arc<HotkeyClient>,
    backend: Arc<dyn DataProvider>,
    existing_id: HotkeyId,
    combo_str: String,
    action_id: ActionId,
) -> Result<HotkeyId, String> {
    client
        .unregister(existing_id)
        .await
        .map_err(|e| e.to_string())?;
    do_bind(client, backend, combo_str, action_id).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use forge_runtime::EventBus;
    use forge_storage::{CredentialsRepo, DataProvider};
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::ExecutionMode;

    use super::*;
    use crate::runtime_view::RuntimeView;
    use crate::server_subsystem::ServerSubsystem;

    fn test_rt() -> RuntimeView {
        let tokio_rt = tokio::runtime::Runtime::new().unwrap();
        let backend = Arc::new(
            tokio_rt
                .block_on(SqliteBackend::open_with_key("sqlite::memory:", [0xab; 32]))
                .unwrap(),
        );
        let server_subsystem = Arc::new(ServerSubsystem::new(
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>
        ));
        let backend: Arc<dyn DataProvider> = backend;
        RuntimeView {
            actions: Arc::new(forge_runtime::actions::ActionsService::new(
                backend.action_repo(),
                backend.queue_repo(),
                backend.history_repo(),
                backend.trigger_instance_repo(),
                backend.soundboard_clips_repo(),
            )),
            backend,
            bus: EventBus::new(Arc::new(forge_runtime::NullEventLogRepo)),
            script_registry: Arc::new(forge_runtime::ScriptRegistry::new()),
            server_subsystem,
            action_engine: None,
            scheduler: None,
            obs_client: None,
            vtube_client: None,
            vtube_sink: forge_vtube::SwitchableVTubeSink::new(),
            obs_sink: forge_obs::SwitchableObsSink::new(),
            discord_client: None,
            midi_client: None,
            hotkey_client: None,
            speak_queue: None,
            pipeline_config: None,
            tts_trigger_settings: None,
            sound_player: None,
            twitch_builtin: None,
            kick_builtin: None,
            youtube_builtin: None,
            platform_connection: std::collections::BTreeMap::new(),
            twitch_flow: None,
            youtube_flow: None,
            kick_flow: None,
            tts_engine_ids: Vec::new(),
            tts_registry: None,
            live_viewers: None,
            twitch_login: None,
            twitch_token_expires: None,
            twitch_reauth_required: false,
            sub_action_registry: Arc::new(forge_registry::SubActionRegistry::new()),
            trigger_registry: Arc::new(forge_registry::TriggerRegistry::new()),
        }
    }

    #[test]
    fn combo_captured_stores_combo() {
        let rt = test_rt();
        let mut state = SettingsHotkeysState::default();
        let _ = update(
            &mut state,
            &rt,
            SettingsHotkeysMsg::ComboCaptured("Ctrl+Shift+A".to_owned()),
        );
        assert_eq!(state.captured_combo.as_deref(), Some("Ctrl+Shift+A"));
    }

    #[test]
    fn combo_reset_clears_state() {
        let rt = test_rt();
        let mut state = SettingsHotkeysState {
            captured_combo: Some("Ctrl+A".to_owned()),
            bind_error: Some("some error".to_owned()),
            ..Default::default()
        };
        let _ = update(&mut state, &rt, SettingsHotkeysMsg::ComboReset);
        assert!(state.captured_combo.is_none());
        assert!(state.bind_error.is_none());
    }

    #[test]
    fn action_picked_stores_name() {
        let rt = test_rt();
        let mut state = SettingsHotkeysState::default();
        let _ = update(
            &mut state,
            &rt,
            SettingsHotkeysMsg::ActionPicked("My Action".to_owned()),
        );
        assert_eq!(state.selected_action_name.as_deref(), Some("My Action"));
    }

    #[test]
    fn bind_clicked_without_combo_sets_error() {
        let rt = test_rt();
        let mut state = SettingsHotkeysState::default();
        let _ = update(&mut state, &rt, SettingsHotkeysMsg::BindClicked);
        assert!(state.bind_error.is_some());
    }

    #[test]
    fn bind_clicked_without_action_sets_error() {
        let rt = test_rt();
        let mut state = SettingsHotkeysState {
            captured_combo: Some("Ctrl+A".to_owned()),
            ..Default::default()
        };
        let _ = update(&mut state, &rt, SettingsHotkeysMsg::BindClicked);
        assert!(state.bind_error.is_some());
    }

    #[test]
    fn bind_result_ok_clears_captured_state() {
        let rt = test_rt();
        let mut state = SettingsHotkeysState {
            captured_combo: Some("Ctrl+A".to_owned()),
            bind_in_progress: true,
            ..Default::default()
        };
        let _ = update(
            &mut state,
            &rt,
            SettingsHotkeysMsg::BindResult(Ok(HotkeyId(1))),
        );
        assert!(state.captured_combo.is_none());
        assert!(!state.bind_in_progress);
    }

    #[test]
    fn bind_result_conflict_opens_modal() {
        let rt = test_rt();
        let mut state = SettingsHotkeysState::default();
        let err = "combo already registered: \"Ctrl+A\"".to_owned();
        let _ = update(&mut state, &rt, SettingsHotkeysMsg::BindResult(Err(err)));
        assert!(state.conflict_modal.is_some());
        let modal = state.conflict_modal.unwrap();
        assert_eq!(modal.combo, "Ctrl+A");
    }

    #[test]
    fn conflict_cancel_clears_modal() {
        let rt = test_rt();
        let mut state = SettingsHotkeysState {
            conflict_modal: Some(ConflictModal {
                combo: "Ctrl+A".to_owned(),
                existing_hotkey_id: Some(HotkeyId(1)),
            }),
            ..Default::default()
        };
        let _ = update(&mut state, &rt, SettingsHotkeysMsg::ConflictCancel);
        assert!(state.conflict_modal.is_none());
    }

    #[test]
    fn bindings_loaded_ok_updates_state() {
        let rt = test_rt();
        let mut state = SettingsHotkeysState {
            bindings_loading: true,
            ..Default::default()
        };
        let bindings = vec![HotkeyBinding {
            hotkey_id: HotkeyId(1),
            combo: "Ctrl+F1".to_owned(),
            action_id: None,
            action_name: None,
        }];
        let _ = update(
            &mut state,
            &rt,
            SettingsHotkeysMsg::BindingsLoaded(Ok(bindings)),
        );
        assert_eq!(state.bindings.len(), 1);
        assert!(!state.bindings_loading);
    }

    #[test]
    fn dismiss_error_clears_bind_error() {
        let rt = test_rt();
        let mut state = SettingsHotkeysState {
            bind_error: Some("something failed".to_owned()),
            ..Default::default()
        };
        let _ = update(&mut state, &rt, SettingsHotkeysMsg::DismissError);
        assert!(state.bind_error.is_none());
    }

    #[test]
    fn portal_status_label_matches_available_flag() {
        assert!(portal_status_label(Some(true)).contains("Portal"));
        assert!(portal_status_label(Some(false)).contains("fallback"));
        assert!(portal_status_label(None).contains("N/A"));
    }

    #[test]
    fn conflict_replace_with_no_cached_id_sets_error_and_closes_modal() {
        let rt = test_rt();
        let mut state = SettingsHotkeysState {
            captured_combo: Some("Ctrl+A".to_owned()),
            conflict_modal: Some(ConflictModal {
                combo: "Ctrl+A".to_owned(),
                existing_hotkey_id: None,
            }),
            ..Default::default()
        };
        let _ = update(&mut state, &rt, SettingsHotkeysMsg::ConflictReplace);
        assert!(state.conflict_modal.is_none());
        assert!(state.bind_error.is_some());
    }

    // --- PLATFORMS-8 / SY-09-F22 orphan-cleanup regression -------------------
    // Guards `cleanup_stale_combo_instances`: re-binding a key must UNLINK the
    // prior persisted `hotkey.global.pressed` row before DELETE (delete is
    // FK-blocked while an action still references it), and must touch ONLY rows
    // whose `combo` override equals the target. Boot re-registration is out of
    // scope here (needs a HotkeyClient/OS mock) — this exercises the storage
    // effect the fix relies on directly.

    async fn mem_backend() -> Arc<dyn DataProvider> {
        Arc::new(
            SqliteBackend::open_with_key("sqlite::memory:", [0xcd; 32])
                .await
                .unwrap(),
        )
    }

    async fn insert_action(backend: &Arc<dyn DataProvider>, name: &str) -> ActionId {
        let queue_id = backend
            .queue_repo()
            .get_by_name("Default")
            .await
            .unwrap()
            .unwrap()
            .id;
        let action = Action {
            id: ActionId::new(),
            name: name.to_owned(),
            group: None,
            queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![],
        };
        let id = action.id;
        backend.action_repo().save(&action).await.unwrap();
        id
    }

    async fn insert_hotkey_instance(
        backend: &Arc<dyn DataProvider>,
        combo: &str,
    ) -> TriggerInstanceId {
        let mut overrides = BTreeMap::new();
        overrides.insert("combo".to_owned(), Variant::String(combo.to_owned()));
        let inst = TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: "hotkey.global.pressed".to_owned(),
            name: combo.to_owned(),
            overrides,
            enabled: true,
            user_defined: true,
            platform_scope: PlatformScope::default(),
        };
        let id = inst.id;
        backend.trigger_instance_repo().save(&inst).await.unwrap();
        id
    }

    #[tokio::test]
    async fn cleanup_unlinks_then_deletes_matching_combo_instance() {
        let backend = mem_backend().await;
        let action_id = insert_action(&backend, "Bound Action").await;
        let inst_id = insert_hotkey_instance(&backend, "Ctrl+Shift+X").await;
        backend
            .trigger_instance_repo()
            .link_action(action_id, inst_id, 0)
            .await
            .unwrap();

        // A naive delete-without-unlink would FK-block and surface an Err here.
        cleanup_stale_combo_instances(&backend, "Ctrl+Shift+X")
            .await
            .expect("cleanup must unlink before delete, not FK-block");

        assert!(
            backend
                .trigger_instance_repo()
                .get(inst_id)
                .await
                .unwrap()
                .is_none(),
            "stale hotkey instance must be deleted"
        );
        assert!(
            backend
                .action_repo()
                .get(action_id)
                .await
                .unwrap()
                .is_some(),
            "linked action must survive orphan cleanup (unlink, not cascade-delete)"
        );
    }

    #[tokio::test]
    async fn cleanup_leaves_instance_with_different_combo_intact() {
        let backend = mem_backend().await;
        let action_x = insert_action(&backend, "X Action").await;
        let action_y = insert_action(&backend, "Y Action").await;
        let inst_x = insert_hotkey_instance(&backend, "Ctrl+X").await;
        let inst_y = insert_hotkey_instance(&backend, "Ctrl+Y").await;
        let repo = backend.trigger_instance_repo();
        repo.link_action(action_x, inst_x, 0).await.unwrap();
        repo.link_action(action_y, inst_y, 0).await.unwrap();

        cleanup_stale_combo_instances(&backend, "Ctrl+X")
            .await
            .unwrap();

        assert!(
            repo.get(inst_y).await.unwrap().is_some(),
            "different-combo instance must not be deleted"
        );
        assert_eq!(
            repo.actions_using(inst_y).await.unwrap(),
            vec![action_y],
            "different-combo instance must keep its action link"
        );
    }
}
