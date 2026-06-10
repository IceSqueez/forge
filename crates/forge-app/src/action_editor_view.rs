use forge_types::ActionId;
use forge_widgets::ForgePalette;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::popover::{MenuItem, MenuPlacement, menu_button};
use forge_widgets::status::{StatusVariant, status_pill};
use forge_widgets::tokens::{FONT_LG, FONT_SM, FONT_XS, Spacing, sp, spf};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use crate::Screen;
use crate::actions::{
    ActionsGroup, AddSubActionMsg, RemoveSubActionMsg, TriggerCategory, category_of,
    trigger_label_of,
};
use crate::app::App;
use crate::message::{ActionEditorMsg, ActionsMsg, Message, MoveSubActionMsg};

fn sub_action_summary(step: &forge_types::SubActionStep) -> (&'static str, String, String) {
    fn as_str(v: &forge_types::Variant) -> &str {
        if let forge_types::Variant::String(s) = v {
            s.as_str()
        } else {
            ""
        }
    }
    fn as_i64(v: &forge_types::Variant) -> i64 {
        if let forge_types::Variant::Int(n) = v {
            *n
        } else {
            0
        }
    }
    match step.kind_id.as_str() {
        "twitch.chat.send_message" => {
            let target = step.config.get("target").map(as_str).unwrap_or("twitch");
            let message = step.config.get("message").map(as_str).unwrap_or("");
            (
                "send",
                forge_widgets::tr!("action_editor_kind_send_chat"),
                format!("\u{2192} {target}: \"{message}\""),
            )
        }
        "core.globals.set" => {
            let name = step.config.get("name").map(as_str).unwrap_or("");
            let value = step.config.get("value").map(as_str).unwrap_or("");
            (
                "variable",
                forge_widgets::tr!("action_editor_kind_set_global"),
                format!("{name} = \"{value}\""),
            )
        }
        "core.logic.wait" => {
            let ms = step.config.get("ms").map(as_i64).unwrap_or(0);
            (
                "clock",
                forge_widgets::tr!("action_editor_kind_delay"),
                format!("{ms} ms"),
            )
        }
        "core.log.write" => {
            let level = step.config.get("level").map(as_str).unwrap_or("info");
            let message = step.config.get("message").map(as_str).unwrap_or("");
            (
                "info-circle",
                forge_widgets::tr!("action_editor_kind_log"),
                format!("[{level}] \"{message}\""),
            )
        }
        "soundboard.sound.play" => {
            let clip_id = step.config.get("clip_id").map(as_str).unwrap_or("");
            (
                "music",
                forge_widgets::tr!("action_editor_kind_play_sound"),
                clip_id.to_string(),
            )
        }
        "tts.speak.text" => {
            let text = step.config.get("text").map(as_str).unwrap_or("");
            (
                "volume",
                forge_widgets::tr!("action_editor_kind_speak"),
                text.to_string(),
            )
        }
        "core.file.read" => {
            let path = step.config.get("path").map(as_str).unwrap_or("");
            let var = step.config.get("target_var").map(as_str).unwrap_or("");
            (
                "file",
                forge_widgets::tr!("action_editor_kind_read_file"),
                format!("{path} \u{2192} %{var}%"),
            )
        }
        "core.random.int" => {
            let min = step.config.get("min").map(as_i64).unwrap_or(0);
            let max = step.config.get("max").map(as_i64).unwrap_or(0);
            let var = step.config.get("target_var").map(as_str).unwrap_or("");
            (
                "dice",
                forge_widgets::tr!("action_editor_kind_random_int"),
                format!("[{min}..{max}] \u{2192} %{var}%"),
            )
        }
        _ => (
            "bolt",
            forge_widgets::tr!("action_editor_kind_sub_action"),
            step.kind_id.clone(),
        ),
    }
}

fn trigger_icon_name(category: &TriggerCategory) -> &'static str {
    match category {
        TriggerCategory::Chat => "chat",
        TriggerCategory::Subscriptions => "people",
        TriggerCategory::Bits => "bolt",
        TriggerCategory::Raids => "broadcast",
        TriggerCategory::Obs => "device-desktop",
        TriggerCategory::Server => "server",
        TriggerCategory::Timer => "clock",
        TriggerCategory::Ungrouped | TriggerCategory::All => "bolt",
    }
}

fn kind_condition_text(kind_id: &str, config: &forge_types::TriggerConfig) -> String {
    match kind_id {
        "twitch.chat.command" => forge_widgets::tr!("actions_summary_twitch_chat_command"),
        "twitch.chat.message" => forge_widgets::tr!("actions_summary_twitch_chat_message"),
        "twitch.support.subscriber" => forge_widgets::tr!("actions_summary_twitch_subscriber"),
        "twitch.support.resubscriber" => forge_widgets::tr!("actions_summary_twitch_resubscriber"),
        "twitch.support.gift_sub" => forge_widgets::tr!("actions_summary_twitch_gift_sub"),
        "twitch.support.cheer" => forge_widgets::tr!("actions_summary_twitch_cheer"),
        "twitch.channel.raid_received" => forge_widgets::tr!("actions_summary_twitch_raid"),
        "obs.scenes.current_changed" => {
            if let Some(forge_types::Variant::String(s)) = config.get("scene") {
                format!("scene = {s}")
            } else {
                forge_widgets::tr!("actions_summary_obs_scene_changed")
            }
        }
        "script.event.custom" => {
            if let Some(forge_types::Variant::String(s)) = config.get("name") {
                format!("event = {s}")
            } else {
                forge_widgets::tr!("actions_summary_server_custom_event")
            }
        }
        _ => String::new(),
    }
}

fn parse_variable_segments(s: &str) -> Vec<(&str, bool)> {
    let bytes = s.as_bytes();
    let mut segs: Vec<(&str, bool)> = Vec::new();
    let mut plain_start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let var_start = i + 1;
            let mut j = var_start;
            if j < bytes.len() && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
                j += 1;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'.')
                {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'%' && j > var_start {
                    if plain_start < i {
                        segs.push((&s[plain_start..i], false));
                    }
                    segs.push((&s[i..j + 1], true));
                    i = j + 1;
                    plain_start = i;
                    continue;
                }
            }
        }
        i += 1;
    }
    if plain_start < s.len() {
        segs.push((&s[plain_start..], false));
    }
    segs
}

fn variable_text<'a>(s: &str, palette: &ForgePalette, mono: iced::Font) -> Element<'a, Message> {
    if s.is_empty() {
        return iced::widget::text(String::new())
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(mono)
            .into();
    }
    let p = *palette;
    let segs = parse_variable_segments(s);
    let els: Vec<Element<'a, Message>> = segs
        .into_iter()
        .map(|(chunk, is_var)| {
            let color = if is_var { p.warning } else { p.text_muted };
            iced::widget::text(chunk.to_owned())
                .size(FONT_XS)
                .color(color)
                .font(mono)
                .into()
        })
        .collect();
    iced::widget::row(els).spacing(0).wrap().into()
}

fn step_icon_btn<'a>(
    icon: Icon,
    disabled: bool,
    msg: Message,
    palette: &ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let icon_color = if disabled { p.disabled } else { p.text_faint };
    let surface_overlay = p.surface_overlay;
    let icon_el = tabler_icon(icon, 12.0, icon_color);

    let content = iced::widget::container(icon_el)
        .width(20.0)
        .height(20.0)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    let mut btn = iced::widget::button(content)
        .padding([sp(Spacing::Xxs), sp(Spacing::Xxs)])
        .style(
            move |_t: &iced::Theme, status| iced::widget::button::Style {
                background: if disabled {
                    None
                } else {
                    match status {
                        iced::widget::button::Status::Hovered
                        | iced::widget::button::Status::Pressed => {
                            Some(Background::Color(surface_overlay))
                        }
                        _ => None,
                    }
                },
                text_color: icon_color,
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
        );

    if !disabled {
        btn = btn.on_press(msg);
    }

    btn.into()
}

fn step_controls<'a>(
    action_id: ActionId,
    i: usize,
    total: usize,
    menu_open: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::row;

    let p = *palette;
    let border_color = p.border_regular;

    let move_up = step_icon_btn(
        Icon::ArrowUp,
        i == 0,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::MoveSubAction(
            MoveSubActionMsg::Up(action_id, i),
        ))),
        palette,
    );
    let move_down = step_icon_btn(
        Icon::ArrowDown,
        i + 1 >= total,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::MoveSubAction(
            MoveSubActionMsg::Down(action_id, i),
        ))),
        palette,
    );

    let divider = iced::widget::container(
        iced::widget::container(iced::widget::Space::new().width(0.5).height(12.0))
            .width(0.5)
            .height(12.0)
            .style(move |_t: &iced::Theme| iced::widget::container::Style {
                background: Some(Background::Color(border_color)),
                ..iced::widget::container::Style::default()
            }),
    )
    .padding([0, sp(Spacing::Xxs)]);

    let items: Vec<MenuItem<Message>> = vec![
        MenuItem::Item {
            label: forge_widgets::tr!("action_editor_step_menu_edit"),
            on_press: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::EditRequested(action_id, i),
            ))),
            icon: Some(Icon::InfoCircle),
            shortcut: None,
            color: None,
            disabled: false,
        },
        MenuItem::Item {
            label: forge_widgets::tr!("action_editor_step_menu_duplicate"),
            on_press: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::DuplicateRequested(action_id, i),
            ))),
            icon: Some(Icon::Copy),
            shortcut: None,
            color: None,
            disabled: false,
        },
        MenuItem::Divider,
        MenuItem::Item {
            label: forge_widgets::tr!("action_editor_step_menu_move_top"),
            on_press: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::MoveSubAction(
                MoveSubActionMsg::ToTop(action_id, i),
            ))),
            icon: Some(Icon::ArrowBarUp),
            shortcut: None,
            color: None,
            disabled: i == 0,
        },
        MenuItem::Item {
            label: forge_widgets::tr!("action_editor_step_menu_move_bottom"),
            on_press: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::MoveSubAction(
                MoveSubActionMsg::ToBottom(action_id, i),
            ))),
            icon: Some(Icon::ArrowBarDown),
            shortcut: None,
            color: None,
            disabled: i + 1 >= total,
        },
        MenuItem::Divider,
        MenuItem::Item {
            label: forge_widgets::tr!("action_editor_step_menu_delete"),
            on_press: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::RemoveSubAction(
                RemoveSubActionMsg::Requested(action_id, i),
            ))),
            icon: Some(Icon::Eraser),
            shortcut: None,
            color: Some(p.random),
            disabled: false,
        },
    ];

    let menu = menu_button(
        Icon::DotsVertical,
        menu_open,
        Message::Actions(ActionsMsg::ToggleStepMenu(i)),
        Message::Actions(ActionsMsg::DismissStepMenu),
        items,
        MenuPlacement::BottomRight,
        palette,
    );

    row![move_up, move_down, divider, menu]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center)
        .into()
}

fn tree_pane<'a>(
    groups: &'a [ActionsGroup],
    selected_id: ActionId,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{column, container, row, scrollable, text};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);
    let dot_size = 6.0_f32;

    let mut col: iced::widget::Column<'_, Message> = column![].spacing(0);

    for group in groups {
        let header_row = row![
            tabler_icon(Icon::ChevronDown, 11.0, p.text_muted),
            text(group.category.display_name())
                .size(FONT_XS)
                .color(p.text_muted)
                .font(mono),
            iced::widget::Space::new().width(Length::Fill),
            text(group.actions.len().to_string())
                .size(FONT_XS)
                .color(p.text_faint)
                .font(mono),
        ]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center)
        .padding([sp(Spacing::Xs), sp(Spacing::Sm)]);

        col = col.push(container(header_row).width(Length::Fill));

        for summary in &group.actions {
            let is_selected = summary.id == selected_id;
            let dot_color = if summary.enabled {
                p.success
            } else {
                p.text_faint
            };
            let name_color = if is_selected {
                p.text_primary
            } else if summary.enabled {
                p.text_secondary
            } else {
                p.text_faint
            };

            let dot = container(iced::widget::Space::new().width(dot_size).height(dot_size))
                .width(dot_size)
                .height(dot_size)
                .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(Background::Color(dot_color)),
                    border: Border {
                        radius: (dot_size / 2.0).into(),
                        ..Border::default()
                    },
                    ..iced::widget::container::Style::default()
                });

            let sub_count = summary.sub_action_count;
            let sub_count_lbl =
                forge_widgets::tr!("action_editor_sub_count", count = sub_count as i64);
            let inner = row![
                dot,
                text(summary.name.clone()).size(FONT_SM).color(name_color),
                iced::widget::Space::new().width(Length::Fill),
                text(sub_count_lbl)
                    .size(FONT_XS)
                    .color(p.text_faint)
                    .font(mono),
            ]
            .spacing(spf(Spacing::Xs))
            .align_y(Alignment::Center);

            let action_id = summary.id;
            let row_btn =
                iced::widget::button(container(inner).width(Length::Fill).padding(Padding {
                    top: 6.0,
                    bottom: 6.0,
                    left: 32.0,
                    right: 14.0,
                }))
                .on_press(Message::Navigate(Screen::ActionEditor(action_id)))
                .padding(0)
                .width(Length::Fill)
                .style(move |_theme: &iced::Theme, _status| {
                    let left_border_color = if is_selected {
                        p.brand
                    } else {
                        iced::Color::TRANSPARENT
                    };
                    let bg = if is_selected {
                        p.surface_overlay
                    } else {
                        iced::Color::TRANSPARENT
                    };
                    iced::widget::button::Style {
                        background: Some(Background::Color(bg)),
                        text_color: name_color,
                        border: Border {
                            color: left_border_color,
                            width: 2.0,
                            radius: 0.0.into(),
                        },
                        shadow: iced::Shadow::default(),
                        snap: false,
                    }
                });

            col = col.push(row_btn);
        }
    }

    let scroll = scrollable(col).height(Length::Fill);

    container(scroll)
        .width(Length::Fixed(280.0))
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(Background::Color(p.shell)),
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn detail_pane<'a>(
    app: &'a App,
    action_id: ActionId,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{column, container, row, scrollable, text};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);

    let detail = match app.ui.actions.detail.as_ref() {
        Some(d) if d.action.id == action_id => d,
        _ => {
            return container(
                text(forge_widgets::tr!("action_editor_loading"))
                    .size(FONT_SM)
                    .color(p.text_muted),
            )
            .padding([sp(Spacing::Md), sp(Spacing::Lg)])
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        }
    };

    let action = &detail.action;

    let pill_variant = if action.enabled {
        forge_widgets::StatusVariant::Positive
    } else {
        forge_widgets::StatusVariant::Negative
    };
    let pill_label = if action.enabled {
        forge_widgets::tr!("action_editor_enabled")
    } else {
        forge_widgets::tr!("action_editor_disabled")
    };
    let pill = forge_widgets::status_pill(pill_label, pill_variant, palette);

    let title_row = row![
        text(action.name.clone())
            .size(FONT_LG)
            .color(p.text_primary),
        pill,
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let desc_text: String = action
        .description
        .clone()
        .unwrap_or_else(|| forge_widgets::tr!("action_editor_no_description"));
    let desc = text(desc_text).size(FONT_XS).color(p.text_muted);

    let test_run_btn = forge_widgets::secondary_button(
        forge_widgets::tr!("action_editor_test_run"),
        Message::Actions(ActionsMsg::TestTrigger(action_id)),
        palette,
    );
    let dup_btn = forge_widgets::secondary_button(
        forge_widgets::tr!("action_editor_duplicate"),
        Message::Actions(ActionsMsg::DuplicateAction(action_id)),
        palette,
    );

    let btn_row = row![test_run_btn, dup_btn].spacing(spf(Spacing::Xs));

    let header_left: Element<'_, Message> =
        column![title_row, desc].spacing(spf(Spacing::Xxs)).into();
    let header_row: Element<'_, Message> = row![
        header_left,
        iced::widget::Space::new().width(Length::Fill),
        btn_row,
    ]
    .align_y(Alignment::Start)
    .into();

    let triggers_label = text(forge_widgets::tr!("action_editor_section_triggers"))
        .size(FONT_XS)
        .color(p.text_muted)
        .font(mono);

    let add_trigger_lbl = forge_widgets::tr!("action_editor_add_trigger");
    let add_trigger_btn = iced::widget::button(
        row![
            tabler_icon(Icon::Plus, 11.0, p.brand),
            text(add_trigger_lbl).size(FONT_XS).color(p.brand),
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center),
    )
    .on_press(Message::Actions(ActionsMsg::OpenTriggerPicker(action_id)))
    .padding(0)
    .style(
        |_theme: &iced::Theme, _status| iced::widget::button::Style {
            background: None,
            text_color: iced::Color::TRANSPARENT,
            border: Border::default(),
            shadow: iced::Shadow::default(),
            snap: false,
        },
    );

    let triggers_header: Element<'_, Message> = row![
        triggers_label,
        iced::widget::Space::new().width(Length::Fill),
        add_trigger_btn,
    ]
    .align_y(Alignment::Center)
    .into();

    let mut triggers_col: iced::widget::Column<'_, Message> = column![].spacing(spf(Spacing::Xs));
    if detail.trigger_instances.is_empty() {
        triggers_col = triggers_col.push(
            container(
                text(forge_widgets::tr!("action_editor_no_triggers"))
                    .size(FONT_XS)
                    .color(p.text_faint),
            )
            .padding([sp(Spacing::Xs), 0]),
        );
    } else {
        for instance in &detail.trigger_instances {
            let cat = category_of(&instance.kind_id);
            let icon_name = trigger_icon_name(&cat);
            let icon_box = container(tabler_icon(Icon::from_name(icon_name), 14.0, p.brand))
                .width(26.0)
                .height(26.0)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(Background::Color(p.surface_overlay)),
                    border: Border {
                        radius: 6.0.into(),
                        ..Border::default()
                    },
                    ..iced::widget::container::Style::default()
                });

            let kind_label = trigger_label_of(&instance.kind_id);
            let condition_str = kind_condition_text(&instance.kind_id, &instance.overrides);
            let (pill_label, pill_variant) = if instance.user_defined {
                ("Custom", StatusVariant::Positive)
            } else {
                ("Default", StatusVariant::Neutral)
            };
            let pill: Element<'_, Message> = status_pill(pill_label, pill_variant, palette);
            let name_row: Element<'_, Message> = row![
                text(instance.name.as_str())
                    .size(FONT_SM)
                    .color(p.text_primary),
                pill,
            ]
            .spacing(spf(Spacing::Xs))
            .align_y(Alignment::Center)
            .into();
            let secondary_str = if condition_str.is_empty() {
                kind_label.to_owned()
            } else {
                format!("{kind_label} \u{00b7} {condition_str}")
            };
            let info_col: Element<'_, Message> = column![
                name_row,
                text(secondary_str)
                    .size(FONT_XS)
                    .color(p.text_muted)
                    .font(mono),
            ]
            .spacing(spf(Spacing::Xxs))
            .into();

            let instance_id = instance.id;
            let action_id_local = action_id;
            let p_btn = p;
            let p_nav = p;
            let delete_lbl = forge_widgets::tr!("action_editor_delete");
            let delete_btn = iced::widget::button(
                text(delete_lbl)
                    .size(FONT_XS)
                    .color(p.random)
                    .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
            )
            .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
            .on_press(Message::Actions(ActionsMsg::RemoveTriggerInstance(
                action_id_local,
                instance_id,
            )))
            .style(move |_t, status| iced::widget::button::Style {
                background: if matches!(status, iced::widget::button::Status::Hovered) {
                    Some(Background::Color(iced::Color {
                        a: 0.08,
                        ..p_btn.random
                    }))
                } else {
                    None
                },
                text_color: p_btn.random,
                border: Border {
                    color: iced::Color::TRANSPARENT,
                    width: 0.0,
                    radius: 4.0.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            });

            let nav_btn: Element<'_, Message> = iced::widget::button(
                row![icon_box, info_col]
                    .spacing(spf(Spacing::Xs))
                    .align_y(Alignment::Center),
            )
            .on_press(Message::Actions(ActionsMsg::TriggerChipClicked(
                instance_id,
            )))
            .padding(0)
            .style(move |_: &iced::Theme, status| iced::widget::button::Style {
                background: if matches!(status, iced::widget::button::Status::Hovered) {
                    Some(Background::Color(iced::Color {
                        a: 0.06,
                        ..p_nav.brand
                    }))
                } else {
                    None
                },
                text_color: iced::Color::TRANSPARENT,
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            })
            .width(Length::Fill)
            .into();

            let dots = tabler_icon(Icon::DotsVertical, 14.0, p.text_faint);

            let trigger_row: Element<'_, Message> = row![nav_btn, delete_btn, dots]
                .spacing(spf(Spacing::Xs))
                .align_y(Alignment::Center)
                .into();

            let trigger_card = container(trigger_row)
                .width(Length::Fill)
                .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
                .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(Background::Color(p.elevated)),
                    border: Border {
                        color: p.border_regular,
                        width: 0.5,
                        radius: 8.0.into(),
                    },
                    ..iced::widget::container::Style::default()
                });

            triggers_col = triggers_col.push(trigger_card);
        }
    }

    let triggers_section: Element<'_, Message> = column![triggers_header, triggers_col]
        .spacing(spf(Spacing::Xs))
        .into();

    let sub_count = action.sub_actions.len();
    let sub_hdr_str = forge_widgets::tr!(
        "action_editor_section_sub_actions",
        count = sub_count as i64
    );
    let sub_label = text(sub_hdr_str)
        .size(FONT_XS)
        .color(p.text_muted)
        .font(mono);

    let add_step_lbl = forge_widgets::tr!("action_editor_add_step");
    let add_step_btn = iced::widget::button(
        row![
            tabler_icon(Icon::Plus, 11.0, p.brand),
            text(add_step_lbl).size(FONT_XS).color(p.brand),
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center),
    )
    .on_press(Message::Actions(ActionsMsg::Editor(
        ActionEditorMsg::AddSubAction(AddSubActionMsg::OpenRequested(action_id)),
    )))
    .padding(0)
    .style(
        |_theme: &iced::Theme, _status| iced::widget::button::Style {
            background: None,
            text_color: iced::Color::TRANSPARENT,
            border: Border::default(),
            shadow: iced::Shadow::default(),
            snap: false,
        },
    );

    let sub_header: Element<'_, Message> = row![
        sub_label,
        iced::widget::Space::new().width(Length::Fill),
        add_step_btn,
    ]
    .align_y(Alignment::Center)
    .into();

    let total = action.sub_actions.len();
    let mut steps_col: iced::widget::Column<'_, Message> = column![].spacing(0);

    for (i, step) in action.sub_actions.iter().enumerate() {
        let step_num = i + 1;
        let is_last = step_num == total;
        let (icon_name, title, details) = sub_action_summary(step);
        let step_icon = Icon::from_name(icon_name);
        let avg_ms_label = detail
            .sub_action_avg_ms
            .get(i)
            .and_then(|v| *v)
            .map(|ms| format!("{ms} ms avg"));

        let circle_label = text(step_num.to_string())
            .size(FONT_XS)
            .color(p.shell)
            .font(forge_widgets::font(forge_widgets::FontRole::Monospace));

        let circle = container(circle_label)
            .width(22.0)
            .height(22.0)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                background: Some(Background::Color(p.brand)),
                border: Border {
                    radius: 11.0.into(),
                    ..Border::default()
                },
                ..iced::widget::container::Style::default()
            });

        let connector_height = if is_last { 0.0 } else { 14.0 };
        let connector = container(
            iced::widget::Space::new()
                .width(2.0)
                .height(connector_height),
        )
        .width(2.0)
        .height(connector_height)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(Background::Color(p.border_regular)),
            ..iced::widget::container::Style::default()
        });

        let left_col: Element<'_, Message> = column![circle, connector]
            .align_x(Alignment::Center)
            .width(24.0)
            .into();

        let icon_el = tabler_icon(step_icon, 13.0, p.text_secondary);
        let title_el = text(title).size(FONT_SM).color(p.text_primary);

        let timing_el: Element<'_, Message> = match avg_ms_label {
            Some(label) => text(label)
                .size(FONT_XS)
                .color(p.text_faint)
                .font(forge_widgets::font(forge_widgets::FontRole::Monospace))
                .into(),
            None => iced::widget::Space::new().width(Length::Shrink).into(),
        };

        let menu_open = app.ui.actions.step_menu_open == Some(i);
        let controls = step_controls(action_id, i, total, menu_open, palette);

        let title_row: Element<'_, Message> = row![
            icon_el,
            title_el,
            iced::widget::Space::new().width(Length::Fill),
            timing_el,
            controls,
        ]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center)
        .into();

        let details_el = variable_text(&details, palette, mono);

        let card_inner: Element<'_, Message> = column![title_row, details_el]
            .spacing(spf(Spacing::Xxs))
            .into();

        let card = container(card_inner)
            .width(Length::Fill)
            .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
            .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                background: Some(Background::Color(p.elevated)),
                border: Border {
                    color: p.border_regular,
                    width: 0.5,
                    radius: 8.0.into(),
                },
                ..iced::widget::container::Style::default()
            });

        let step_row: Element<'_, Message> = row![left_col, card]
            .spacing(spf(Spacing::Xs))
            .align_y(Alignment::Start)
            .into();

        let bottom_pad = if is_last { 0.0 } else { 6.0 };
        let step_wrapper = container(step_row).width(Length::Fill).padding(Padding {
            bottom: bottom_pad,
            ..Padding::ZERO
        });

        steps_col = steps_col.push(step_wrapper);
    }

    let sub_section: Element<'_, Message> = column![sub_header, steps_col]
        .spacing(spf(Spacing::Xs))
        .into();

    let body: Element<'_, Message> = column![header_row, triggers_section, sub_section]
        .spacing(spf(Spacing::Md))
        .into();

    container(scrollable(body))
        .padding([18_u16, 22_u16])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

pub fn action_editor_view<'a>(
    app: &'a App,
    action_id: ActionId,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let left = tree_pane(&app.ui.actions.tree, action_id, palette);
    let right = detail_pane(app, action_id, palette);
    let action_name = app
        .ui
        .actions
        .tree
        .iter()
        .flat_map(|g| g.actions.iter())
        .find(|a| a.id == action_id)
        .map(|a| a.name.as_str())
        .unwrap_or("Action");
    let page_header = crate::page_chrome::simple_page_header(
        &[
            (
                forge_widgets::tr!("action_editor_breadcrumb_automation"),
                false,
            ),
            (
                forge_widgets::tr!("action_editor_breadcrumb_actions"),
                false,
            ),
            (action_name.to_owned(), true),
        ],
        palette,
    );

    let base: Element<'_, Message> = iced::widget::column![
        page_header,
        iced::widget::row![left, right]
            .spacing(0)
            .height(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into();

    if let Some(picker_state) = app.ui.actions.trigger_picker.as_ref() {
        let picker_el = crate::actions_trigger_picker::view(picker_state, &app.rt, palette);
        iced::widget::stack![base, picker_el].into()
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::parse_variable_segments;

    #[test]
    fn plain_text_produces_single_non_var_segment() {
        let segs = parse_variable_segments("hello world");
        assert_eq!(segs, vec![("hello world", false)]);
    }

    #[test]
    fn single_variable_is_parsed() {
        let segs = parse_variable_segments("%user%");
        assert_eq!(segs, vec![("%user%", true)]);
    }

    #[test]
    fn variable_at_end_of_string() {
        let segs = parse_variable_segments("name = %counter%");
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0], ("name = ", false));
        assert_eq!(segs[1], ("%counter%", true));
    }

    #[test]
    fn variable_in_middle_produces_three_segments() {
        let segs = parse_variable_segments("hello %user% world");
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0], ("hello ", false));
        assert_eq!(segs[1], ("%user%", true));
        assert_eq!(segs[2], (" world", false));
    }

    #[test]
    fn dotted_variable_name_is_recognised() {
        let segs = parse_variable_segments("%forge.counter%");
        assert_eq!(segs, vec![("%forge.counter%", true)]);
    }

    #[test]
    fn variable_name_starting_with_digit_is_plain() {
        let segs = parse_variable_segments("%1bad%");
        assert_eq!(segs, vec![("%1bad%", false)]);
    }

    #[test]
    fn empty_percent_pair_is_plain() {
        let segs = parse_variable_segments("%%");
        assert_eq!(segs, vec![("%%", false)]);
    }

    #[test]
    fn arrow_and_variable() {
        let segs = parse_variable_segments("~/quotes.txt \u{2192} %lines%");
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0], ("~/quotes.txt \u{2192} ", false));
        assert_eq!(segs[1], ("%lines%", true));
    }

    #[test]
    fn empty_string_yields_empty_segments() {
        let segs = parse_variable_segments("");
        assert!(segs.is_empty());
    }

    #[test]
    fn multiple_variables_parsed() {
        let segs = parse_variable_segments("%a% and %b%");
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0], ("%a%", true));
        assert_eq!(segs[1], (" and ", false));
        assert_eq!(segs[2], ("%b%", true));
    }
}
