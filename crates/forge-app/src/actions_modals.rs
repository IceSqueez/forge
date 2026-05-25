use forge_widgets::ForgePalette;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_SM, FONT_XS, Spacing, spf};
use iced::{Element, Length};

use crate::actions::{
    AddActionForm, AddActionMsg, AddSubActionForm, AddSubActionMsg, AddTriggerForm, AddTriggerMsg,
    SubActionKindChoice, TriggerCategory, kind_label, kind_summary,
};
use crate::message::{ActionEditorMsg, ActionsMsg, Message};
use crate::page_chrome::sheet_chrome;

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
        "My automation",
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
        forge_widgets::section_header("NAME", None, palette),
        name_row,
    ]
    .spacing(spf(Spacing::Xs));

    let group_input = forge_widgets::text_input_field(
        "Examples",
        &form.group,
        |v| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::GroupChanged(v),
            )))
        },
        palette,
    );

    let group_block = column![
        forge_widgets::section_header("GROUP", None, palette),
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
        forge_widgets::section_header("QUEUE", None, palette),
        queue_select,
    ]
    .spacing(spf(Spacing::Xs));

    let two_col = row![group_block, queue_block].spacing(spf(Spacing::Sm));

    let desc_input = forge_widgets::text_input_field(
        "Plays a sound, shows overlay alert...",
        &form.description,
        |v| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::DescriptionChanged(v),
            )))
        },
        palette,
    );

    let desc_block = column![
        forge_widgets::section_header("DESCRIPTION", None, palette),
        desc_input,
    ]
    .spacing(spf(Spacing::Xs));

    let enabled_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: "Enabled",
            description: "Action runs when a trigger fires.",
            value: form.enabled,
            on_toggle: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::EnabledToggled(!form.enabled),
            ))),
        },
    );

    let concurrent_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: "Concurrent execution",
            description: "Allow parallel runs in this queue.",
            value: form.concurrent,
            on_toggle: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::ConcurrentToggled(!form.concurrent),
            ))),
        },
    );

    let bypass_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: "Bypass queue pause",
            description: "Always run even if queue is paused.",
            value: form.bypass_pause,
            on_toggle: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::BypassPauseToggled(!form.bypass_pause),
            ))),
        },
    );

    let random_pick_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: "Random pick",
            description: "Run ONE random sub-action per trigger instead of all.",
            value: form.random_pick,
            on_toggle: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::RandomPickToggled(!form.random_pick),
            ))),
        },
    );

    let behavior_header = forge_widgets::section_header("BEHAVIOR", None, palette);

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
        "Cancel",
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
            AddActionMsg::Cancel,
        ))),
        palette,
    );

    let create_on_press = Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
        AddActionMsg::Submit,
    )));
    let create_btn = if form.is_valid() && !form.saving {
        forge_widgets::primary_button("Create action", create_on_press, palette)
    } else {
        forge_widgets::secondary_button("Create action", Message::Noop, palette)
    };

    let footer_buttons = row![cancel_btn, create_btn].spacing(spf(Spacing::Xs));

    let footer: Element<'_, Message> = iced::widget::container(
        row![
            text("ESC to cancel")
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
            title: "New action",
            on_close: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::Cancel,
            ))),
            kbd_hint: None,
        },
        body_col.into(),
        footer,
    )
}

pub(crate) fn add_trigger_modal_view<'a>(
    form: &'a AddTriggerForm,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::BannerKind;
    use iced::widget::{column, row, scrollable, text};
    use iced::{Alignment, Background, Length};

    let search_input = forge_widgets::search_input(
        "Filter trigger types...",
        &form.search,
        |v| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
                AddTriggerMsg::SearchChanged(v),
            )))
        },
        palette,
    );

    let chip_all = forge_widgets::category_chip(
        palette,
        "All",
        palette.brand,
        form.category == TriggerCategory::All,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::CategorySelected(TriggerCategory::All),
        ))),
    );
    let chip_chat = forge_widgets::category_chip(
        palette,
        "Chat",
        palette.brand,
        form.category == TriggerCategory::Chat,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::CategorySelected(TriggerCategory::Chat),
        ))),
    );
    let chip_subs = forge_widgets::category_chip(
        palette,
        "Subscriptions",
        palette.brand,
        form.category == TriggerCategory::Subscriptions,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::CategorySelected(TriggerCategory::Subscriptions),
        ))),
    );
    let chip_bits = forge_widgets::category_chip(
        palette,
        "Bits",
        palette.bits,
        form.category == TriggerCategory::Bits,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::CategorySelected(TriggerCategory::Bits),
        ))),
    );
    let chip_raids = forge_widgets::category_chip(
        palette,
        "Raids",
        palette.random,
        form.category == TriggerCategory::Raids,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::CategorySelected(TriggerCategory::Raids),
        ))),
    );
    let chip_obs = forge_widgets::category_chip(
        palette,
        "OBS",
        palette.brand,
        form.category == TriggerCategory::Obs,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::CategorySelected(TriggerCategory::Obs),
        ))),
    );

    let chips_row = row![
        chip_all, chip_chat, chip_subs, chip_bits, chip_raids, chip_obs
    ]
    .spacing(spf(Spacing::Xs));

    let visible = form.visible_kinds();
    let is_empty = visible.is_empty();
    let mut grid_col = column![].spacing(spf(Spacing::Xs));
    for kind in visible {
        let selected = form.selected_kind.as_ref() == Some(&kind);
        let lbl = kind_label(&kind);
        let summ = kind_summary(&kind);
        let card = trigger_picker_card(
            lbl,
            summ,
            selected,
            palette,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
                AddTriggerMsg::KindSelected(kind),
            ))),
        );
        grid_col = grid_col.push(card);
    }

    if form.selected_kind.is_none() && is_empty {
        grid_col = grid_col.push(
            text("No trigger types match your filter.")
                .size(FONT_SM)
                .color(palette.text_faint),
        );
    }

    let mut config_col = column![].spacing(spf(Spacing::Xs));

    if let Some(kind) = &form.selected_kind {
        match kind {
            forge_types::TriggerKind::TwitchChatCommand => {
                let cmd_input = forge_widgets::text_input_field(
                    "!quote",
                    &form.config.command_name,
                    |v| {
                        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
                            AddTriggerMsg::CommandNameChanged(v),
                        )))
                    },
                    palette,
                );
                let cmd_block = column![
                    forge_widgets::section_header("COMMAND NAME", None, palette),
                    cmd_input,
                ]
                .spacing(spf(Spacing::Xs));

                let cooldown_input = forge_widgets::text_input_field(
                    "0",
                    &form.config.cooldown_secs,
                    |v| {
                        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
                            AddTriggerMsg::CooldownChanged(v),
                        )))
                    },
                    palette,
                );
                let cooldown_block = column![
                    forge_widgets::section_header("COOLDOWN (SECS)", None, palette),
                    cooldown_input,
                ]
                .spacing(spf(Spacing::Xs));

                let p = *palette;
                let perm_options: Vec<String> = vec![
                    "Everyone".to_string(),
                    "Subscriber".to_string(),
                    "Vip".to_string(),
                    "Moderator".to_string(),
                    "Broadcaster".to_string(),
                ];
                let selected_perm = permission_label(&form.config.permission).to_string();
                let perm_select: Element<'_, Message> =
                    iced::widget::pick_list(perm_options, Some(selected_perm), |name: String| {
                        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
                            AddTriggerMsg::PermissionSelected(permission_from_label(&name)),
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

                let perm_block = column![
                    forge_widgets::section_header("PERMISSION", None, palette),
                    perm_select,
                ]
                .spacing(spf(Spacing::Xs));

                config_col = config_col
                    .push(cmd_block)
                    .push(cooldown_block)
                    .push(perm_block);
            }
            forge_types::TriggerKind::TwitchCheer => {
                let bits_input = forge_widgets::text_input_field(
                    "1",
                    &form.config.min_bits,
                    |v| {
                        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
                            AddTriggerMsg::MinBitsChanged(v),
                        )))
                    },
                    palette,
                );
                let bits_block = column![
                    forge_widgets::section_header("MINIMUM BITS", None, palette),
                    bits_input,
                ]
                .spacing(spf(Spacing::Xs));
                config_col = config_col.push(bits_block);
            }
            _ => {
                config_col = config_col.push(
                    text("No configuration required for this trigger type.")
                        .size(FONT_XS)
                        .color(palette.text_muted),
                );
            }
        }
    }

    let mut body_col = column![search_input, chips_row, scrollable(grid_col).height(200),]
        .spacing(spf(Spacing::Xs));

    if form.selected_kind.is_some() {
        body_col = body_col.push(
            iced::widget::container(column![].spacing(0))
                .width(Length::Fill)
                .height(0.5)
                .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(Background::Color(palette.border_regular)),
                    ..iced::widget::container::Style::default()
                }),
        );
        body_col = body_col.push(forge_widgets::section_header("CONFIGURE", None, palette));
        body_col = body_col.push(config_col);
    }

    if let Some(err) = form.error.as_deref() {
        body_col = body_col.push(forge_widgets::live_status_banner(
            BannerKind::Error,
            err,
            None,
            palette,
        ));
    }

    let cancel_btn = forge_widgets::secondary_button(
        "Cancel",
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::Cancel,
        ))),
        palette,
    );

    let save_on_press = Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
        AddTriggerMsg::Submit,
    )));
    let save_btn = if form.is_valid() && !form.saving {
        forge_widgets::primary_button("Add trigger", save_on_press, palette)
    } else {
        forge_widgets::secondary_button("Add trigger", Message::Noop, palette)
    };

    let footer_buttons = row![cancel_btn, save_btn].spacing(spf(Spacing::Xs));

    let footer: Element<'_, Message> = iced::widget::container(
        row![
            text("ESC to cancel")
                .size(FONT_XS)
                .color(palette.text_faint)
                .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
            iced::widget::Space::new().width(Length::Fill),
            footer_buttons,
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .into();

    let panel = sheet_chrome(
        "Add trigger",
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::Cancel,
        ))),
        body_col.into(),
        Some(footer),
        palette,
    );
    forge_widgets::side_sheet(
        panel,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::Cancel,
        ))),
        forge_widgets::SheetEdge::Right,
        480.0,
        palette,
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

    let chip_send_chat = forge_widgets::category_chip(
        palette,
        "Send chat",
        palette.brand,
        form.kind == SubActionKindChoice::SendChat,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::SendChat),
        ))),
    );
    let chip_set_global = forge_widgets::category_chip(
        palette,
        "Set global",
        palette.warning,
        form.kind == SubActionKindChoice::SetGlobal,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::SetGlobal),
        ))),
    );
    let chip_delay = forge_widgets::category_chip(
        palette,
        "Delay",
        palette.info,
        form.kind == SubActionKindChoice::Delay,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::Delay),
        ))),
    );
    let chip_log = forge_widgets::category_chip(
        palette,
        "Log",
        palette.text_muted,
        form.kind == SubActionKindChoice::Log,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::Log),
        ))),
    );
    let chip_play_sound = forge_widgets::category_chip(
        palette,
        "Play sound",
        palette.success,
        form.kind == SubActionKindChoice::PlaySound,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::PlaySound),
        ))),
    );
    let chip_speak = forge_widgets::category_chip(
        palette,
        "Speak",
        palette.info,
        form.kind == SubActionKindChoice::Speak,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::Speak),
        ))),
    );
    let chip_read_file = forge_widgets::category_chip(
        palette,
        "Read file",
        palette.random,
        form.kind == SubActionKindChoice::ReadFile,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::ReadFile),
        ))),
    );
    let chip_random_int = forge_widgets::category_chip(
        palette,
        "Random int",
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
            let helper = text("Variables: %user%, %message%, %args%")
                .size(FONT_XS)
                .color(palette.warning)
                .font(forge_widgets::font(forge_widgets::FontRole::Monospace));
            let msg_block = column![
                forge_widgets::section_header("MESSAGE", None, palette),
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
                forge_widgets::section_header("TARGET PLATFORM", None, palette),
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
                forge_widgets::section_header("VARIABLE NAME", None, palette),
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
            let helper = text("Supports variable interpolation")
                .size(FONT_XS)
                .color(palette.warning)
                .font(forge_widgets::font(forge_widgets::FontRole::Monospace));
            let val_block = column![
                forge_widgets::section_header("VALUE", None, palette),
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
                forge_widgets::section_header("MILLISECONDS", None, palette),
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
                forge_widgets::section_header("LEVEL", None, palette),
                level_select,
            ]
            .spacing(spf(Spacing::Xs));

            let msg_input = forge_widgets::text_input_field(
                "Action started",
                &form.config.log_message,
                |v| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::LogMessageChanged(v),
                    )))
                },
                palette,
            );
            let msg_block = column![
                forge_widgets::section_header("MESSAGE", None, palette),
                msg_input,
            ]
            .spacing(spf(Spacing::Xs));

            column![level_block, msg_block]
                .spacing(spf(Spacing::Sm))
                .into()
        }
        SubActionKindChoice::PlaySound => {
            if form.available_clips.is_empty() {
                let hint = text("No clips yet \u{2014} add one in the Soundboard screen first.")
                    .size(FONT_SM)
                    .color(palette.text_muted);
                column![forge_widgets::section_header("CLIP", None, palette), hint]
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
                    forge_widgets::section_header("CLIP", None, palette),
                    clip_select
                ]
                .spacing(spf(Spacing::Xs))
                .into()
            }
        }
        SubActionKindChoice::Speak => {
            use iced::widget::column;
            let text_block = column![
                forge_widgets::section_header("TEXT", None, palette),
                forge_widgets::inputs::text_input_field(
                    "Text to speak…",
                    &form.config.speak_text,
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::SpeakTextChanged(v)
                    ))),
                    palette,
                ),
            ]
            .spacing(spf(Spacing::Xs));
            let voice_block = column![
                forge_widgets::section_header("VOICE OVERRIDE (optional)", None, palette),
                forge_widgets::inputs::text_input_field(
                    "Leave blank to use alias resolver",
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
                forge_widgets::section_header("PATH (relative to assets sandbox)", None, palette),
                forge_widgets::inputs::text_input_field(
                    "greetings/welcome.txt",
                    &form.config.read_file_path,
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::ReadFilePathChanged(v)
                    ))),
                    palette,
                ),
                text("Sandboxed under data_dir/assets/ · no ../ traversal · max 1 MiB")
                    .size(FONT_XS)
                    .color(palette.text_muted)
                    .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
            ]
            .spacing(spf(Spacing::Xxs));
            let target_block = column![
                forge_widgets::section_header("TARGET VARIABLE", None, palette),
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
                forge_widgets::section_header("MIN", None, palette),
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
                forge_widgets::section_header("MAX", None, palette),
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
                forge_widgets::section_header("TARGET VARIABLE", None, palette),
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

    let (btn_label, title_label) = if form.editing_index.is_some() {
        ("Save changes", "Edit step")
    } else {
        ("Add step", "Add step")
    };

    let cancel_btn = forge_widgets::secondary_button(
        "Cancel",
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::Cancel,
        ))),
        palette,
    );

    let add_on_press = Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
        AddSubActionMsg::Submit,
    )));
    let add_btn = if form.is_valid() && !form.saving {
        forge_widgets::primary_button(btn_label, add_on_press, palette)
    } else {
        forge_widgets::secondary_button(btn_label, Message::Noop, palette)
    };

    let footer_buttons = row![cancel_btn, add_btn].spacing(spf(Spacing::Xs));

    let footer: iced::Element<'_, Message> = iced::widget::container(
        row![
            text("ESC to cancel")
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

    let panel = sheet_chrome(
        title_label,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::Cancel,
        ))),
        body_col.into(),
        Some(footer),
        palette,
    );
    forge_widgets::side_sheet(
        panel,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::Cancel,
        ))),
        forge_widgets::SheetEdge::Right,
        480.0,
        palette,
    )
}

fn trigger_picker_card<'a>(
    label: &'a str,
    summary: &'a str,
    selected: bool,
    palette: &'a ForgePalette,
    on_press: Message,
) -> Element<'a, Message> {
    use iced::widget::{button, column, container, row, text};
    use iced::{Alignment, Background, Border, Length};

    let icon_el = container(tabler_icon(Icon::Bolt, FONT_SM, palette.brand))
        .width(24)
        .height(24)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.surface_overlay)),
            border: Border {
                radius: forge_widgets::radius(forge_widgets::Radius::Sm).into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let label_col = column![
        text(label).size(FONT_SM).color(palette.text_primary),
        text(summary)
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
    ]
    .spacing(spf(Spacing::Xxs));

    let inner = row![icon_el, container(label_col).width(Length::Fill),]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center);

    let border_color = if selected {
        palette.brand
    } else {
        palette.border_regular
    };
    let border_width = if selected { 1.0 } else { 0.5 };

    button(inner)
        .on_press(on_press)
        .padding(iced::Padding {
            top: 8.0,
            right: 10.0,
            bottom: 8.0,
            left: 10.0,
        })
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, _status| button::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: border_color,
                width: border_width,
                radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
            },
            text_color: palette.text_primary,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .into()
}

fn permission_label(perm: &forge_types::CommandPermission) -> &'static str {
    match perm {
        forge_types::CommandPermission::Everyone => "Everyone",
        forge_types::CommandPermission::Subscriber => "Subscriber",
        forge_types::CommandPermission::Vip => "Vip",
        forge_types::CommandPermission::Moderator => "Moderator",
        forge_types::CommandPermission::Broadcaster => "Broadcaster",
    }
}

fn permission_from_label(label: &str) -> forge_types::CommandPermission {
    match label {
        "Subscriber" => forge_types::CommandPermission::Subscriber,
        "Vip" => forge_types::CommandPermission::Vip,
        "Moderator" => forge_types::CommandPermission::Moderator,
        "Broadcaster" => forge_types::CommandPermission::Broadcaster,
        _ => forge_types::CommandPermission::Everyone,
    }
}
