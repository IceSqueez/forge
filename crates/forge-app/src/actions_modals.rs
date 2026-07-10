use std::collections::BTreeMap;

use forge_registry::SubActionCategory;
use forge_widgets::ForgePalette;
use forge_widgets::tokens::{FONT_SM, FONT_XS, Spacing, spf};
use iced::{Element, Length};

use crate::actions::{AddActionForm, AddActionMsg, AddSubActionForm, AddSubActionMsg};
use crate::actions_field_form::{DynamicOptions, FieldBuffers, FieldEditMsg, render_field};
use crate::message::{ActionEditorMsg, ActionsMsg, Message};
use crate::runtime_view::RuntimeView;

pub(crate) fn add_action_modal_view<'a>(
    form: &'a AddActionForm,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::icons::Icon;
    use forge_widgets::{BannerKind, ModalProps, ToggleAccent, ToggleProps};
    use iced::widget::{column, row, text};

    let name_count = format!("{}/64", form.name.len().min(64));
    let name_counter = text(name_count)
        .size(FONT_XS)
        .color(palette.text_faint)
        .font(forge_widgets::font(forge_widgets::FontRole::Monospace));

    let name_input = forge_widgets::text_input_field(
        forge_widgets::tr!("actions_name_placeholder"),
        &form.name,
        |v| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::NameChanged(v),
            )))
        },
        palette,
    );

    let name_row = row![name_input, name_counter]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::alignment::Vertical::Center);

    let name_block = column![
        forge_widgets::section_header(
            forge_widgets::tr!("actions_modal_section_name"),
            None,
            palette
        ),
        name_row,
    ]
    .spacing(spf(Spacing::Xs));

    // GROUP renders as the design's colored-dot select box: a leading brand dot
    // inside the input frame. Kept as a free-text field (not a closed dropdown) so
    // authors can still name a brand-new group here; the dot supplies the visual.
    let gp = *palette;
    let group_ph = forge_widgets::tr!("actions_group_placeholder");
    let group_dot = iced::widget::container(iced::widget::Space::new().width(8).height(8))
        .width(8)
        .height(8)
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(gp.brand)),
            border: iced::Border {
                radius: forge_widgets::radius(forge_widgets::Radius::Sm).into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        });
    let group_text_input = iced::widget::text_input(group_ph.as_str(), &form.group)
        .on_input(|v| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::GroupChanged(v),
            )))
        })
        .size(FONT_SM)
        .padding(0)
        .width(Length::Fill)
        .style(move |_theme, _status| iced::widget::text_input::Style {
            background: iced::Background::Color(iced::Color::TRANSPARENT),
            border: iced::Border {
                color: iced::Color::TRANSPARENT,
                width: 0.0,
                radius: 0.0.into(),
            },
            icon: gp.text_muted,
            placeholder: gp.text_muted,
            value: gp.text_primary,
            selection: iced::Color {
                a: 0.25,
                ..gp.brand
            },
        });
    let group_input = iced::widget::container(
        row![group_dot, group_text_input]
            .spacing(spf(Spacing::Xs))
            .align_y(iced::alignment::Vertical::Center),
    )
    .padding(forge_widgets::inputs::input_padding())
    .width(Length::Fill)
    .style(move |_theme: &iced::Theme| iced::widget::container::Style {
        background: Some(iced::Background::Color(gp.shell)),
        border: iced::Border {
            color: gp.border_regular,
            width: forge_widgets::tokens::BORDER_THIN,
            radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
        },
        ..iced::widget::container::Style::default()
    });

    let group_block = column![
        forge_widgets::section_header(
            forge_widgets::tr!("actions_modal_section_group"),
            None,
            palette
        ),
        group_input,
    ]
    .spacing(spf(Spacing::Xs));

    let queue_names: Vec<String> = form.queue_options.iter().map(|(_, n)| n.clone()).collect();
    let p = *palette;
    let queue_select: Element<'_, Message> = iced::widget::pick_list(
        queue_names,
        form.selected_queue_name.clone(),
        |name: String| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::QueueSelected(name),
            )))
        },
    )
    .padding(forge_widgets::inputs::input_padding())
    .width(Length::Fill)
    .style(move |_theme, status| {
        use iced::widget::pick_list;
        let border_color = match status {
            pick_list::Status::Opened { .. } => p.border_active,
            _ => p.border_regular,
        };
        pick_list::Style {
            text_color: p.text_primary,
            placeholder_color: p.text_muted,
            handle_color: p.text_muted,
            background: iced::Background::Color(p.shell),
            border: iced::Border {
                color: border_color,
                width: 0.5,
                radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
            },
        }
    })
    .into();

    let queue_block = column![
        forge_widgets::section_header(
            forge_widgets::tr!("actions_modal_section_queue"),
            None,
            palette
        ),
        queue_select,
    ]
    .spacing(spf(Spacing::Xs));

    let two_col = row![group_block, queue_block].spacing(spf(Spacing::Sm));

    let desc_input = forge_widgets::text_area_field(
        forge_widgets::tr!("actions_description_placeholder"),
        &form.description,
        |a| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::DescriptionAction(a),
            )))
        },
        palette,
    );

    let desc_block = column![
        forge_widgets::section_header(
            forge_widgets::tr!("actions_modal_section_description"),
            None,
            palette
        ),
        desc_input,
    ]
    .spacing(spf(Spacing::Xs));

    let enabled_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: forge_widgets::tr!("actions_modal_enabled_label"),
            description: forge_widgets::tr!("actions_modal_enabled_desc"),
            value: form.enabled,
            on_toggle: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::EnabledToggled(!form.enabled),
            ))),
            accent: Some(ToggleAccent::new(Icon::CircleCheck, palette.success)),
        },
    );

    let concurrent_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: forge_widgets::tr!("actions_modal_concurrent_label"),
            description: forge_widgets::tr!("actions_modal_concurrent_desc"),
            value: form.concurrent,
            on_toggle: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::ConcurrentToggled(!form.concurrent),
            ))),
            accent: Some(ToggleAccent::new(Icon::Copy, palette.info)),
        },
    );

    let bypass_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: forge_widgets::tr!("actions_modal_bypass_label"),
            description: forge_widgets::tr!("actions_modal_bypass_desc"),
            value: form.bypass_pause,
            on_toggle: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::BypassPauseToggled(!form.bypass_pause),
            ))),
            accent: Some(ToggleAccent::new(Icon::PlayerPlay, palette.warning)),
        },
    );

    let random_pick_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: forge_widgets::tr!("actions_modal_random_pick_label"),
            description: forge_widgets::tr!("actions_modal_random_pick_desc"),
            value: form.random_pick,
            on_toggle: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::RandomPickToggled(!form.random_pick),
            ))),
            accent: Some(ToggleAccent::new(Icon::Repeat, palette.random)),
        },
    );

    let behavior_header = forge_widgets::section_header(
        forge_widgets::tr!("actions_modal_section_behavior"),
        None,
        palette,
    );

    let mut body_col = column![
        name_block,
        two_col,
        desc_block,
        behavior_header,
        enabled_toggle,
        concurrent_toggle,
        bypass_toggle,
        random_pick_toggle,
    ]
    .spacing(spf(Spacing::Sm));

    if let Some(err) = form.error.as_deref() {
        body_col = body_col.push(forge_widgets::live_status_banner(
            BannerKind::Error,
            err,
            None,
            palette,
        ));
    }

    let cancel_btn = forge_widgets::secondary_button(
        forge_widgets::tr!("actions_modal_cancel_btn"),
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
            AddActionMsg::Cancel,
        ))),
        palette,
    );

    let create_on_press = Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
        AddActionMsg::Submit,
    )));
    let create_btn = if form.is_valid() && !form.saving {
        forge_widgets::primary_button(
            forge_widgets::tr!("actions_modal_create_btn"),
            create_on_press,
            palette,
        )
    } else {
        forge_widgets::secondary_button(
            forge_widgets::tr!("actions_modal_create_btn"),
            Message::Noop,
            palette,
        )
    };

    let footer_buttons = row![cancel_btn, create_btn].spacing(spf(Spacing::Xs));

    let footer: Element<'_, Message> = iced::widget::container(
        row![
            text(forge_widgets::tr!("actions_esc_hint"))
                .size(FONT_XS)
                .color(palette.text_faint)
                .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
            iced::widget::Space::new().width(Length::Fill),
            footer_buttons,
        ]
        .align_y(iced::alignment::Vertical::Center),
    )
    .width(Length::Fill)
    .into();

    forge_widgets::modal(
        palette,
        ModalProps {
            title: std::borrow::Cow::Owned(forge_widgets::tr!("actions_modal_new_action_title")),
            subtitle: None,
            icon: None,
            icon_tint: None,
            size: forge_widgets::ModalSize::Md,
            on_close: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::Cancel,
            ))),
            kbd_hint: None,
            on_submit: (form.is_valid() && !form.saving).then(|| {
                Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                    AddActionMsg::Submit,
                )))
            }),
        },
        body_col.into(),
        footer,
    )
}

fn sub_category_label(cat: SubActionCategory) -> String {
    use forge_widgets::tr;
    match cat {
        SubActionCategory::Chat => tr!("trigger_cat_chat"),
        SubActionCategory::Moderation => tr!("trigger_cat_moderation"),
        SubActionCategory::ChannelPoints => tr!("trigger_cat_channel_points"),
        SubActionCategory::PollsPredictions => tr!("sub_cat_polls_predictions"),
        SubActionCategory::Globals => tr!("sub_cat_globals"),
        SubActionCategory::Logic => tr!("sub_cat_logic"),
        SubActionCategory::Delay => tr!("sub_cat_delay"),
        SubActionCategory::Scripts => tr!("sub_cat_scripts"),
        SubActionCategory::Files => tr!("sub_cat_files"),
        SubActionCategory::Twitch => "Twitch".to_owned(),
        SubActionCategory::YouTube => "YouTube".to_owned(),
        SubActionCategory::Kick => "Kick".to_owned(),
        SubActionCategory::Obs => tr!("trigger_cat_obs"),
        SubActionCategory::VTube => "VTube Studio".to_owned(),
        SubActionCategory::Discord => "Discord".to_owned(),
        SubActionCategory::Midi => "MIDI".to_owned(),
        SubActionCategory::Hotkey => tr!("trigger_cat_hotkey"),
        SubActionCategory::Audio => tr!("sub_cat_audio"),
        SubActionCategory::Tts => tr!("sub_cat_tts"),
        SubActionCategory::Http => tr!("sub_cat_http"),
        SubActionCategory::Server => "Server".to_owned(),
        SubActionCategory::Util => tr!("sub_cat_util"),
    }
}

fn sub_category_color(cat: SubActionCategory, palette: &ForgePalette) -> iced::Color {
    match cat {
        SubActionCategory::Chat | SubActionCategory::Twitch => palette.brand,
        SubActionCategory::YouTube => palette.platform_youtube,
        SubActionCategory::Kick => palette.platform_kick,
        SubActionCategory::Globals | SubActionCategory::ChannelPoints => palette.warning,
        SubActionCategory::Scripts => palette.warning,
        SubActionCategory::Files | SubActionCategory::Http => palette.random,
        SubActionCategory::Audio | SubActionCategory::Tts => palette.success,
        SubActionCategory::Obs
        | SubActionCategory::VTube
        | SubActionCategory::Discord
        | SubActionCategory::Midi
        | SubActionCategory::Hotkey => palette.info,
        _ => palette.text_muted,
    }
}

fn dynamic_options_for<'a>(form: &'a AddSubActionForm, _rt: &'a RuntimeView) -> DynamicOptions<'a> {
    let mut map: DynamicOptions<'a> = BTreeMap::new();

    let clips: Vec<(String, String)> = form
        .available_clips
        .iter()
        .map(|(id, name)| (id.to_string(), name.clone()))
        .collect();
    if !clips.is_empty() {
        map.insert("soundboard.clip_ids", clips);
    }

    let actions: Vec<(String, String)> = form
        .available_actions
        .iter()
        .map(|(id, name)| (id.to_string(), name.clone()))
        .collect();
    if !actions.is_empty() {
        map.insert("action.ids", actions);
    }

    let queues: Vec<(String, String)> = form
        .available_queues
        .iter()
        .map(|(id, name)| (id.to_string(), name.clone()))
        .collect();
    if !queues.is_empty() {
        map.insert("queue.ids", queues);
    }

    let trigger_instances: Vec<(String, String)> = form
        .available_trigger_instances
        .iter()
        .map(|(id, name)| (id.to_string(), name.clone()))
        .collect();
    if !trigger_instances.is_empty() {
        map.insert("trigger_instance.ids", trigger_instances);
    }

    let scripts: Vec<(String, String)> = form
        .available_scripts
        .iter()
        .map(|name| (name.clone(), name.clone()))
        .collect();
    if !scripts.is_empty() {
        map.insert("script.names", scripts);
    }

    map
}

struct SubKindEntry {
    kind_id: String,
    label: String,
    summary: String,
}

struct SubKindGroup {
    category: SubActionCategory,
    entries: Vec<SubKindEntry>,
}

fn sub_kind_picker_body<'a>(
    form: &'a AddSubActionForm,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{column, container, scrollable, text};

    let p = *palette;
    let search_lower = form.search.to_lowercase();

    let mut groups: BTreeMap<String, SubKindGroup> = BTreeMap::new();
    for runner in rt.sub_action_registry.all() {
        let matches = search_lower.is_empty()
            || runner.label().to_lowercase().contains(&search_lower)
            || runner.id().to_lowercase().contains(&search_lower)
            || runner.search_text().to_lowercase().contains(&search_lower);
        if !matches {
            continue;
        }
        let cat = runner.category();
        let label = sub_category_label(cat);
        groups
            .entry(label)
            .or_insert_with(|| SubKindGroup {
                category: cat,
                entries: Vec::new(),
            })
            .entries
            .push(SubKindEntry {
                kind_id: runner.id().to_owned(),
                label: runner.label().to_owned(),
                summary: runner.summary().to_owned(),
            });
    }

    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    for (cat_label, group) in groups {
        let SubKindGroup {
            category: cat,
            mut entries,
        } = group;
        entries.sort_by(|a, b| a.label.cmp(&b.label));
        rows.push(forge_widgets::section_header(
            cat_label,
            Some(entries.len() as u32),
            palette,
        ));
        let dot_color = sub_category_color(cat, palette);
        for entry in entries {
            rows.push(sub_kind_row(
                entry.kind_id,
                &entry.label,
                &entry.summary,
                dot_color,
                palette,
            ));
        }
    }

    let list_el: Element<'a, Message> = if rows.is_empty() {
        container(
            text(forge_widgets::tr!("triggers_create_no_results"))
                .size(FONT_SM)
                .color(p.text_muted),
        )
        .padding([spf(Spacing::Md), spf(Spacing::Md)])
        .into()
    } else {
        scrollable(column(rows)).height(Length::Fill).into()
    };

    let search = forge_widgets::search_input(
        forge_widgets::tr!("triggers_create_search_placeholder"),
        &form.search,
        |s| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::SearchChanged(s),
            )))
        },
        palette,
    );

    let header = container(
        column![
            text(forge_widgets::tr!("actions_sub_select_kind"))
                .size(FONT_SM)
                .color(p.text_primary),
            search,
        ]
        .spacing(spf(Spacing::Xs)),
    )
    .padding([spf(Spacing::Sm), spf(Spacing::Md)])
    .width(Length::Fill);

    column![header, list_el]
        .spacing(spf(Spacing::Xs))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn sub_kind_row<'a>(
    kind_id: String,
    label: &str,
    summary: &str,
    dot_color: iced::Color,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{Space, container, text};
    use iced::{Alignment, Background, Border, Color};

    let p = *palette;
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

    forge_widgets::row_card(
        text(label.to_owned()).size(FONT_SM).color(p.text_primary),
        palette,
    )
    .leading(container(dot).align_y(Alignment::Center))
    .meta(text(summary.to_owned()).size(FONT_XS).color(p.text_faint))
    .padding([spf(Spacing::Xs), spf(Spacing::Sm)])
    .on_press(Message::Actions(ActionsMsg::Editor(
        ActionEditorMsg::AddSubAction(AddSubActionMsg::KindSelected(kind_id)),
    )))
    .into()
}

fn sub_kind_form_body<'a>(
    form: &'a AddSubActionForm,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::Alignment;
    use iced::widget::{Space, button, column, container, row, text};

    let p = *palette;
    let kind_id = form.selected_kind_id.as_deref().unwrap_or("");
    let runner = rt.sub_action_registry.get(kind_id);
    let kind_label = runner.map(|r| r.label().to_owned()).unwrap_or_default();

    let back_btn = button(
        row![
            forge_widgets::icons::tabler_icon::<Message>(
                forge_widgets::icons::Icon::ArrowBackUp,
                14.0,
                p.text_secondary,
            ),
            text(forge_widgets::tr!("triggers_create_back"))
                .size(FONT_SM)
                .color(p.text_secondary),
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center),
    )
    .on_press(Message::Actions(ActionsMsg::Editor(
        ActionEditorMsg::AddSubAction(AddSubActionMsg::BackToKindPicker),
    )))
    .padding([spf(Spacing::Xxs), spf(Spacing::Xs)])
    .style(|_: &iced::Theme, _status| button::Style {
        background: None,
        border: iced::Border::default(),
        text_color: iced::Color::TRANSPARENT,
        shadow: iced::Shadow::default(),
        snap: false,
    });

    let header = container(
        column![
            back_btn,
            text(kind_label).size(FONT_SM).color(p.text_primary),
        ]
        .spacing(spf(Spacing::Xxs)),
    )
    .padding([spf(Spacing::Sm), spf(Spacing::Md)])
    .width(Length::Fill);

    let buffers = FieldBuffers {
        text: &form.text_buffer,
        overrides: &form.overrides_buffer,
    };
    let options = dynamic_options_for(form, rt);
    let on_edit = |edit: FieldEditMsg| {
        let m = match edit {
            FieldEditMsg::Set(k, v) => AddSubActionMsg::FieldChanged(k, v),
            FieldEditMsg::IntInput(k, raw) => AddSubActionMsg::IntInputChanged(k, raw),
            FieldEditMsg::Clear(k) => AddSubActionMsg::FieldCleared(k),
        };
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(m)))
    };

    let config_fields = runner.map(|r| r.config_fields()).unwrap_or_default();
    let field_rows: Vec<Element<'a, Message>> = config_fields
        .iter()
        .map(|field| render_field(field, &buffers, &options, palette, on_edit))
        .collect();

    let fields_section: Element<'a, Message> = if field_rows.is_empty() {
        container(
            text(forge_widgets::tr!("actions_sub_no_config"))
                .size(FONT_SM)
                .color(p.text_muted),
        )
        .padding([0.0, spf(Spacing::Md)])
        .into()
    } else {
        column(
            std::iter::once(forge_widgets::section_header(
                forge_widgets::tr!("triggers_create_section_config"),
                None,
                palette,
            ))
            .chain(field_rows)
            .collect::<Vec<_>>(),
        )
        .padding([0.0, spf(Spacing::Md)])
        .into()
    };

    iced::widget::scrollable(
        column![
            header,
            fields_section,
            Space::new().height(spf(Spacing::Md))
        ]
        .spacing(spf(Spacing::Xs)),
    )
    .height(Length::Fill)
    .into()
}

pub(crate) fn add_sub_action_modal_view<'a>(
    form: &'a AddSubActionForm,
    rt: &'a RuntimeView,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use crate::actions::SubActionFormStep;
    use forge_widgets::BannerKind;
    use iced::widget::{Space, column, container, row, text};

    let on_cancel = Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
        AddSubActionMsg::Cancel,
    )));

    let title_label = if form.editing_index.is_some() {
        forge_widgets::tr!("actions_sub_modal_edit_title")
    } else {
        forge_widgets::tr!("actions_sub_modal_add_title")
    };

    let body_inner: Element<'a, Message> = match form.step {
        SubActionFormStep::PickKind => sub_kind_picker_body(form, rt, palette),
        SubActionFormStep::FillForm => sub_kind_form_body(form, rt, palette),
    };

    let mut stacked = column![body_inner].spacing(spf(Spacing::Sm));
    if let Some(err) = form.error.as_deref() {
        stacked = stacked.push(
            container(forge_widgets::live_status_banner(
                BannerKind::Error,
                err,
                None,
                palette,
            ))
            .padding([0.0, spf(Spacing::Md)]),
        );
    }

    let body = container(stacked).width(Length::Fill).height(Length::Fill);

    let footer: Element<'a, Message> = {
        let cancel_btn = forge_widgets::secondary_button(
            forge_widgets::tr!("actions_sub_modal_cancel_btn"),
            on_cancel.clone(),
            palette,
        );
        let buttons: Element<'a, Message> = if matches!(form.step, SubActionFormStep::FillForm) {
            let btn_label = if form.editing_index.is_some() {
                forge_widgets::tr!("actions_sub_modal_save_btn")
            } else {
                forge_widgets::tr!("actions_sub_modal_add_btn")
            };
            let add_on_press = Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::Submit,
            )));
            let add_btn = if form.selected_kind_id.is_some() && !form.saving {
                forge_widgets::primary_button(btn_label, add_on_press, palette)
            } else {
                forge_widgets::secondary_button(btn_label, Message::Noop, palette)
            };
            row![cancel_btn, add_btn].spacing(spf(Spacing::Xs)).into()
        } else {
            row![cancel_btn].into()
        };
        container(
            row![
                text(forge_widgets::tr!("actions_esc_hint"))
                    .size(FONT_XS)
                    .color(palette.text_faint)
                    .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
                Space::new().width(Length::Fill),
                buttons,
            ]
            .align_y(iced::alignment::Vertical::Center),
        )
        .width(Length::Fill)
        .padding([12_u16, 16_u16])
        .style(move |_t: &iced::Theme| container::Style {
            border: iced::Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
    };

    let content = column![body, footer]
        .width(Length::Fill)
        .height(Length::Fill);

    forge_widgets::SideSheet::new(content)
        .open(true)
        .palette(palette)
        .width(forge_widgets::SheetWidth::new(480.0, 360.0, 720.0))
        .header(forge_widgets::SheetHeader {
            title: std::borrow::Cow::Owned(title_label),
            subtitle: None,
            on_close: Some(on_cancel.clone()),
        })
        .on_close(on_cancel)
        .into()
}
