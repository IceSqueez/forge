use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use forge_registry::{FormField, KindPlatformContract};
use forge_types::{PlatformId, PlatformScope, TriggerInstance, TriggerInstanceId, Variant};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Task,
    widget::{Space, button, column, container, row, rule, scrollable, stack, text},
};

use forge_widgets::{
    ForgePalette, Radius, Spacing, ToastKind, ToggleProps, category_chip,
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
    pub platform_scope: PlatformScope,
    pub custom_expanded: bool,
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
            platform_scope: PlatformScope::Any,
            custom_expanded: false,
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
    PlatformScopeChanged(PlatformScope),
    PlatformScopeCustomToggled(PlatformId),
    PlatformScopeCustomExpansionToggled,
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
                platform_scope: form.platform_scope.clone(),
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
        CreateInstanceFormMsg::PlatformScopeChanged(scope) => {
            form.platform_scope = scope;
            form.custom_expanded = false;
            Task::none()
        }
        CreateInstanceFormMsg::PlatformScopeCustomToggled(platform_id) => {
            let mut new_set: BTreeSet<PlatformId> = match &form.platform_scope {
                PlatformScope::Any => BTreeSet::new(),
                PlatformScope::Only(set) => set.clone(),
            };
            if new_set.contains(&platform_id) {
                new_set.remove(&platform_id);
            } else {
                new_set.insert(platform_id);
            }
            form.platform_scope = if new_set.is_empty() {
                PlatformScope::Any
            } else {
                PlatformScope::only(new_set).unwrap_or(PlatformScope::Any)
            };
            Task::none()
        }
        CreateInstanceFormMsg::PlatformScopeCustomExpansionToggled => {
            form.custom_expanded = !form.custom_expanded;
            Task::none()
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
            let cat_label = crate::actions_trigger_picker::category_display_label(desc.category());
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
            text(forge_widgets::tr!("triggers_create_no_results"))
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
            text(forge_widgets::tr!("triggers_create_select_kind"))
                .size(FONT_SM)
                .color(p.text_primary)
                .font(font(FontRole::Body)),
            search_input(
                forge_widgets::tr!("triggers_create_search_placeholder"),
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
                forge_widgets::tr!("triggers_create_cancel"),
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
            text(forge_widgets::tr!("triggers_create_back"))
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

    let new_instance_title = forge_widgets::tr!("triggers_create_new_instance", kind = kind_label);
    let header = container(
        column![
            back_btn,
            text(new_instance_title)
                .size(FONT_SM)
                .color(p.text_primary)
                .font(font(FontRole::Body)),
        ]
        .spacing(spf(Spacing::Xxs)),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Sm), sp(Spacing::Md)]);

    let name_input = text_input_field(
        forge_widgets::tr!("triggers_create_name_placeholder"),
        form.name.as_str(),
        |s| {
            Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                CreateInstanceFormMsg::NameChanged(s),
            ))
        },
        palette,
    );

    let name_section = column![
        section_header(
            forge_widgets::tr!("triggers_create_section_name"),
            None,
            palette
        ),
        name_input
    ]
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
            std::iter::once(section_header(
                forge_widgets::tr!("triggers_create_section_config"),
                None,
                palette,
            ))
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

    let platform_section = render_platform_section(form, kind_id, rt, palette);

    let scrollable_body = scrollable(
        column![
            Space::new().height(spf(Spacing::Xs)),
            name_section,
            platform_section,
            Space::new().height(spf(Spacing::Xs)),
            fields_section,
            error_el,
            Space::new().height(spf(Spacing::Md)),
        ]
        .spacing(0),
    )
    .height(Length::Fixed(360.0));

    let can_create = !form.name.is_empty() && !form.saving;

    let create_lbl = forge_widgets::tr!("triggers_create_btn");
    let create_el: Element<'_, Message> = if can_create {
        primary_button(
            create_lbl.clone(),
            Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                CreateInstanceFormMsg::SubmitRequested,
            )),
            palette,
        )
    } else {
        container(
            text(create_lbl)
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
                forge_widgets::tr!("triggers_create_cancel"),
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
                    label: label.to_string(),
                    description: String::new(),
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
                    label: label.to_string(),
                    description: String::new(),
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

fn platform_name_str(p: PlatformId) -> &'static str {
    match p {
        PlatformId::Twitch => "Twitch",
        PlatformId::YouTube => "YouTube",
        PlatformId::Kick => "Kick",
    }
}

fn platform_color(p: PlatformId, palette: &ForgePalette) -> Color {
    match p {
        PlatformId::Twitch => palette.platform_twitch,
        PlatformId::YouTube => palette.platform_youtube,
        PlatformId::Kick => palette.platform_kick,
    }
}

fn scope_display_text(scope: &PlatformScope) -> String {
    match scope {
        PlatformScope::Any => forge_widgets::tr!("triggers_create_scope_any"),
        PlatformScope::Only(set) => {
            let names: Vec<&str> = set.iter().map(|p| platform_name_str(*p)).collect();
            names.join(", ")
        }
    }
}

fn scope_mode_pill<'a>(
    label: impl Into<std::borrow::Cow<'a, str>>,
    active: bool,
    p: ForgePalette,
    on_press: Message,
) -> Element<'a, Message> {
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
    button(
        text(label.into())
            .size(FONT_XS)
            .color(text_color)
            .font(font(FontRole::Body)),
    )
    .on_press(on_press)
    .padding([2, sp(Spacing::Xs)])
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

fn render_platform_section<'a>(
    form: &'a CreateInstanceFormState,
    kind_id: &str,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;

    let Some(descriptor) = rt.trigger_registry.get(kind_id) else {
        return Space::new().width(0).height(0).into();
    };

    match descriptor.platform_contract() {
        KindPlatformContract::Universal => Space::new().width(0).height(0).into(),

        KindPlatformContract::PlatformSpecific(pid) => {
            let dot_color = platform_color(pid, palette);
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
                    text(platform_name_str(pid))
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
            let will_fire_str =
                forge_widgets::tr!("triggers_create_will_fire", scope = platform_name_str(pid));
            let preview = text(will_fire_str)
                .size(FONT_XS)
                .color(p.text_faint)
                .font(font(FontRole::Body));
            column![
                Space::new().height(spf(Spacing::Xs)),
                section_header(
                    forge_widgets::tr!("triggers_create_section_platform"),
                    None,
                    palette
                ),
                container(badge).padding([sp(Spacing::Xxs), sp(Spacing::Md)]),
                container(preview).padding([2, sp(Spacing::Md)]),
            ]
            .into()
        }

        KindPlatformContract::MultiPlatform => {
            let scope = &form.platform_scope;

            let is_single_only = |pid: PlatformId| matches!(scope, PlatformScope::Only(s) if s.len() == 1 && s.contains(&pid));
            let any_active = !form.custom_expanded && matches!(scope, PlatformScope::Any);
            let custom_active =
                form.custom_expanded || matches!(scope, PlatformScope::Only(s) if s.len() > 1);

            let any_pill = scope_mode_pill(
                forge_widgets::tr!("triggers_create_scope_any"),
                any_active,
                p,
                Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                    CreateInstanceFormMsg::PlatformScopeChanged(PlatformScope::Any),
                )),
            );

            let platform_pill_els: Vec<Element<'_, Message>> =
                [PlatformId::Twitch, PlatformId::YouTube, PlatformId::Kick]
                    .iter()
                    .map(|&pid| {
                        let active = !form.custom_expanded && is_single_only(pid);
                        let dot_color = platform_color(pid, palette);
                        let mut single_set = BTreeSet::new();
                        single_set.insert(pid);
                        let new_scope =
                            PlatformScope::only(single_set).unwrap_or(PlatformScope::Any);
                        category_chip(
                            palette,
                            platform_name_str(pid),
                            dot_color,
                            active,
                            Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                                CreateInstanceFormMsg::PlatformScopeChanged(new_scope),
                            )),
                        )
                    })
                    .collect();

            let custom_pill = scope_mode_pill(
                forge_widgets::tr!("triggers_create_scope_custom"),
                custom_active,
                p,
                Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                    CreateInstanceFormMsg::PlatformScopeCustomExpansionToggled,
                )),
            );

            let mut pill_items: Vec<Element<'_, Message>> = vec![any_pill];
            pill_items.extend(platform_pill_els);
            pill_items.push(custom_pill);

            let pill_row = container(
                row(pill_items)
                    .spacing(spf(Spacing::Xxs))
                    .align_y(Alignment::Center),
            )
            .padding([0, sp(Spacing::Md)]);

            let expanded_el: Element<'_, Message> = if form.custom_expanded {
                let checkbox_els: Vec<Element<'_, Message>> =
                    [PlatformId::Twitch, PlatformId::YouTube, PlatformId::Kick]
                        .iter()
                        .map(|&pid| {
                            let checked =
                                matches!(scope, PlatformScope::Only(s) if s.contains(&pid));
                            let dot_color = platform_color(pid, palette);
                            category_chip(
                                palette,
                                platform_name_str(pid),
                                dot_color,
                                checked,
                                Message::TriggersRegistry(TriggersRegistryMsg::CreateFormMsg(
                                    CreateInstanceFormMsg::PlatformScopeCustomToggled(pid),
                                )),
                            )
                        })
                        .collect();
                container(
                    row(checkbox_els)
                        .spacing(spf(Spacing::Xxs))
                        .align_y(Alignment::Center),
                )
                .padding([sp(Spacing::Xxs), sp(Spacing::Md)])
                .into()
            } else {
                Space::new().width(0).height(0).into()
            };

            let will_fire_scope_str = forge_widgets::tr!(
                "triggers_create_will_fire",
                scope = scope_display_text(scope)
            );
            let preview = container(
                text(will_fire_scope_str)
                    .size(FONT_XS)
                    .color(p.text_faint)
                    .font(font(FontRole::Body)),
            )
            .padding([2, sp(Spacing::Md)]);

            column![
                Space::new().height(spf(Spacing::Xs)),
                section_header(
                    forge_widgets::tr!("triggers_create_section_platform"),
                    None,
                    palette
                ),
                pill_row,
                expanded_el,
                preview,
            ]
            .into()
        }
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
            vtube_client: None,
            vtube_sink: forge_vtube::SwitchableVTubeSink::new(),
            discord_client: None,
            midi_client: None,
            hotkey_client: None,
            speak_queue: None,
            sound_player: None,
            twitch_chat_handle: None,
            chat_send_bridge: None,
            twitch_flow: None,
            youtube_flow: None,
            kick_flow: None,
            tts_engine_ids: Vec::new(),
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

    #[test]
    fn platform_scope_changed_replaces_scope() {
        let rt = test_rt();
        let mut set = std::collections::BTreeSet::new();
        set.insert(PlatformId::Twitch);
        let new_scope = PlatformScope::only(set).unwrap();
        let mut state = Some(CreateInstanceFormState::default());
        let _task = update(
            &mut state,
            &rt,
            CreateInstanceFormMsg::PlatformScopeChanged(new_scope.clone()),
        );
        let form = state.as_ref().unwrap();
        assert_eq!(form.platform_scope, new_scope);
        assert!(!form.custom_expanded);
    }

    #[test]
    fn platform_scope_custom_toggled_adds_platform() {
        let rt = test_rt();
        let mut state = Some(CreateInstanceFormState::default());
        let _task = update(
            &mut state,
            &rt,
            CreateInstanceFormMsg::PlatformScopeCustomToggled(PlatformId::YouTube),
        );
        let form = state.as_ref().unwrap();
        let mut expected_set = std::collections::BTreeSet::new();
        expected_set.insert(PlatformId::YouTube);
        assert_eq!(
            form.platform_scope,
            PlatformScope::only(expected_set).unwrap()
        );
    }

    #[test]
    fn platform_scope_custom_toggled_removes_last_platform_reverts_to_any() {
        let rt = test_rt();
        let mut set = std::collections::BTreeSet::new();
        set.insert(PlatformId::Kick);
        let mut state = Some(CreateInstanceFormState {
            platform_scope: PlatformScope::only(set).unwrap(),
            ..Default::default()
        });
        let _task = update(
            &mut state,
            &rt,
            CreateInstanceFormMsg::PlatformScopeCustomToggled(PlatformId::Kick),
        );
        assert_eq!(state.as_ref().unwrap().platform_scope, PlatformScope::Any);
    }

    #[test]
    fn platform_scope_expansion_toggled_flips_flag() {
        let rt = test_rt();
        let mut state = Some(CreateInstanceFormState::default());
        let _task = update(
            &mut state,
            &rt,
            CreateInstanceFormMsg::PlatformScopeCustomExpansionToggled,
        );
        assert!(state.as_ref().unwrap().custom_expanded);
        let _task2 = update(
            &mut state,
            &rt,
            CreateInstanceFormMsg::PlatformScopeCustomExpansionToggled,
        );
        assert!(!state.as_ref().unwrap().custom_expanded);
    }
}
