use forge_widgets::ForgePalette;
use forge_widgets::tokens::{FONT_SM, FONT_XS, Spacing, spf};
use iced::{Element, Length};

use crate::actions::{
    AddActionForm, AddActionMsg, AddSubActionForm, AddSubActionMsg, SubActionKindChoice,
};
use crate::message::{ActionEditorMsg, ActionsMsg, Message};

pub(crate) fn add_action_modal_view<'a>(
    form: &'a AddActionForm,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::{BannerKind, ModalProps, ToggleProps};
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

    let group_input = forge_widgets::text_input_field(
        forge_widgets::tr!("actions_group_placeholder"),
        &form.group,
        |v| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::GroupChanged(v),
            )))
        },
        palette,
    );

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

    let desc_input = forge_widgets::text_input_field(
        forge_widgets::tr!("actions_description_placeholder"),
        &form.description,
        |v| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::DescriptionChanged(v),
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
            on_close: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::Cancel,
            ))),
            kbd_hint: None,
        },
        body_col.into(),
        footer,
    )
}

fn log_level_label(level: &forge_types::LogLevel) -> &'static str {
    match level {
        forge_types::LogLevel::Trace => "Trace",
        forge_types::LogLevel::Debug => "Debug",
        forge_types::LogLevel::Info => "Info",
        forge_types::LogLevel::Warn => "Warn",
        forge_types::LogLevel::Error => "Error",
    }
}

fn log_level_from_label(label: &str) -> forge_types::LogLevel {
    match label {
        "Trace" => forge_types::LogLevel::Trace,
        "Debug" => forge_types::LogLevel::Debug,
        "Warn" => forge_types::LogLevel::Warn,
        "Error" => forge_types::LogLevel::Error,
        _ => forge_types::LogLevel::Info,
    }
}

pub(crate) fn add_sub_action_modal_view<'a>(
    form: &'a AddSubActionForm,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::BannerKind;
    use iced::Length;
    use iced::widget::{column, row, text};

    let lbl_send_chat = forge_widgets::tr!("actions_sub_chip_send_chat");
    let lbl_set_global = forge_widgets::tr!("actions_sub_chip_set_global");
    let lbl_delay = forge_widgets::tr!("actions_sub_chip_delay");
    let lbl_log = forge_widgets::tr!("actions_sub_chip_log");
    let lbl_play_sound = forge_widgets::tr!("actions_sub_chip_play_sound");
    let lbl_speak = forge_widgets::tr!("actions_sub_chip_speak");
    let lbl_read_file = forge_widgets::tr!("actions_sub_chip_read_file");
    let lbl_random_int = forge_widgets::tr!("actions_sub_chip_random_int");

    let chip_send_chat = forge_widgets::category_chip(
        palette,
        &lbl_send_chat,
        palette.brand,
        form.kind == SubActionKindChoice::SendChat,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::SendChat),
        ))),
    );
    let chip_set_global = forge_widgets::category_chip(
        palette,
        &lbl_set_global,
        palette.warning,
        form.kind == SubActionKindChoice::SetGlobal,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::SetGlobal),
        ))),
    );
    let chip_delay = forge_widgets::category_chip(
        palette,
        &lbl_delay,
        palette.info,
        form.kind == SubActionKindChoice::Delay,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::Delay),
        ))),
    );
    let chip_log = forge_widgets::category_chip(
        palette,
        &lbl_log,
        palette.text_muted,
        form.kind == SubActionKindChoice::Log,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::Log),
        ))),
    );
    let chip_play_sound = forge_widgets::category_chip(
        palette,
        &lbl_play_sound,
        palette.success,
        form.kind == SubActionKindChoice::PlaySound,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::PlaySound),
        ))),
    );
    let chip_speak = forge_widgets::category_chip(
        palette,
        &lbl_speak,
        palette.info,
        form.kind == SubActionKindChoice::Speak,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::Speak),
        ))),
    );
    let chip_read_file = forge_widgets::category_chip(
        palette,
        &lbl_read_file,
        palette.random,
        form.kind == SubActionKindChoice::ReadFile,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::ReadFile),
        ))),
    );
    let chip_random_int = forge_widgets::category_chip(
        palette,
        &lbl_random_int,
        palette.warning,
        form.kind == SubActionKindChoice::RandomInt,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::RandomInt),
        ))),
    );
    let chips_row = row![
        chip_send_chat,
        chip_set_global,
        chip_delay,
        chip_log,
        chip_play_sound,
        chip_speak,
        chip_read_file,
        chip_random_int,
    ]
    .spacing(spf(Spacing::Xs));

    let config_block: iced::Element<'_, Message> = match form.kind {
        SubActionKindChoice::SendChat => {
            let msg_input = forge_widgets::text_input_field(
                "Hello %user%!",
                &form.config.send_chat_message,
                |v| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::SendChatMessageChanged(v),
                    )))
                },
                palette,
            );
            let helper = text(forge_widgets::tr!("actions_sub_helper_variables"))
                .size(FONT_XS)
                .color(palette.warning)
                .font(forge_widgets::font(forge_widgets::FontRole::Monospace));
            let msg_block = column![
                forge_widgets::section_header(
                    forge_widgets::tr!("actions_sub_section_message"),
                    None,
                    palette
                ),
                msg_input,
                helper,
            ]
            .spacing(spf(Spacing::Xxs));

            let p = *palette;
            let target_options: Vec<String> = vec!["twitch".to_string()];
            let selected_target = form.config.send_chat_target.clone();
            let target_select: iced::Element<'_, Message> =
                iced::widget::pick_list(target_options, Some(selected_target), |name: String| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::SendChatTargetChanged(name),
                    )))
                })
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
            let target_block = column![
                forge_widgets::section_header(
                    forge_widgets::tr!("actions_sub_section_target_platform"),
                    None,
                    palette
                ),
                target_select,
            ]
            .spacing(spf(Spacing::Xs));

            column![msg_block, target_block]
                .spacing(spf(Spacing::Sm))
                .into()
        }
        SubActionKindChoice::SetGlobal => {
            let name_input = forge_widgets::text_input_field(
                "my_counter",
                &form.config.set_global_name,
                |v| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::SetGlobalNameChanged(v),
                    )))
                },
                palette,
            );
            let name_block = column![
                forge_widgets::section_header(
                    forge_widgets::tr!("actions_sub_section_variable_name"),
                    None,
                    palette
                ),
                name_input,
            ]
            .spacing(spf(Spacing::Xs));

            let val_input = forge_widgets::text_input_field(
                "%user% or 42",
                &form.config.set_global_value,
                |v| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::SetGlobalValueChanged(v),
                    )))
                },
                palette,
            );
            let helper = text(forge_widgets::tr!("actions_sub_helper_interpolation"))
                .size(FONT_XS)
                .color(palette.warning)
                .font(forge_widgets::font(forge_widgets::FontRole::Monospace));
            let val_block = column![
                forge_widgets::section_header(
                    forge_widgets::tr!("actions_sub_section_value"),
                    None,
                    palette
                ),
                val_input,
                helper,
            ]
            .spacing(spf(Spacing::Xxs));

            column![name_block, val_block]
                .spacing(spf(Spacing::Sm))
                .into()
        }
        SubActionKindChoice::Delay => {
            let ms_input = forge_widgets::text_input_field(
                "500",
                &form.config.delay_ms,
                |v| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::DelayMsChanged(v),
                    )))
                },
                palette,
            );
            column![
                forge_widgets::section_header(
                    forge_widgets::tr!("actions_sub_section_milliseconds"),
                    None,
                    palette
                ),
                ms_input,
            ]
            .spacing(spf(Spacing::Xs))
            .into()
        }
        SubActionKindChoice::Log => {
            let p = *palette;
            let level_options: Vec<String> = vec![
                "Trace".to_string(),
                "Debug".to_string(),
                "Info".to_string(),
                "Warn".to_string(),
                "Error".to_string(),
            ];
            let selected_level = log_level_label(&form.config.log_level).to_string();
            let level_select: iced::Element<'_, Message> =
                iced::widget::pick_list(level_options, Some(selected_level), |name: String| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::LogLevelSelected(log_level_from_label(&name)),
                    )))
                })
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
            let level_block = column![
                forge_widgets::section_header(
                    forge_widgets::tr!("actions_sub_section_level"),
                    None,
                    palette
                ),
                level_select,
            ]
            .spacing(spf(Spacing::Xs));

            let msg_input = forge_widgets::text_input_field(
                forge_widgets::tr!("actions_log_message_placeholder"),
                &form.config.log_message,
                |v| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::LogMessageChanged(v),
                    )))
                },
                palette,
            );
            let msg_block = column![
                forge_widgets::section_header(
                    forge_widgets::tr!("actions_sub_section_message"),
                    None,
                    palette
                ),
                msg_input,
            ]
            .spacing(spf(Spacing::Xs));

            column![level_block, msg_block]
                .spacing(spf(Spacing::Sm))
                .into()
        }
        SubActionKindChoice::PlaySound => {
            if form.available_clips.is_empty() {
                let hint = text(forge_widgets::tr!("actions_sub_no_clips"))
                    .size(FONT_SM)
                    .color(palette.text_muted);
                column![
                    forge_widgets::section_header(
                        forge_widgets::tr!("actions_sub_section_clip"),
                        None,
                        palette
                    ),
                    hint
                ]
                .spacing(spf(Spacing::Xs))
                .into()
            } else {
                let p = *palette;
                let clip_names: Vec<String> = form
                    .available_clips
                    .iter()
                    .map(|(_, n)| n.clone())
                    .collect();
                let selected_name = form.config.play_sound_clip_id.and_then(|id| {
                    form.available_clips
                        .iter()
                        .find(|(cid, _)| *cid == id)
                        .map(|(_, n)| n.clone())
                });
                let clips_for_closure = form.available_clips.clone();
                let clip_select: iced::Element<'_, Message> =
                    iced::widget::pick_list(clip_names, selected_name, move |name: String| {
                        let clip_id = clips_for_closure
                            .iter()
                            .find(|(_, n)| *n == name)
                            .map(|(id, _)| *id)
                            .unwrap_or_default();
                        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                            AddSubActionMsg::PlaySoundClipSelected(clip_id),
                        )))
                    })
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
                column![
                    forge_widgets::section_header(
                        forge_widgets::tr!("actions_sub_section_clip"),
                        None,
                        palette
                    ),
                    clip_select
                ]
                .spacing(spf(Spacing::Xs))
                .into()
            }
        }
        SubActionKindChoice::Speak => {
            use iced::widget::column;
            let text_block = column![
                forge_widgets::section_header(
                    forge_widgets::tr!("actions_sub_section_text"),
                    None,
                    palette
                ),
                forge_widgets::inputs::text_input_field(
                    forge_widgets::tr!("actions_speak_text_placeholder"),
                    &form.config.speak_text,
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::SpeakTextChanged(v)
                    ))),
                    palette,
                ),
            ]
            .spacing(spf(Spacing::Xs));
            let voice_override_lbl = forge_widgets::tr!("actions_sub_section_voice_override");
            let voice_block = column![
                forge_widgets::section_header(voice_override_lbl, None, palette),
                forge_widgets::inputs::text_input_field(
                    forge_widgets::tr!("actions_sub_voice_hint"),
                    &form.config.speak_voice_override,
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::SpeakVoiceOverrideChanged(v)
                    ))),
                    palette,
                ),
            ]
            .spacing(spf(Spacing::Xs));
            column![text_block, voice_block]
                .spacing(spf(Spacing::Sm))
                .into()
        }
        SubActionKindChoice::ReadFile => {
            use iced::widget::column;
            let path_block = column![
                forge_widgets::section_header(
                    forge_widgets::tr!("actions_sub_section_path"),
                    None,
                    palette
                ),
                forge_widgets::inputs::text_input_field(
                    "greetings/welcome.txt",
                    &form.config.read_file_path,
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::ReadFilePathChanged(v)
                    ))),
                    palette,
                ),
                text(forge_widgets::tr!("actions_sub_path_hint"))
                    .size(FONT_XS)
                    .color(palette.text_muted)
                    .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
            ]
            .spacing(spf(Spacing::Xxs));
            let target_block = column![
                forge_widgets::section_header(
                    forge_widgets::tr!("actions_sub_section_target_var"),
                    None,
                    palette
                ),
                forge_widgets::inputs::text_input_field(
                    "welcome_text",
                    &form.config.read_file_target_var,
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::ReadFileTargetVarChanged(v)
                    ))),
                    palette,
                ),
            ]
            .spacing(spf(Spacing::Xs));
            column![path_block, target_block]
                .spacing(spf(Spacing::Sm))
                .into()
        }
        SubActionKindChoice::RandomInt => {
            use iced::widget::column;
            let min_block = column![
                forge_widgets::section_header(
                    forge_widgets::tr!("actions_sub_section_min"),
                    None,
                    palette
                ),
                forge_widgets::inputs::text_input_field(
                    "1",
                    &form.config.random_int_min,
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::RandomIntMinChanged(v)
                    ))),
                    palette,
                ),
            ]
            .spacing(spf(Spacing::Xs));
            let max_block = column![
                forge_widgets::section_header(
                    forge_widgets::tr!("actions_sub_section_max"),
                    None,
                    palette
                ),
                forge_widgets::inputs::text_input_field(
                    "100",
                    &form.config.random_int_max,
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::RandomIntMaxChanged(v)
                    ))),
                    palette,
                ),
            ]
            .spacing(spf(Spacing::Xs));
            let target_block = column![
                forge_widgets::section_header(
                    forge_widgets::tr!("actions_sub_section_target_var"),
                    None,
                    palette
                ),
                forge_widgets::inputs::text_input_field(
                    "dice_roll",
                    &form.config.random_int_target_var,
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::RandomIntTargetVarChanged(v)
                    ))),
                    palette,
                ),
            ]
            .spacing(spf(Spacing::Xs));
            column![
                row![min_block, max_block].spacing(spf(Spacing::Xs)),
                target_block
            ]
            .spacing(spf(Spacing::Sm))
            .into()
        }
    };

    let mut body_col = column![chips_row, config_block].spacing(spf(Spacing::Md));

    if let Some(err) = form.error.as_deref() {
        body_col = body_col.push(forge_widgets::live_status_banner(
            BannerKind::Error,
            err,
            None,
            palette,
        ));
    }

    let btn_label = if form.editing_index.is_some() {
        forge_widgets::tr!("actions_sub_modal_save_btn")
    } else {
        forge_widgets::tr!("actions_sub_modal_add_btn")
    };
    let title_label = if form.editing_index.is_some() {
        forge_widgets::tr!("actions_sub_modal_edit_title")
    } else {
        forge_widgets::tr!("actions_sub_modal_add_title")
    };

    let cancel_btn = forge_widgets::secondary_button(
        forge_widgets::tr!("actions_sub_modal_cancel_btn"),
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::Cancel,
        ))),
        palette,
    );

    let add_on_press = Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
        AddSubActionMsg::Submit,
    )));
    let add_btn = if form.is_valid() && !form.saving {
        forge_widgets::primary_button(btn_label.clone(), add_on_press, palette)
    } else {
        forge_widgets::secondary_button(btn_label.clone(), Message::Noop, palette)
    };

    let footer_buttons = row![cancel_btn, add_btn].spacing(spf(Spacing::Xs));

    let footer: iced::Element<'_, Message> = iced::widget::container(
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

    let on_cancel = Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
        AddSubActionMsg::Cancel,
    )));

    let footer_bar = iced::widget::container(footer)
        .width(Length::Fill)
        .padding([12_u16, 16_u16])
        .style(move |_t: &iced::Theme| iced::widget::container::Style {
            border: iced::Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        });

    let body = iced::widget::container(body_col)
        .width(Length::Fill)
        .height(Length::Fill);

    let content = iced::widget::column![body, footer_bar]
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
