use forge_types::{ActionId, SubActionSpec};
use iced::{Alignment, Element, Length, Padding};

use crate::Screen;
use crate::actions::AddSubActionMsg;
use crate::actions::{ActionsGroup, TriggerCategory, trigger_label_of};
use crate::app::App;
use crate::message::{ActionsMsg, Message};
use forge_widgets::ForgePalette;
use forge_widgets::icons::{BOOTSTRAP_FONT, ICON_CHEVRON_DOWN, ICON_DOTS_VERTICAL, ICON_PLUS};

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
            text(ICON_CHEVRON_DOWN.to_string())
                .size(11.0)
                .color(p.text_muted)
                .font(BOOTSTRAP_FONT),
            text(group.category.display_name())
                .size(11.0)
                .color(p.text_muted)
                .font(mono),
            iced::widget::Space::new().width(Length::Fill),
            text(group.actions.len().to_string())
                .size(11.0)
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
                    background: Some(iced::Background::Color(dot_color)),
                    border: iced::Border {
                        radius: (dot_size / 2.0).into(),
                        ..iced::Border::default()
                    },
                    ..iced::widget::container::Style::default()
                });

            let sub_count = summary.sub_action_count;
            let inner = row![
                dot,
                text(summary.name.clone()).size(12.5).color(name_color),
                iced::widget::Space::new().width(Length::Fill),
                text(format!("{sub_count} sub"))
                    .size(10.0)
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
                        background: Some(iced::Background::Color(bg)),
                        text_color: name_color,
                        border: iced::Border {
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
            background: Some(iced::Background::Color(p.shell)),
            border: iced::Border {
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
                    .size(13.0)
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
        text(action.name.clone()).size(18.0).color(p.text_primary),
        pill,
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let desc_text = action.description.as_deref().unwrap_or("No description");
    let desc = text(desc_text).size(12.0).color(p.text_muted);

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
    let triggers_label = text("TRIGGERS").size(11.0).color(p.text_muted).font(mono);

    let add_trigger_btn = iced::widget::button(
        text(format!("{} Add trigger", ICON_PLUS))
            .size(11.0)
            .color(p.brand)
            .font(BOOTSTRAP_FONT),
    )
    .on_press(Message::Actions(ActionsMsg::OpenAddTriggerModal(action_id)))
    .padding(0)
    .style(
        |_theme: &iced::Theme, _status| iced::widget::button::Style {
            background: None,
            text_color: iced::Color::TRANSPARENT,
            border: iced::Border::default(),
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
                    .size(12.0)
                    .color(p.text_faint),
            )
            .padding([8_u16, 0_u16]),
        );
    } else {
        for trigger in &detail.triggers {
            let cat = crate::actions::category_of(&trigger.kind);
            let icon_name = trigger_icon_name(&cat);
            let icon_char = forge_widgets::icons::bootstrap_icon_for(icon_name);
            let icon_box = container(
                text(icon_char.to_string())
                    .size(14.0)
                    .color(p.brand)
                    .font(BOOTSTRAP_FONT),
            )
            .width(26.0)
            .height(26.0)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                background: Some(iced::Background::Color(p.surface_overlay)),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..iced::Border::default()
                },
                ..iced::widget::container::Style::default()
            });

            let label_str = trigger_label_of(&trigger.kind);
            let condition_str = kind_condition_text(&trigger.kind);

            let info_col: Element<'_, Message> = column![
                text(label_str).size(12.5).color(p.text_primary),
                text(condition_str)
                    .size(11.0)
                    .color(p.text_muted)
                    .font(mono),
            ]
            .spacing(1)
            .into();

            let dots = text(ICON_DOTS_VERTICAL.to_string())
                .size(14.0)
                .color(p.text_faint)
                .font(BOOTSTRAP_FONT);

            let trigger_row: Element<'_, Message> = row![icon_box, info_col, dots]
                .spacing(10)
                .align_y(Alignment::Center)
                .into();

            let trigger_card = container(trigger_row)
                .width(Length::Fill)
                .padding([10_u16, 12_u16])
                .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(p.elevated)),
                    border: iced::Border {
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
        .size(11.0)
        .color(p.text_muted)
        .font(mono);

    let add_step_btn = iced::widget::button(
        text(format!("{} Add step", ICON_PLUS))
            .size(11.0)
            .color(p.brand)
            .font(BOOTSTRAP_FONT),
    )
    .on_press(Message::AddSubAction(AddSubActionMsg::OpenRequested(
        action_id,
    )))
    .padding(0)
    .style(
        |_theme: &iced::Theme, _status| iced::widget::button::Style {
            background: None,
            text_color: iced::Color::TRANSPARENT,
            border: iced::Border::default(),
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
        let icon_char = forge_widgets::icons::bootstrap_icon_for(icon_name);

        let circle_label = text(step_num.to_string())
            .size(11.0)
            .color(p.shell)
            .font(forge_widgets::font(forge_widgets::FontRole::Monospace));

        let circle = container(circle_label)
            .width(22.0)
            .height(22.0)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                background: Some(iced::Background::Color(p.brand)),
                border: iced::Border {
                    radius: 11.0.into(),
                    ..iced::Border::default()
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
            background: Some(iced::Background::Color(p.border_regular)),
            ..iced::widget::container::Style::default()
        });

        let left_col: Element<'_, Message> = column![circle, connector]
            .align_x(Alignment::Center)
            .width(24.0)
            .into();

        let icon_el = text(icon_char.to_string())
            .size(13.0)
            .color(p.text_secondary)
            .font(BOOTSTRAP_FONT);

        let title_el = text(title).size(12.5).color(p.text_primary);

        let title_row: Element<'_, Message> = row![icon_el, title_el]
            .spacing(8)
            .align_y(Alignment::Center)
            .into();

        let details_el = text(details).size(11.0).color(p.text_muted).font(mono);

        let card_inner: Element<'_, Message> = column![title_row, details_el].spacing(3).into();

        let card = container(card_inner)
            .width(Length::Fill)
            .padding([10_u16, 12_u16])
            .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                background: Some(iced::Background::Color(p.elevated)),
                border: iced::Border {
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
