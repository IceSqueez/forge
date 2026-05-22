use forge_types::{ActionId, SubActionSpec};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use crate::Screen;
use crate::actions::AddSubActionMsg;
use crate::actions::RemoveSubActionMsg;
use crate::actions::{ActionsGroup, TriggerCategory, trigger_label_of};
use crate::app::App;
use crate::message::{ActionsMsg, Message, MoveSubActionMsg};
use forge_widgets::ForgePalette;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::popover::{MenuItem, MenuPlacement, menu_button};
use forge_widgets::tokens::{FONT_LG, FONT_SM, FONT_XS};

fn sub_action_summary(spec: &SubActionSpec) -> (&'static str, &'static str, String) {
    match spec {
        SubActionSpec::SendChat { target, message } => (
            "send",
            "Send chat message",
            format!("\u{2192} {target}: \"{message}\""),
        ),
        SubActionSpec::SetGlobal { name, value } => {
            ("variable", "Set global", format!("{name} = {value:?}"))
        }
        SubActionSpec::IncrementGlobal { name, amount } => (
            "variable",
            "Increment global",
            format!("{name} += {amount}"),
        ),
        SubActionSpec::GetGlobal { name, into_arg } => (
            "variable",
            "Get global",
            format!("{name} \u{2192} %{into_arg}%"),
        ),
        SubActionSpec::DeleteGlobal { name } => {
            ("variable", "Delete global", format!("delete {name}"))
        }
        SubActionSpec::Delay { ms } => ("clock", "Delay", format!("{ms} ms")),
        SubActionSpec::Log { level, message } => {
            ("info-circle", "Log", format!("[{level:?}] {message:?}"))
        }
        SubActionSpec::RunScript { script_name } => {
            ("file-code", "Run script", script_name.to_string())
        }
        other => ("bolt", other.kind_label(), format!("{other:?}")),
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

fn kind_condition_text(kind: &forge_types::TriggerKind) -> String {
    use forge_types::TriggerKind;
    match kind {
        TriggerKind::TwitchChatCommand => "any command match".to_string(),
        TriggerKind::TwitchChatAnyMessage => "every chat message".to_string(),
        TriggerKind::TwitchSubscribe => "new subscriber".to_string(),
        TriggerKind::TwitchResubscribe => "re-subscribe".to_string(),
        TriggerKind::TwitchGiftSub => "gift subs".to_string(),
        TriggerKind::TwitchCheer => "bits cheered".to_string(),
        TriggerKind::TwitchRaid => "raid received".to_string(),
        TriggerKind::ObsSceneChanged { scene: Some(s) } => format!("scene = {s}"),
        TriggerKind::ObsSceneChanged { scene: None } => "any scene".to_string(),
        TriggerKind::CodeEvent { name } => format!("event = {name}"),
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
        .padding(Padding::from([2u16, 4u16]))
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
        Message::MoveSubAction(MoveSubActionMsg::Up(action_id, i)),
        palette,
    );
    let move_down = step_icon_btn(
        Icon::ArrowDown,
        i + 1 >= total,
        Message::MoveSubAction(MoveSubActionMsg::Down(action_id, i)),
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
    .padding(Padding::from([0u16, 4u16]));

    let items: Vec<MenuItem<Message>> = vec![
        MenuItem::Item {
            label: "Edit step\u{2026}".to_string(),
            on_press: Message::Noop,
            icon: Some(Icon::InfoCircle),
            shortcut: None,
            color: None,
            disabled: true,
        },
        MenuItem::Item {
            label: "Duplicate".to_string(),
            on_press: Message::AddSubAction(AddSubActionMsg::DuplicateRequested(action_id, i)),
            icon: Some(Icon::Copy),
            shortcut: None,
            color: None,
            disabled: false,
        },
        MenuItem::Divider,
        MenuItem::Item {
            label: "Move to top".to_string(),
            on_press: Message::MoveSubAction(MoveSubActionMsg::ToTop(action_id, i)),
            icon: Some(Icon::ArrowBarUp),
            shortcut: None,
            color: None,
            disabled: i == 0,
        },
        MenuItem::Item {
            label: "Move to bottom".to_string(),
            on_press: Message::MoveSubAction(MoveSubActionMsg::ToBottom(action_id, i)),
            icon: Some(Icon::ArrowBarDown),
            shortcut: None,
            color: None,
            disabled: i + 1 >= total,
        },
        MenuItem::Divider,
        MenuItem::Item {
            label: "Delete step".to_string(),
            on_press: Message::RemoveSubAction(RemoveSubActionMsg::Requested(action_id, i)),
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
        .spacing(2)
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
        .spacing(8)
        .align_y(Alignment::Center)
        .padding([6_u16, 14_u16]);

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
            let inner = row![
                dot,
                text(summary.name.clone()).size(FONT_SM).color(name_color),
                iced::widget::Space::new().width(Length::Fill),
                text(format!("{sub_count} sub"))
                    .size(FONT_XS)
                    .color(p.text_faint)
                    .font(mono),
            ]
            .spacing(10)
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

    let detail = match app.actions.detail.as_ref() {
        Some(d) if d.action.id == action_id => d,
        _ => {
            return container(
                text("Loading action\u{2026}")
                    .size(FONT_SM)
                    .color(p.text_muted),
            )
            .padding([18_u16, 22_u16])
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        }
    };

    let action = &detail.action;

    // ── Header ────────────────────────────────────────────────────────────
    let pill_variant = if action.enabled {
        forge_widgets::StatusVariant::Positive
    } else {
        forge_widgets::StatusVariant::Negative
    };
    let pill_label = if action.enabled {
        "Enabled"
    } else {
        "Disabled"
    };
    let pill = forge_widgets::status_pill(pill_label, pill_variant, palette);

    let title_row = row![
        text(action.name.clone())
            .size(FONT_LG)
            .color(p.text_primary),
        pill,
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let desc_text = action.description.as_deref().unwrap_or("No description");
    let desc = text(desc_text).size(FONT_XS).color(p.text_muted);

    let test_run_btn = forge_widgets::secondary_button(
        "Test run",
        Message::Actions(ActionsMsg::TestTrigger(action_id)),
        palette,
    );
    let dup_btn = forge_widgets::secondary_button(
        "Duplicate",
        Message::Actions(ActionsMsg::DuplicateAction(action_id)),
        palette,
    );

    let btn_row = row![test_run_btn, dup_btn].spacing(6);

    let header_left: Element<'_, Message> = column![title_row, desc].spacing(4).into();
    let header_row: Element<'_, Message> = row![
        header_left,
        iced::widget::Space::new().width(Length::Fill),
        btn_row,
    ]
    .align_y(Alignment::Start)
    .into();

    // ── Triggers ─────────────────────────────────────────────────────────
    let triggers_label = text("TRIGGERS")
        .size(FONT_XS)
        .color(p.text_muted)
        .font(mono);

    let add_trigger_btn = iced::widget::button(
        row![
            tabler_icon(Icon::Plus, 11.0, p.brand),
            text("Add trigger").size(FONT_XS).color(p.brand),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .on_press(Message::Actions(ActionsMsg::OpenAddTriggerModal(action_id)))
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

    let mut triggers_col: iced::widget::Column<'_, Message> = column![].spacing(6);
    if detail.triggers.is_empty() {
        triggers_col = triggers_col.push(
            container(
                text("No triggers \u{00b7} click Add trigger to start")
                    .size(FONT_XS)
                    .color(p.text_faint),
            )
            .padding([8_u16, 0_u16]),
        );
    } else {
        for trigger in &detail.triggers {
            let cat = crate::actions::category_of(&trigger.kind);
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

            let label_str = trigger_label_of(&trigger.kind);
            let condition_str = kind_condition_text(&trigger.kind);

            let info_col: Element<'_, Message> = column![
                text(label_str).size(FONT_SM).color(p.text_primary),
                text(condition_str)
                    .size(FONT_XS)
                    .color(p.text_muted)
                    .font(mono),
            ]
            .spacing(1)
            .into();

            let trigger_id = trigger.id;
            let action_id_local = action_id;
            let p_btn = p;
            let delete_btn = iced::widget::button(
                text("Delete")
                    .size(FONT_XS)
                    .color(p.random)
                    .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
            )
            .padding([2u16, 8u16])
            .on_press(Message::Actions(ActionsMsg::DeleteTrigger(
                trigger_id,
                action_id_local,
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

            let dots = tabler_icon(Icon::DotsVertical, 14.0, p.text_faint);

            let trigger_row: Element<'_, Message> = row![icon_box, info_col, delete_btn, dots]
                .spacing(10)
                .align_y(Alignment::Center)
                .into();

            let trigger_card = container(trigger_row)
                .width(Length::Fill)
                .padding([10_u16, 12_u16])
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

    let triggers_section: Element<'_, Message> =
        column![triggers_header, triggers_col].spacing(8).into();

    // ── Sub-actions ───────────────────────────────────────────────────────
    let sub_count = action.sub_actions.len();
    let sub_label = text(format!("SUB-ACTIONS \u{00b7} {sub_count}"))
        .size(FONT_XS)
        .color(p.text_muted)
        .font(mono);

    let add_step_btn = iced::widget::button(
        row![
            tabler_icon(Icon::Plus, 11.0, p.brand),
            text("Add step").size(FONT_XS).color(p.brand),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .on_press(Message::AddSubAction(AddSubActionMsg::OpenRequested(
        action_id,
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

    for (i, spec) in action.sub_actions.iter().enumerate() {
        let step_num = i + 1;
        let is_last = step_num == total;
        let (icon_name, title, details) = sub_action_summary(spec);
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

        let menu_open = app.actions.step_menu_open == Some(i);
        let controls = step_controls(action_id, i, total, menu_open, palette);

        let title_row: Element<'_, Message> = row![
            icon_el,
            title_el,
            iced::widget::Space::new().width(Length::Fill),
            timing_el,
            controls,
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into();

        let details_el = variable_text(&details, palette, mono);

        let card_inner: Element<'_, Message> = column![title_row, details_el].spacing(3).into();

        let card = container(card_inner)
            .width(Length::Fill)
            .padding([10_u16, 12_u16])
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
            .spacing(10)
            .align_y(Alignment::Start)
            .into();

        let bottom_pad = if is_last { 0.0 } else { 6.0 };
        let step_wrapper = container(step_row).width(Length::Fill).padding(Padding {
            bottom: bottom_pad,
            ..Padding::ZERO
        });

        steps_col = steps_col.push(step_wrapper);
    }

    let sub_section: Element<'_, Message> = column![sub_header, steps_col].spacing(10).into();

    let body: Element<'_, Message> = column![header_row, triggers_section, sub_section]
        .spacing(18)
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
    let left = tree_pane(&app.actions.tree, action_id, palette);
    let right = detail_pane(app, action_id, palette);

    iced::widget::row![left, right]
        .spacing(0)
        .height(Length::Fill)
        .into()
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
