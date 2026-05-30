use std::collections::BTreeMap;
use std::sync::Arc;

use forge_registry::FormField;
use forge_types::{TriggerInstance, TriggerInstanceId, Variant};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Task,
    widget::{Space, button, column, container, row, rule, scrollable, stack, text},
};

use forge_widgets::{
    ForgePalette, Radius, Spacing, ToastKind, ToggleProps,
    icons::{Icon, tabler_icon},
    primary_button, radius, search_input, secondary_button, section_header, sp, spf,
    text_input_field, toggle,
    tokens::{BORDER_THIN, FONT_SM, FONT_XS, FontRole, font},
};

use crate::Message;
use crate::message::ToastMsg;
use crate::runtime_view::RuntimeView;
use crate::triggers_registry::TriggersRegistryMsg;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateFormStep {
    PickKind,
    FillForm,
}

pub struct CreateInstanceFormState {
    pub step: CreateFormStep,
    pub selected_kind_id: Option<String>,
    pub name: String,
    pub overrides_buffer: BTreeMap<String, Variant>,
    pub text_buffer: BTreeMap<String, String>,
    pub search: String,
    pub saving: bool,
    pub validation_error: Option<String>,
}

impl Default for CreateInstanceFormState {
    fn default() -> Self {
        Self {
            step: CreateFormStep::PickKind,
            selected_kind_id: None,
            name: String::new(),
            overrides_buffer: BTreeMap::new(),
            text_buffer: BTreeMap::new(),
            search: String::new(),
            saving: false,
            validation_error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CreateInstanceFormMsg {
    KindSelected(String),
    BackToKindPicker,
    NameChanged(String),
    SearchChanged(String),
    FieldChanged(String, Variant),
    IntInputChanged(String, String),
    FieldCleared(String),
    SubmitRequested,
    SubmitResult(Result<TriggerInstanceId, String>),
    Cancelled,
}

pub fn update(
    state: &mut Option<CreateInstanceFormState>,
    rt: &RuntimeView,
    msg: CreateInstanceFormMsg,
) -> Task<Message> {
    if let CreateInstanceFormMsg::Cancelled = &msg {
        *state = None;
        return Task::none();
    }
    if let CreateInstanceFormMsg::SubmitResult(Ok(id)) = &msg {
        let id = *id;
        *state = None;
        return Task::batch([
            Task::done(Message::TriggersRegistry(
                TriggersRegistryMsg::LoadRequested,
            )),
            Task::done(Message::TriggersRegistry(TriggersRegistryMsg::RowSelected(
                id,
            ))),
        ]);
    }

    let form = match state {
        Some(f) => f,
        None => return Task::none(),
    };

    match msg {
        CreateInstanceFormMsg::Cancelled | CreateInstanceFormMsg::SubmitResult(Ok(_)) => {
            unreachable!()
        }
        CreateInstanceFormMsg::KindSelected(kind_id) => {
            let default_config = rt
                .trigger_registry
                .get(&kind_id)
                .map(|d| d.default_config())
                .unwrap_or_default();
            let mut text_buf: BTreeMap<String, String> = BTreeMap::new();
            for (k, v) in &default_config {
                text_buf.insert(k.clone(), variant_to_display_str(v));
            }
            form.selected_kind_id = Some(kind_id);
            form.overrides_buffer = default_config;
            form.text_buffer = text_buf;
            form.step = CreateFormStep::FillForm;
            form.validation_error = None;
            Task::none()
        }
        CreateInstanceFormMsg::BackToKindPicker => {
            form.step = CreateFormStep::PickKind;
            form.selected_kind_id = None;
            form.overrides_buffer = BTreeMap::new();
            form.text_buffer = BTreeMap::new();
            form.validation_error = None;
            Task::none()
        }
        CreateInstanceFormMsg::NameChanged(s) => {
            form.name = s;
            Task::none()
        }
        CreateInstanceFormMsg::SearchChanged(s) => {
            form.search = s;
            Task::none()
        }
        CreateInstanceFormMsg::FieldChanged(key, variant) => {
            form.text_buffer
                .insert(key.clone(), variant_to_display_str(&variant));
            form.overrides_buffer.insert(key, variant);
            Task::none()
        }
        CreateInstanceFormMsg::IntInputChanged(key, raw) => {
            form.text_buffer.insert(key.clone(), raw.clone());
            if let Ok(n) = raw.parse::<i64>() {
                form.overrides_buffer.insert(key, Variant::Int(n));
            }
            Task::none()
        }
        CreateInstanceFormMsg::FieldCleared(key) => {
            form.overrides_buffer.remove(&key);
            form.text_buffer.remove(&key);
            Task::none()
        }
        CreateInstanceFormMsg::SubmitRequested => {
            if form.name.is_empty() {
                return Task::none();
            }
            let kind_id = match form.selected_kind_id.clone() {
                Some(k) => k,
                None => return Task::none(),
            };
            form.saving = true;
            form.validation_error = None;
            let instance = TriggerInstance {
                id: TriggerInstanceId::new(),
                kind_id,
                name: form.name.clone(),
                overrides: form.overrides_buffer.clone(),
                enabled: true,
                user_defined: true,
                platform_scope: Default::default(),
            };
            let dp = Arc::clone(&rt.backend);
            Task::perform(
                async move {
                    let id = instance.id;
                    dp.trigger_instance_repo()
                        .save(&instance)
                        .await
                        .map(|()| id)
                        .map_err(|e| e.to_string())
                },
                |r| {
                    Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                        CreateInstanceFormMsg::SubmitResult(r),
                    ))
                },
            )
        }
        CreateInstanceFormMsg::SubmitResult(Err(err)) => {
            form.saving = false;
            form.validation_error = Some(err.clone());
            Task::done(Message::Toast(ToastMsg::Fired {
                kind: ToastKind::Error,
                message: err,
                duration_ms: 5000,
            }))
        }
    }
}

pub fn view<'a>(
    form: &'a CreateInstanceFormState,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;

    let backdrop = button(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(Message::TriggersRegistry(
            TriggersRegistryMsg::CreateFormMsg(CreateInstanceFormMsg::Cancelled),
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

    let inner: Element<'_, Message> = match form.step {
        CreateFormStep::PickKind => kind_picker_view(form, rt, palette),
        CreateFormStep::FillForm => form_view(form, rt, palette),
    };

    let card = container(inner)
        .max_width(560)
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

fn kind_picker_view<'a>(
    form: &'a CreateInstanceFormState,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let search_lower = form.search.to_lowercase();

    let mut groups: BTreeMap<String, Vec<&'a dyn forge_registry::TriggerKindDescriptor>> =
        BTreeMap::new();
    for desc in rt.trigger_registry.all() {
        let matches = search_lower.is_empty()
            || desc.label().to_lowercase().contains(&search_lower)
            || desc.id().to_lowercase().contains(&search_lower);
        if matches {
            let cat_label =
                crate::actions_trigger_picker::category_display_label(desc.category()).to_owned();
            groups.entry(cat_label).or_default().push(desc);
        }
    }

    let mut list_rows: Vec<Element<'a, Message>> = Vec::new();
    for (cat_label, descs) in &groups {
        list_rows.push(section_header(
            cat_label.clone(),
            Some(descs.len() as u32),
            palette,
        ));
        for &desc in descs {
            list_rows.push(kind_row_entry(desc, palette));
        }
    }

    let list_el: Element<'_, Message> = if list_rows.is_empty() {
        container(
            text("No matching trigger kinds")
                .size(FONT_SM)
                .color(p.text_muted)
                .font(font(FontRole::Body)),
        )
        .padding([sp(Spacing::Md), sp(Spacing::Md)])
        .into()
    } else {
        scrollable(column(list_rows))
            .height(Length::Fixed(380.0))
            .into()
    };

    let divider_style = move |_: &iced::Theme| rule::Style {
        color: p.border_regular,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    };

    let header = container(
        column![
            text("Select trigger kind")
                .size(FONT_SM)
                .color(p.text_primary)
                .font(font(FontRole::Body)),
            search_input(
                "Search kinds…",
                &form.search,
                |s| {
                    Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                        CreateInstanceFormMsg::SearchChanged(s),
                    ))
                },
                palette,
            ),
        ]
        .spacing(spf(Spacing::Xs)),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Md), sp(Spacing::Md)]);

    let footer = container(
        row![
            Space::new().width(Length::Fill),
            secondary_button(
                "Cancel",
                Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                    CreateInstanceFormMsg::Cancelled,
                )),
                palette,
            ),
        ]
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
        header,
        rule::horizontal(1.0).style(divider_style),
        list_el,
        rule::horizontal(1.0).style(divider_style),
        footer,
    ]
    .width(Length::Fill)
    .into()
}

fn kind_row_entry<'a>(
    desc: &'a dyn forge_registry::TriggerKindDescriptor,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let dot_color = kind_dot_color(desc.id(), palette);
    let dot_size = 7.0_f32;

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

    let label_col = column![
        text(desc.label())
            .size(FONT_SM)
            .color(p.text_primary)
            .font(font(FontRole::Body)),
        text(desc.id())
            .size(FONT_XS)
            .color(p.text_faint)
            .font(font(FontRole::Monospace)),
    ]
    .spacing(2);

    let kind_id = desc.id().to_owned();

    let row_inner = row![
        container(dot)
            .align_y(Alignment::Center)
            .padding([0, sp(Spacing::Xs)]),
        container(label_col).width(Length::Fill),
        tabler_icon::<Message>(Icon::ChevronRight, 13.0, p.text_faint),
    ]
    .align_y(Alignment::Center)
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)]);

    button(row_inner)
        .on_press(Message::TriggersRegistry(
            TriggersRegistryMsg::CreateFormMsg(CreateInstanceFormMsg::KindSelected(kind_id)),
        ))
        .padding(0)
        .width(Length::Fill)
        .style(|_: &iced::Theme, status| button::Style {
            background: match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Some(Background::Color(Color {
                        a: 0.06,
                        ..Color::WHITE
                    }))
                }
                _ => None,
            },
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .into()
}

fn form_view<'a>(
    form: &'a CreateInstanceFormState,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;

    let kind_id = match &form.selected_kind_id {
        Some(k) => k.as_str(),
        None => "",
    };
    let kind_label = rt
        .trigger_registry
        .get(kind_id)
        .map(|d| d.label())
        .unwrap_or(kind_id);

    let back_btn = button(
        row![
            tabler_icon::<Message>(Icon::ArrowBackUp, 14.0, p.text_secondary),
            text("Back")
                .size(FONT_SM)
                .color(p.text_secondary)
                .font(font(FontRole::Body)),
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center),
    )
    .on_press(Message::TriggersRegistry(
        TriggersRegistryMsg::CreateFormMsg(CreateInstanceFormMsg::BackToKindPicker),
    ))
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
    .style(|_: &iced::Theme, _status| button::Style {
        background: None,
        border: Border::default(),
        text_color: Color::TRANSPARENT,
        shadow: iced::Shadow::default(),
        snap: false,
    });

    let header = container(
        column![
            back_btn,
            text(format!("New {kind_label} instance"))
                .size(FONT_SM)
                .color(p.text_primary)
                .font(font(FontRole::Body)),
        ]
        .spacing(spf(Spacing::Xxs)),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Sm), sp(Spacing::Md)]);

    let name_input = text_input_field(
        "Instance name (required)",
        form.name.as_str(),
        |s| {
            Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                CreateInstanceFormMsg::NameChanged(s),
            ))
        },
        palette,
    );

    let name_section = column![section_header("NAME", None, palette), name_input]
        .spacing(spf(Spacing::Xs))
        .padding([0, sp(Spacing::Md)]);

    let config_fields: Vec<FormField> = rt
        .trigger_registry
        .get(kind_id)
        .map(|d| d.config_fields())
        .unwrap_or_default();

    let field_rows: Vec<Element<'_, Message>> = config_fields
        .iter()
        .map(|field| render_field(field, form, palette))
        .collect();

    let fields_section: Element<'_, Message> = if field_rows.is_empty() {
        Space::new().width(0).height(0).into()
    } else {
        column(
            std::iter::once(section_header("CONFIGURATION", None, palette))
                .chain(field_rows)
                .collect::<Vec<_>>(),
        )
        .padding([0, sp(Spacing::Md)])
        .into()
    };

    let error_el: Element<'_, Message> = if let Some(ref err) = form.validation_error {
        container(
            text(err.as_str())
                .size(FONT_XS)
                .color(p.random)
                .font(font(FontRole::Body)),
        )
        .padding([0, sp(Spacing::Md)])
        .into()
    } else {
        Space::new().width(0).height(0).into()
    };

    let scrollable_body = scrollable(
        column![
            Space::new().height(spf(Spacing::Xs)),
            name_section,
            Space::new().height(spf(Spacing::Xs)),
            fields_section,
            error_el,
            Space::new().height(spf(Spacing::Md)),
        ]
        .spacing(0),
    )
    .height(Length::Fixed(360.0));

    let can_create = !form.name.is_empty() && !form.saving;

    let create_el: Element<'_, Message> = if can_create {
        primary_button(
            "Create",
            Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                CreateInstanceFormMsg::SubmitRequested,
            )),
            palette,
        )
    } else {
        container(
            text("Create")
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

    let divider_style = move |_: &iced::Theme| rule::Style {
        color: p.border_regular,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    };

    let footer = container(
        row![
            secondary_button(
                "Cancel",
                Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                    CreateInstanceFormMsg::Cancelled,
                )),
                palette,
            ),
            Space::new().width(Length::Fill),
            create_el,
        ]
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
        header,
        rule::horizontal(1.0).style(divider_style),
        scrollable_body,
        rule::horizontal(1.0).style(divider_style),
        footer,
    ]
    .width(Length::Fill)
    .into()
}

fn render_field<'a>(
    field: &FormField,
    form: &'a CreateInstanceFormState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let field_row_padding = [sp(Spacing::Xxs), 0u16];

    match field {
        FormField::Text {
            key,
            label,
            placeholder,
        } => {
            let display = form.text_buffer.get(*key).map(|s| s.as_str()).unwrap_or("");
            let k = key.to_string();
            column![
                text(*label)
                    .size(FONT_XS)
                    .color(p.text_secondary)
                    .font(font(FontRole::Body)),
                text_input_field(
                    *placeholder,
                    display,
                    move |v| {
                        Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                            CreateInstanceFormMsg::FieldChanged(k.clone(), Variant::String(v)),
                        ))
                    },
                    palette
                ),
            ]
            .spacing(spf(Spacing::Xxs))
            .padding(field_row_padding)
            .into()
        }
        FormField::TextArea { key, label } => {
            let display = form.text_buffer.get(*key).map(|s| s.as_str()).unwrap_or("");
            let k = key.to_string();
            column![
                text(*label)
                    .size(FONT_XS)
                    .color(p.text_secondary)
                    .font(font(FontRole::Body)),
                text_input_field(
                    "",
                    display,
                    move |v| {
                        Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                            CreateInstanceFormMsg::FieldChanged(k.clone(), Variant::String(v)),
                        ))
                    },
                    palette
                ),
            ]
            .spacing(spf(Spacing::Xxs))
            .padding(field_row_padding)
            .into()
        }
        FormField::Integer { key, label, .. } => {
            let display = form.text_buffer.get(*key).map(|s| s.as_str()).unwrap_or("");
            let k = key.to_string();
            column![
                text(*label)
                    .size(FONT_XS)
                    .color(p.text_secondary)
                    .font(font(FontRole::Body)),
                text_input_field(
                    "0",
                    display,
                    move |v| {
                        Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                            CreateInstanceFormMsg::IntInputChanged(k.clone(), v),
                        ))
                    },
                    palette
                ),
            ]
            .spacing(spf(Spacing::Xxs))
            .padding(field_row_padding)
            .into()
        }
        FormField::Toggle { key, label } => {
            let checked = matches!(form.overrides_buffer.get(*key), Some(Variant::Bool(true)));
            let k = key.to_string();
            container(toggle(
                palette,
                ToggleProps {
                    label,
                    description: "",
                    value: checked,
                    on_toggle: Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                        CreateInstanceFormMsg::FieldChanged(k, Variant::Bool(!checked)),
                    )),
                },
            ))
            .padding(field_row_padding)
            .into()
        }
        FormField::Select {
            key,
            label,
            options,
        } => {
            let opts: Vec<String> = options.iter().map(|s| s.to_string()).collect();
            let current: Option<String> = form.overrides_buffer.get(*key).and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            });
            let k = key.to_string();
            let p_sel = p;
            let picker = iced::widget::pick_list(opts, current, move |s: String| {
                Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                    CreateInstanceFormMsg::FieldChanged(k.clone(), Variant::String(s)),
                ))
            })
            .padding(forge_widgets::input_padding())
            .width(Length::Fill)
            .style(move |_theme, status| {
                use iced::widget::pick_list;
                let border_color = match status {
                    pick_list::Status::Opened { .. } => p_sel.border_active,
                    _ => p_sel.border_input,
                };
                pick_list::Style {
                    text_color: p_sel.text_primary,
                    placeholder_color: p_sel.text_muted,
                    handle_color: p_sel.text_muted,
                    background: Background::Color(p_sel.shell),
                    border: Border {
                        color: border_color,
                        width: BORDER_THIN,
                        radius: radius(Radius::Md).into(),
                    },
                }
            });
            column![
                text(*label)
                    .size(FONT_XS)
                    .color(p.text_secondary)
                    .font(font(FontRole::Body)),
                picker,
            ]
            .spacing(spf(Spacing::Xxs))
            .padding(field_row_padding)
            .into()
        }
        FormField::DynamicSelect {
            key,
            label,
            options_key,
        } => {
            let display = form.text_buffer.get(*key).map(|s| s.as_str()).unwrap_or("");
            let k = key.to_string();
            let placeholder = format!("Enter value  ({options_key})");
            column![
                text(*label)
                    .size(FONT_XS)
                    .color(p.text_secondary)
                    .font(font(FontRole::Body)),
                text_input_field(
                    placeholder,
                    display,
                    move |v| {
                        Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                            CreateInstanceFormMsg::FieldChanged(k.clone(), Variant::String(v)),
                        ))
                    },
                    palette
                ),
            ]
            .spacing(spf(Spacing::Xxs))
            .padding(field_row_padding)
            .into()
        }
        FormField::Optional { key, label, inner } => {
            let is_enabled = matches!(form.overrides_buffer.get(*key), Some(Variant::Bool(true)));
            let k = key.to_string();
            let toggle_el = toggle(
                palette,
                ToggleProps {
                    label,
                    description: "",
                    value: is_enabled,
                    on_toggle: Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                        CreateInstanceFormMsg::FieldChanged(k, Variant::Bool(!is_enabled)),
                    )),
                },
            );
            if is_enabled {
                column![toggle_el, render_field(inner, form, palette)]
                    .spacing(spf(Spacing::Xs))
                    .padding(field_row_padding)
                    .into()
            } else {
                container(toggle_el).padding(field_row_padding).into()
            }
        }
    }
}

fn kind_dot_color(kind_id: &str, palette: &ForgePalette) -> Color {
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

fn variant_to_display_str(v: &Variant) -> String {
    match v {
        Variant::Int(n) => n.to_string(),
        Variant::Float(f) => f.to_string(),
        Variant::Bool(b) => b.to_string(),
        Variant::String(s) => s.clone(),
        Variant::Datetime(dt) => dt.to_string(),
        Variant::Array(_) | Variant::Object(_) => String::new(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use forge_runtime::{EventBus, NullEventLogRepo, ScriptRegistry};
    use forge_storage::CredentialsRepo;
    use forge_storage_sqlite::SqliteBackend;

    use crate::runtime_view::RuntimeView;
    use crate::server_subsystem::ServerSubsystem;

    fn test_rt() -> RuntimeView {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let backend = Arc::new(
            rt.block_on(SqliteBackend::open_with_key("sqlite::memory:", [0xab; 32]))
                .expect("in-memory SQLite"),
        );
        let server_subsystem = Arc::new(ServerSubsystem::new(
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>
        ));
        let backend: Arc<dyn forge_storage::DataProvider> = backend;
        RuntimeView {
            actions: Arc::new(forge_runtime::actions::ActionsService::new(
                backend.action_repo(),
                backend.queue_repo(),
                backend.history_repo(),
                backend.trigger_instance_repo(),
                backend.soundboard_clips_repo(),
            )),
            backend,
            bus: EventBus::new(Arc::new(NullEventLogRepo)),
            script_registry: Arc::new(ScriptRegistry::new()),
            server_subsystem,
            action_engine: None,
            scheduler: None,
            obs_client: None,
            speak_queue: None,
            sound_player: None,
            twitch_chat_handle: None,
            chat_send_bridge: None,
            twitch_flow: None,
            twitch_login: None,
            twitch_token_expires: None,
            twitch_reauth_required: false,
            sub_action_registry: Arc::new(forge_registry::SubActionRegistry::new()),
            trigger_registry: Arc::new(forge_registry::TriggerRegistry::new()),
        }
    }

    #[test]
    fn submit_with_empty_name_does_not_call_save() {
        let rt = test_rt();
        let mut state = Some(CreateInstanceFormState {
            step: CreateFormStep::FillForm,
            selected_kind_id: Some("twitch.chat.message".to_owned()),
            name: String::new(),
            ..Default::default()
        });
        let _task = update(&mut state, &rt, CreateInstanceFormMsg::SubmitRequested);
        assert!(!state.as_ref().unwrap().saving);
    }

    #[test]
    fn kind_selected_transitions_to_form_step() {
        let rt = test_rt();
        let mut state = Some(CreateInstanceFormState::default());
        let _task = update(
            &mut state,
            &rt,
            CreateInstanceFormMsg::KindSelected("twitch.chat.message".to_owned()),
        );
        let form = state.as_ref().unwrap();
        assert_eq!(form.step, CreateFormStep::FillForm);
        assert_eq!(
            form.selected_kind_id,
            Some("twitch.chat.message".to_owned())
        );
    }

    #[test]
    fn field_changed_updates_buffer() {
        let rt = test_rt();
        let mut state = Some(CreateInstanceFormState {
            step: CreateFormStep::FillForm,
            selected_kind_id: Some("twitch.chat.message".to_owned()),
            ..Default::default()
        });
        let _task = update(
            &mut state,
            &rt,
            CreateInstanceFormMsg::FieldChanged("min_bits".to_owned(), Variant::Int(500)),
        );
        let form = state.as_ref().unwrap();
        assert_eq!(
            form.overrides_buffer.get("min_bits"),
            Some(&Variant::Int(500))
        );
    }

    #[test]
    fn cancelled_clears_state() {
        let rt = test_rt();
        let mut state = Some(CreateInstanceFormState::default());
        let _task = update(&mut state, &rt, CreateInstanceFormMsg::Cancelled);
        assert!(state.is_none());
    }
}
