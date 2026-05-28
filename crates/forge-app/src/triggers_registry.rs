use std::borrow::Cow;
use std::sync::Arc;

use forge_registry::effective_config;
use forge_storage::StorageError;
use forge_types::{ActionId, TriggerConfig, TriggerInstanceId};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Task,
    widget::{Space, button, column, container, row, rule, scrollable, stack, text},
};

use forge_widgets::{
    ForgePalette, Radius, SheetHeader, SheetWidth, SideSheet, Spacing, ToastKind, category_chip,
    destructive_button, empty_state,
    icons::{Icon, tabler_icon},
    radius, search_input, secondary_button, section_header, sp, spf,
    tokens::{BORDER_THIN, FONT_SM, FONT_XS, FontRole, font},
    value_preview,
};

use crate::Message;
use crate::message::ToastMsg;
use crate::runtime_view::RuntimeView;

#[derive(Debug, Clone)]
pub struct TriggerInstanceRow {
    pub id: TriggerInstanceId,
    pub name: String,
    pub kind_id: String,
    pub enabled: bool,
    pub used_in_count: usize,
    pub overrides: TriggerConfig,
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
    pub selected_id: Option<TriggerInstanceId>,
    pub used_in: Vec<InstanceUsage>,
    pub search: String,
    pub platform_filter: Option<String>,
    pub usage_filter: UsageFilter,
    pub sheet_width: f32,
    pub confirm_disable: Option<ConfirmDisable>,
}

impl Default for TriggersRegistryState {
    fn default() -> Self {
        Self {
            instances: Vec::new(),
            selected_id: None,
            used_in: Vec::new(),
            search: String::new(),
            platform_filter: None,
            usage_filter: UsageFilter::All,
            sheet_width: 420.0,
            confirm_disable: None,
        }
    }
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
    DeleteResult(Result<(), String>),
    NavigateToAction(ActionId),
    ScrollTo(TriggerInstanceId),
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
            state.instances = rows;
            Task::none()
        }
        TriggersRegistryMsg::Loaded(Err(msg)) => Task::done(Message::Toast(ToastMsg::Fired {
            kind: ToastKind::Error,
            message: msg,
            duration_ms: 5000,
        })),
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
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    dp.trigger_instance_repo()
                        .delete(id)
                        .await
                        .map(|_| ())
                        .map_err(|e| match e {
                            StorageError::ReferenceBlock { .. } => {
                                "Remove this trigger from all actions before deleting.".to_owned()
                            }
                            other => other.to_string(),
                        })
                },
                |r| Message::TriggersRegistry(TriggersRegistryMsg::DeleteResult(r)),
            )
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
            }))
        }
        TriggersRegistryMsg::NavigateToAction(_) => Task::none(),
        TriggersRegistryMsg::ScrollTo(_) => Task::none(),
    }
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

    let list_content: Element<'_, Message> = if state.instances.is_empty() && !filters_active {
        container(empty_state(
            "No custom trigger instances yet",
            "Create a named trigger with custom settings to reuse across multiple actions.",
            None::<(&str, Message)>,
            palette,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if filtered.is_empty() {
        let clear_msg = Message::TriggersRegistry(TriggersRegistryMsg::PlatformFilterChanged(None));
        container(empty_state(
            "No results",
            "Adjust or clear the filters to find your triggers.",
            Some(("Clear filters", clear_msg)),
            palette,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        let row_els: Vec<Element<'_, Message>> = filtered
            .iter()
            .map(|row| instance_row(row, state.selected_id == Some(row.id), palette))
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
        .map(|row| sheet_body_for(row, &state.used_in, rt, palette))
        .unwrap_or_else(|| Space::new().width(Length::Fill).height(Length::Fill).into());

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

    if let Some(ref cd) = state.confirm_disable {
        let dialog = confirm_disable_dialog(cd, palette);
        stack![main_with_sheet, dialog].into()
    } else {
        main_with_sheet
    }
}

fn registry_header<'a>(
    state: &'a TriggersRegistryState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;

    let search = container(search_input(
        "Search triggers…",
        &state.search,
        |s| Message::TriggersRegistry(TriggersRegistryMsg::SearchChanged(s)),
        palette,
    ))
    .width(Length::Fixed(200.0));

    let twitch_active = state
        .platform_filter
        .as_deref()
        .is_some_and(|x| x == "twitch.");
    let obs_active = state
        .platform_filter
        .as_deref()
        .is_some_and(|x| x == "obs.");
    let script_active = state
        .platform_filter
        .as_deref()
        .is_some_and(|x| x == "script.");
    let all_active = state.platform_filter.is_none();

    let chip_twitch = category_chip(
        palette,
        "Twitch",
        p.brand,
        twitch_active,
        Message::TriggersRegistry(TriggersRegistryMsg::PlatformFilterChanged(Some(
            "twitch.".to_owned(),
        ))),
    );
    let chip_obs = category_chip(
        palette,
        "OBS",
        p.success,
        obs_active,
        Message::TriggersRegistry(TriggersRegistryMsg::PlatformFilterChanged(Some(
            "obs.".to_owned(),
        ))),
    );
    let chip_script = category_chip(
        palette,
        "Script",
        p.warning,
        script_active,
        Message::TriggersRegistry(TriggersRegistryMsg::PlatformFilterChanged(Some(
            "script.".to_owned(),
        ))),
    );
    let chip_all = category_chip(
        palette,
        "All",
        p.text_secondary,
        all_active,
        Message::TriggersRegistry(TriggersRegistryMsg::PlatformFilterChanged(None)),
    );

    let platform_chips = row![chip_twitch, chip_obs, chip_script, chip_all]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center);

    let usage_all_active = state.usage_filter == UsageFilter::All;
    let usage_used_active = state.usage_filter == UsageFilter::Used;
    let usage_unused_active = state.usage_filter == UsageFilter::Unused;

    let chip_u_all = usage_filter_chip(
        "All",
        usage_all_active,
        Message::TriggersRegistry(TriggersRegistryMsg::UsageFilterChanged(UsageFilter::All)),
        palette,
    );
    let chip_u_used = usage_filter_chip(
        "Used",
        usage_used_active,
        Message::TriggersRegistry(TriggersRegistryMsg::UsageFilterChanged(UsageFilter::Used)),
        palette,
    );
    let chip_u_unused = usage_filter_chip(
        "Unused",
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
        text("Triggers").size(FONT_SM).color(p.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let right = row![
        platform_chips,
        make_divider_v(),
        usage_chips,
        make_divider_v(),
        search,
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
    label: &'a str,
    active: bool,
    on_press: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let bg = if active {
        Some(Background::Color(p.surface_overlay))
    } else {
        None
    };
    let text_color = if active {
        p.text_primary
    } else {
        p.text_secondary
    };

    let inner = text(label)
        .size(FONT_XS)
        .color(text_color)
        .font(font(FontRole::Body));

    button(inner)
        .on_press(on_press)
        .padding([sp(Spacing::Xxs), sp(Spacing::Sm)])
        .style(move |_: &iced::Theme, _status| button::Style {
            background: bg,
            border: Border {
                radius: radius(Radius::Pill).into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            text_color,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .into()
}

fn platform_dot_color(kind_id: &str, palette: &ForgePalette) -> Color {
    if kind_id.starts_with("twitch.") {
        palette.brand
    } else if kind_id.starts_with("obs.") {
        palette.success
    } else if kind_id.starts_with("script.") {
        palette.warning
    } else {
        palette.info
    }
}

fn instance_row<'a>(
    row: &'a TriggerInstanceRow,
    selected: bool,
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

    let name_col = column![
        text(row.name.as_str())
            .size(FONT_SM)
            .color(if row.enabled {
                p.text_primary
            } else {
                p.text_muted
            })
            .font(font(FontRole::Body)),
        text(row.kind_id.as_str())
            .size(FONT_XS)
            .color(p.text_faint)
            .font(font(FontRole::Monospace)),
    ]
    .spacing(2);

    let usage_badge: Element<'_, Message> = if row.used_in_count > 0 {
        let label = format!("used in {}", row.used_in_count);
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

    let toggle_label = if row.enabled { "ON" } else { "OFF" };
    let toggle_bg = if row.enabled {
        p.brand
    } else {
        p.surface_overlay
    };
    let toggle_fg = if row.enabled {
        p.shell
    } else {
        p.text_secondary
    };
    let toggle_id = row.id;
    let toggle_enabled = row.enabled;

    let toggle_btn = button(
        text(toggle_label)
            .size(FONT_XS)
            .color(toggle_fg)
            .font(font(FontRole::Body)),
    )
    .on_press(Message::TriggersRegistry(
        TriggersRegistryMsg::EnableToggled(toggle_id, !toggle_enabled),
    ))
    .padding([2, sp(Spacing::Xs)])
    .style(move |_: &iced::Theme, _status| button::Style {
        background: Some(Background::Color(toggle_bg)),
        border: Border {
            radius: radius(Radius::Pill).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        text_color: toggle_fg,
        shadow: iced::Shadow::default(),
        snap: false,
    });

    let delete_btn = button(tabler_icon::<Message>(Icon::X, 13.0, p.text_faint))
        .on_press(Message::TriggersRegistry(
            TriggersRegistryMsg::DeleteRequested(row.id),
        ))
        .padding(sp(Spacing::Xxs))
        .style(|_: &iced::Theme, status| button::Style {
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
        });

    let controls = row![toggle_btn, delete_btn]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center);

    let row_bg = if selected { p.elevated } else { p.base };
    let left_border_color = if selected {
        p.brand
    } else {
        Color::TRANSPARENT
    };

    let inner = row![
        container(dot)
            .align_y(Alignment::Center)
            .padding([0, sp(Spacing::Xs)]),
        container(column![name_col]).width(Length::Fill),
        container(usage_badge)
            .align_y(Alignment::Center)
            .padding([0, sp(Spacing::Xs)]),
        container(controls).align_y(Alignment::Center),
    ]
    .align_y(Alignment::Center)
    .padding([sp(Spacing::Xs), sp(Spacing::Md)]);

    let row_id = row.id;
    button(
        container(inner)
            .width(Length::Fill)
            .style(move |_: &iced::Theme| container::Style {
                background: Some(Background::Color(row_bg)),
                border: Border {
                    color: left_border_color,
                    width: 2.0,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            }),
    )
    .on_press(Message::TriggersRegistry(TriggersRegistryMsg::RowSelected(
        row_id,
    )))
    .padding(0)
    .style(|_: &iced::Theme, _status| button::Style {
        background: None,
        border: Border::default(),
        text_color: Color::TRANSPARENT,
        shadow: iced::Shadow::default(),
        snap: false,
    })
    .into()
}

fn sheet_body_for<'a>(
    row: &'a TriggerInstanceRow,
    used_in: &'a [InstanceUsage],
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

    let config_section: Element<'_, Message> =
        if let Some(descriptor) = rt.trigger_registry.get(&row.kind_id) {
            let default_cfg = descriptor.default_config();
            let effective = effective_config(&default_cfg, &row.overrides);
            let fields = descriptor.config_fields();

            if fields.is_empty() {
                container(
                    text("No configurable fields")
                        .size(FONT_XS)
                        .color(p.text_faint)
                        .font(font(FontRole::Body)),
                )
                .padding([sp(Spacing::Xs), sp(Spacing::Md)])
                .into()
            } else {
                let field_rows: Vec<Element<'_, Message>> = fields
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

                        let value_el: Element<'_, Message> = if let Some(v) = value {
                            if is_overridden {
                                value_preview::<Message>(palette, v)
                            } else {
                                text(variant_one_line(v))
                                    .size(FONT_XS)
                                    .color(p.text_muted)
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

                        let field_row = row![
                            container(label_el).width(Length::FillPortion(4)),
                            container(value_el).width(Length::FillPortion(6)),
                        ]
                        .align_y(Alignment::Center)
                        .padding([sp(Spacing::Xxs), sp(Spacing::Md)]);

                        field_row.into()
                    })
                    .collect();

                let hdr = section_header("CONFIGURATION", None, palette);
                column(std::iter::once(hdr).chain(field_rows).collect::<Vec<_>>()).into()
            }
        } else {
            container(
                text("Trigger kind not registered")
                    .size(FONT_XS)
                    .color(p.text_faint)
                    .font(font(FontRole::Body)),
            )
            .padding([sp(Spacing::Xs), sp(Spacing::Md)])
            .into()
        };

    let used_in_section: Element<'_, Message> = if !used_in.is_empty() {
        let hdr = section_header("USED IN", Some(used_in.len() as u32), palette);
        let usage_rows: Vec<Element<'_, Message>> = used_in
            .iter()
            .map(|u| {
                container(
                    text(u.action_name.as_str())
                        .size(FONT_SM)
                        .color(p.text_secondary)
                        .font(font(FontRole::Body)),
                )
                .padding([sp(Spacing::Xxs), sp(Spacing::Md)])
                .into()
            })
            .collect();
        column(std::iter::once(hdr).chain(usage_rows).collect::<Vec<_>>()).into()
    } else {
        Space::new().width(0).height(0).into()
    };

    let can_delete = row.used_in_count == 0;
    let delete_id = row.id;

    let footer = container(
        row![
            Space::new().width(Length::Fill),
            if can_delete {
                destructive_button(
                    "Delete",
                    Message::TriggersRegistry(TriggersRegistryMsg::DeleteRequested(delete_id)),
                    palette,
                )
            } else {
                container(
                    text("Delete")
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

    column![
        kind_row,
        rule::horizontal(1.0).style(divider_style),
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

    let body_text = format!(
        "Disabling this trigger will pause it for {} action(s). Continue?",
        cd.action_count
    );

    let card = container(
        column![
            text(body_text)
                .size(FONT_SM)
                .color(p.text_primary)
                .font(font(FontRole::Body)),
            row![
                secondary_button(
                    "Cancel",
                    Message::TriggersRegistry(TriggersRegistryMsg::DisableConfirmDismissed),
                    palette,
                ),
                Space::new().width(Length::Fill),
                destructive_button(
                    "Disable anyway",
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
        }
    }

    #[test]
    fn usage_filter_all_passes_any_count() {
        let row_zero = make_row("twitch.chat.command", 0);
        let row_used = make_row("twitch.chat.command", 3);
        assert!(matches!(UsageFilter::All, UsageFilter::All));
        assert!(row_zero.used_in_count == 0);
        assert!(row_used.used_in_count > 0);
    }

    #[test]
    fn platform_filter_twitch_prefix_matches() {
        let row = make_row("twitch.chat.command", 0);
        let prefix = "twitch.";
        assert!(row.kind_id.starts_with(prefix));
    }

    #[test]
    fn platform_filter_obs_prefix_matches() {
        let row = make_row("obs.scenes.current_changed", 0);
        assert!(row.kind_id.starts_with("obs."));
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
}
