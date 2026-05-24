use forge_storage::ActionTelemetry;
use forge_widgets::ForgePalette;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_LG, FONT_SM, FONT_XS, Spacing, spf};
use iced::{Element, Length};

use crate::App;
use crate::actions::{ActionsFilter, AddSubActionMsg};
use crate::message::{ActionEditorMsg, ActionsMsg, Message};

pub(crate) fn actions_view<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, row, scrollable, text};

    let p = *palette;
    let actions_state = &app.ui.actions;

    let total = actions_state.total_actions();
    let visible = actions_state.visible_actions();

    let page_header = actions_page_header(actions_state, palette);

    let mut tree_col: iced::widget::Column<'_, Message> = column![].spacing(0);

    if actions_state.loading {
        tree_col = tree_col.push(
            container(text("Loading...").size(FONT_XS).color(p.text_muted))
                .padding([16, 14])
                .width(Length::Fill),
        );
    } else if total == 0 {
        tree_col = tree_col.push(
            container(text("No actions yet").size(FONT_XS).color(p.text_faint))
                .padding([16, 14])
                .width(Length::Fill),
        );
    } else {
        for group in &actions_state.tree {
            let filtered: Vec<_> = group
                .actions
                .iter()
                .filter(|a| actions_state.action_passes_filter(a))
                .collect();

            if filtered.is_empty() {
                continue;
            }

            let is_collapsed = actions_state.collapsed_groups.contains(&group.category);
            tree_col = tree_col.push(actions_group_header(group, is_collapsed, palette));

            if !is_collapsed {
                for summary in &filtered {
                    let selected = actions_state.selected == Some(summary.id);
                    let menu_open = actions_state.action_menu_open == Some(summary.id);
                    let rename_buf = actions_state
                        .renaming_action
                        .as_ref()
                        .filter(|(id, _)| *id == summary.id)
                        .map(|(_, n)| n.as_str());
                    tree_col = tree_col.push(actions_tree_row(
                        summary, selected, menu_open, rename_buf, palette,
                    ));
                }
            }
        }
    }

    let tree_scrollable = scrollable(tree_col).height(Length::Fill);

    let left_panel = container(tree_scrollable)
        .width(Length::Fixed(290.0))
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.shell)),
            border: iced::Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        });

    let right_panel = actions_detail_panel(actions_state, palette);

    let footer = actions_footer(visible, total, palette);

    let body = row![left_panel, right_panel]
        .spacing(0)
        .height(Length::Fill);

    let body_and_footer: Element<'_, Message> = container(column![body, footer].spacing(0))
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    let main_view: Element<'_, Message> = if let Some(open_id) = actions_state.action_menu_open {
        let open_summary = actions_state
            .tree
            .iter()
            .flat_map(|g| g.actions.iter())
            .find(|a| a.id == open_id);
        let menu_top_offset = if open_summary.is_some() {
            compute_action_menu_y_offset(actions_state, open_id)
        } else {
            None
        };
        if let (Some(summary), Some(top_y)) = (open_summary, menu_top_offset) {
            let toggle_label = if summary.enabled { "Disable" } else { "Enable" };
            let menu_items: Vec<forge_widgets::MenuItem<Message>> = vec![
                forge_widgets::MenuItem::Item {
                    label: "Rename\u{2026}".into(),
                    icon: Some(Icon::InfoCircle),
                    on_press: Message::Actions(ActionsMsg::RenameStarted(open_id)),
                    shortcut: None,
                    color: None,
                    disabled: false,
                },
                forge_widgets::MenuItem::Item {
                    label: "Duplicate".into(),
                    icon: Some(Icon::Copy),
                    on_press: Message::Actions(ActionsMsg::DuplicateAction(open_id)),
                    shortcut: None,
                    color: None,
                    disabled: false,
                },
                forge_widgets::MenuItem::Item {
                    label: toggle_label.to_owned(),
                    icon: Some(Icon::Bolt),
                    on_press: Message::Actions(ActionsMsg::ToggleEnabled(
                        open_id,
                        !summary.enabled,
                    )),
                    shortcut: None,
                    color: None,
                    disabled: false,
                },
                forge_widgets::MenuItem::Divider,
                forge_widgets::MenuItem::Item {
                    label: "Delete\u{2026}".into(),
                    icon: Some(Icon::Eraser),
                    on_press: Message::Actions(ActionsMsg::DeleteAction(open_id)),
                    shortcut: None,
                    color: Some(p.random),
                    disabled: false,
                },
            ];
            let panel = forge_widgets::menu_panel(menu_items, palette);
            let overlay = container(panel)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(iced::Padding {
                    top: top_y,
                    right: 0.0,
                    bottom: 0.0,
                    left: 90.0,
                })
                .align_x(iced::Alignment::Start)
                .align_y(iced::Alignment::Start);
            iced::widget::stack![body_and_footer, overlay].into()
        } else {
            body_and_footer
        }
    } else {
        body_and_footer
    };

    let main_view: Element<'_, Message> =
        if let Some(form) = app.ui.actions.add_sub_action_modal.as_ref() {
            let modal_el = crate::app::add_sub_action_modal_view(form, palette);
            iced::widget::stack![main_view, modal_el].into()
        } else if let Some(form) = app.ui.actions.add_trigger_modal.as_ref() {
            let modal_el = crate::app::add_trigger_modal_view(form, palette);
            iced::widget::stack![main_view, modal_el].into()
        } else if let Some(form) = app.ui.actions.add_action_modal.as_ref() {
            let modal_el = crate::app::add_action_modal_view(form, palette);
            iced::widget::stack![main_view, modal_el].into()
        } else {
            main_view
        };

    iced::widget::column![page_header, main_view]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn actions_page_header<'a>(
    state: &'a crate::actions::ActionsState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{container, row, text};
    let p = *palette;

    let crumb_chevron = tabler_icon(Icon::ChevronRight, 11.0, p.text_faint);
    let crumb_chevron_2 = tabler_icon(Icon::ChevronRight, 11.0, p.text_faint);
    let crumbs_left = row![
        tabler_icon(Icon::Home, 13.0, p.text_faint),
        crumb_chevron,
        text("Automation").size(FONT_SM).color(p.text_muted),
        crumb_chevron_2,
        text("Actions").size(FONT_SM).color(p.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::alignment::Vertical::Center);

    let chip_all = forge_widgets::filter_chip(
        palette,
        "All",
        p.brand,
        state.filter == ActionsFilter::All,
        Message::Actions(ActionsMsg::FilterChanged(ActionsFilter::All)),
    );
    let chip_chat = forge_widgets::filter_chip(
        palette,
        "Chat",
        p.info,
        state.filter == ActionsFilter::Chat,
        Message::Actions(ActionsMsg::FilterChanged(ActionsFilter::Chat)),
    );
    let chip_timers = forge_widgets::filter_chip(
        palette,
        "Timers",
        p.warning,
        state.filter == ActionsFilter::Timers,
        Message::Actions(ActionsMsg::FilterChanged(ActionsFilter::Timers)),
    );
    let chip_points = forge_widgets::filter_chip(
        palette,
        "Points",
        p.accent_pink_light,
        state.filter == ActionsFilter::Points,
        Message::Actions(ActionsMsg::FilterChanged(ActionsFilter::Points)),
    );
    let chips = row![chip_all, chip_chat, chip_timers, chip_points].spacing(spf(Spacing::Xxs));

    let divider = container(iced::widget::Space::new().width(0.5).height(16.0))
        .width(0.5)
        .height(16.0)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.border_regular)),
            ..iced::widget::container::Style::default()
        });

    let search = forge_widgets::search_input(
        "Search actions...",
        &state.search,
        |q| Message::Actions(ActionsMsg::SearchChanged(q)),
        palette,
    );

    let new_btn = forge_widgets::primary_button_small(
        "+ New action",
        Message::Actions(ActionsMsg::OpenAddActionModal),
        palette,
    );

    let right_side = row![
        chips,
        divider,
        container(search).width(Length::Fixed(180.0)),
        new_btn,
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::alignment::Vertical::Center);

    let inner = row![
        crumbs_left,
        iced::widget::Space::new().width(Length::Fill),
        right_side,
    ]
    .align_y(iced::alignment::Vertical::Center);

    container(inner)
        .width(Length::Fill)
        .padding([10_u16, 16_u16])
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

fn actions_group_header<'a>(
    group: &'a crate::actions::ActionsGroup,
    collapsed: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{button, container, row, text};

    let p = *palette;
    let chevron_icon = if collapsed {
        Icon::ChevronRight
    } else {
        Icon::ChevronDown
    };
    let chevron_el = tabler_icon(chevron_icon, 11.0, p.text_faint);

    let cat_el = text(group.category.display_name())
        .size(FONT_XS)
        .color(p.text_muted)
        .font(forge_widgets::font(forge_widgets::FontRole::Monospace));

    let count_el = text(group.actions.len().to_string())
        .size(FONT_XS)
        .color(p.text_faint)
        .font(forge_widgets::font(forge_widgets::FontRole::Monospace));

    let inner = row![
        chevron_el,
        cat_el,
        iced::widget::Space::new().width(Length::Fill),
        count_el,
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::alignment::Vertical::Center);

    let cat = group.category.clone();

    button(container(inner).width(Length::Fill).padding([8, 16]))
        .on_press(Message::Actions(ActionsMsg::ToggleGroupCollapsed(cat)))
        .padding(0)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, status| {
            let bg_color = match status {
                iced::widget::button::Status::Hovered => p.elevated,
                _ => p.shell,
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg_color)),
                text_color: p.text_muted,
                border: iced::Border {
                    color: p.border_regular,
                    width: 0.5,
                    radius: 0.0.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            }
        })
        .into()
}

fn actions_tree_row<'a>(
    summary: &'a crate::actions::ActionSummary,
    selected: bool,
    menu_open: bool,
    rename_buf: Option<&'a str>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::menu_button_trigger;
    use iced::widget::{button, container, row, text, text_input};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);
    let action_id = summary.id;

    let state_icon = if summary.enabled {
        tabler_icon(Icon::CircleCheckFilled, 13.0, p.success)
    } else {
        tabler_icon(Icon::Circle, 13.0, p.text_faint)
    };

    let name_color = if !summary.enabled {
        p.text_faint
    } else if selected {
        p.text_primary
    } else {
        p.text_secondary
    };

    let name_el: Element<'a, Message> = if let Some(buf) = rename_buf {
        text_input("", buf)
            .id(crate::actions::action_rename_input_id())
            .on_input(|s| Message::Actions(ActionsMsg::RenameBufferChanged(s)))
            .on_submit(Message::Actions(ActionsMsg::RenameSubmit))
            .size(FONT_SM)
            .padding(iced::Padding {
                top: 2.0,
                bottom: 2.0,
                left: 6.0,
                right: 6.0,
            })
            .width(Length::Fill)
            .style(
                move |_t: &iced::Theme, _s| iced::widget::text_input::Style {
                    background: iced::Background::Color(p.shell),
                    border: iced::Border {
                        color: p.brand,
                        width: 0.5,
                        radius: forge_widgets::radius(forge_widgets::Radius::Sm).into(),
                    },
                    icon: p.text_muted,
                    placeholder: p.text_muted,
                    value: p.text_primary,
                    selection: iced::Color { a: 0.25, ..p.brand },
                },
            )
            .into()
    } else {
        container(
            text(&summary.name)
                .size(FONT_SM)
                .color(name_color)
                .font(mono)
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .width(Length::Fill)
        .clip(true)
        .into()
    };

    let count_el = text(summary.sub_action_count.to_string())
        .size(FONT_XS)
        .color(p.text_faint)
        .font(mono);

    let stripe_color = if selected {
        p.brand
    } else {
        iced::Color::TRANSPARENT
    };
    let stripe = container(iced::widget::Space::new().width(2.0).height(Length::Fill))
        .width(2.0)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(stripe_color)),
            ..iced::widget::container::Style::default()
        });

    let select_btn = button(
        row![state_icon, name_el, count_el,]
            .spacing(spf(Spacing::Xs))
            .align_y(iced::alignment::Vertical::Center),
    )
    .on_press(Message::Actions(ActionsMsg::ActionSelected(action_id)))
    .padding(iced::Padding {
        top: 6.0,
        bottom: 6.0,
        left: 32.0,
        right: 8.0,
    })
    .width(Length::Fill)
    .style(move |_theme: &iced::Theme, status| {
        let bg_color = match (selected, status) {
            (true, _) | (false, iced::widget::button::Status::Hovered) => p.base,
            _ => iced::Color::TRANSPARENT,
        };
        iced::widget::button::Style {
            background: Some(iced::Background::Color(bg_color)),
            text_color: name_color,
            border: iced::Border::default(),
            shadow: iced::Shadow::default(),
            snap: false,
        }
    });

    let menu_btn = menu_button_trigger(
        Icon::DotsVertical,
        menu_open,
        Message::Actions(ActionsMsg::ToggleActionMenu(action_id)),
        palette,
    );

    let right_col = container(menu_btn)
        .padding(iced::Padding {
            top: 2.0,
            bottom: 2.0,
            left: 0.0,
            right: 6.0,
        })
        .align_y(iced::Alignment::Center);

    row![stripe, select_btn, right_col]
        .spacing(0)
        .width(Length::Fill)
        .align_y(iced::Alignment::Center)
        .into()
}

fn actions_detail_panel<'a>(
    state: &'a crate::actions::ActionsState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{column, container, row, scrollable, text};

    let p = *palette;

    if state.selected.is_none() {
        return container(forge_widgets::empty_state(
            "No action selected",
            "Select an action from the list to view its details.",
            None::<(&str, Message)>,
            palette,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into();
    }

    let Some(detail) = &state.detail else {
        return container(text("Loading...").size(FONT_XS).color(p.text_muted))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([24, 24])
            .into();
    };

    let action = &detail.action;

    let enabled_dot_color = if action.enabled {
        p.success
    } else {
        p.text_faint
    };
    let enabled_dot_size = 5.0_f32;
    let enabled_dot = container(
        iced::widget::Space::new()
            .width(enabled_dot_size)
            .height(enabled_dot_size),
    )
    .width(enabled_dot_size)
    .height(enabled_dot_size)
    .style(move |_theme: &iced::Theme| iced::widget::container::Style {
        background: Some(iced::Background::Color(enabled_dot_color)),
        border: iced::Border {
            radius: (enabled_dot_size / 2.0).into(),
            color: iced::Color::TRANSPARENT,
            width: 0.0,
        },
        ..iced::widget::container::Style::default()
    });

    let status_label = if action.enabled {
        "Enabled"
    } else {
        "Disabled"
    };
    let status_badge = container(
        row![
            enabled_dot,
            text(status_label).size(FONT_XS).color(enabled_dot_color)
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(iced::alignment::Vertical::Center),
    )
    .padding([3, 8])
    .style(move |_theme: &iced::Theme| iced::widget::container::Style {
        background: Some(iced::Background::Color(p.shell)),
        border: iced::Border {
            color: p.border_regular,
            width: 0.5,
            radius: forge_widgets::radius(forge_widgets::Radius::Sm).into(),
        },
        ..iced::widget::container::Style::default()
    });

    let name_el = text(&action.name)
        .size(FONT_LG)
        .color(p.text_primary)
        .font(iced::Font {
            weight: iced::font::Weight::Medium,
            ..iced::Font::DEFAULT
        });

    let name_row = row![name_el, status_badge]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::alignment::Vertical::Center);

    let test_btn = forge_widgets::ghost_button_with_icon(
        Icon::PlayerPlay,
        "Test run",
        Message::Actions(ActionsMsg::TestTrigger(action.id)),
        palette,
    );
    let dup_btn = forge_widgets::ghost_button_with_icon(
        Icon::Copy,
        "Duplicate",
        Message::Actions(ActionsMsg::DuplicateAction(action.id)),
        palette,
    );
    let action_btns = row![test_btn, dup_btn].spacing(spf(Spacing::Xs));

    let header_row = row![container(name_row).width(Length::Fill), action_btns,]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::alignment::Vertical::Top);

    let mut detail_col = column![header_row].spacing(0);

    if let Some(desc) = &action.description {
        let desc_el = text(desc.as_str()).size(FONT_XS).color(p.text_muted);
        detail_col = detail_col.push(container(desc_el).padding([4, 0]));
    }

    detail_col = detail_col.push(iced::widget::Space::new().height(14.0));

    if state.telemetry_loading {
        let placeholder = crate::actions::telemetry_grid(&ActionTelemetry::default(), palette);
        detail_col = detail_col.push(placeholder);
        detail_col = detail_col.push(iced::widget::Space::new().height(18.0));
    } else if let Some(tel) = &state.telemetry {
        let grid = crate::actions::telemetry_grid(tel, palette);
        detail_col = detail_col.push(grid);
        detail_col = detail_col.push(iced::widget::Space::new().height(18.0));
    }

    detail_col = detail_col.push(section_header_with_add(
        &format!("TRIGGERS \u{00b7} {}", detail.triggers.len()),
        "Add trigger",
        p.warning,
        Message::Actions(ActionsMsg::OpenAddTriggerModal(action.id)),
        palette,
    ));
    detail_col = detail_col.push(iced::widget::Space::new().height(8.0));

    if detail.triggers.is_empty() {
        detail_col = detail_col.push(empty_placeholder_card(
            Icon::Bolt,
            p.warning,
            "No triggers \u{2014} this action will never fire on its own",
            palette,
        ));
    } else {
        for trigger in &detail.triggers {
            let kind_str = crate::actions::trigger_label_of(&trigger.kind);
            let trigger_row = container(
                row![
                    tabler_icon(Icon::Bolt, FONT_SM, p.brand),
                    text(kind_str).size(FONT_SM).color(p.text_secondary),
                ]
                .spacing(spf(Spacing::Xs))
                .align_y(iced::alignment::Vertical::Center),
            )
            .width(Length::Fill)
            .padding([18, 12])
            .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                background: Some(iced::Background::Color(p.shell)),
                border: iced::Border {
                    color: p.border_input,
                    width: 0.5,
                    radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
                },
                ..iced::widget::container::Style::default()
            });
            detail_col = detail_col.push(trigger_row);
            detail_col = detail_col.push(iced::widget::Space::new().height(6.0));
        }
    }
    detail_col = detail_col.push(iced::widget::Space::new().height(14.0));

    detail_col = detail_col.push(section_header_with_add(
        &format!("SUB-ACTIONS \u{00b7} {}", action.sub_actions.len()),
        "Add sub-action",
        p.brand,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::OpenRequested(action.id),
        ))),
        palette,
    ));
    detail_col = detail_col.push(iced::widget::Space::new().height(8.0));

    if action.sub_actions.is_empty() {
        detail_col = detail_col.push(empty_placeholder_card(
            Icon::Plus,
            p.brand,
            "No steps yet \u{2014} add one",
            palette,
        ));
    } else {
        for (i, spec) in action.sub_actions.iter().enumerate() {
            let step_label = format!("{}. {}", i + 1, spec.kind_label());
            let step_row = container(text(step_label).size(FONT_SM).color(p.text_secondary))
                .width(Length::Fill)
                .padding([18, 12])
                .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(p.shell)),
                    border: iced::Border {
                        color: p.border_input,
                        width: 0.5,
                        radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
                    },
                    ..iced::widget::container::Style::default()
                });
            detail_col = detail_col.push(step_row);
            detail_col = detail_col.push(iced::widget::Space::new().height(6.0));
        }
    }

    container(scrollable(container(detail_col).padding([18, 24])).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.elevated)),
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn section_header_with_add<'a>(
    label: &str,
    add_label: &'static str,
    add_color: iced::Color,
    on_add: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{button, container, row, text};
    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);

    let label_el = text(label.to_owned())
        .size(FONT_XS)
        .color(p.text_muted)
        .font(mono);

    let add_btn = button(
        row![
            tabler_icon(Icon::Plus, 11.0, add_color),
            text(add_label).size(FONT_XS).color(add_color),
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(iced::alignment::Vertical::Center),
    )
    .on_press(on_add)
    .padding([2_u16, 4_u16])
    .style(
        move |_theme: &iced::Theme, _status| iced::widget::button::Style {
            background: None,
            border: iced::Border::default(),
            text_color: iced::Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        },
    );

    container(
        row![
            label_el,
            iced::widget::Space::new().width(Length::Fill),
            add_btn,
        ]
        .align_y(iced::alignment::Vertical::Center),
    )
    .padding([6_u16, 14_u16])
    .width(Length::Fill)
    .into()
}

fn compute_action_menu_y_offset(
    state: &crate::actions::ActionsState,
    open_id: forge_types::ActionId,
) -> Option<f32> {
    const PAGE_HEADER_H: f32 = 40.0;
    const GROUP_HEADER_H: f32 = 28.0;
    const ROW_H: f32 = 30.0;

    let mut y = PAGE_HEADER_H;
    for group in &state.tree {
        let visible: Vec<&crate::actions::ActionSummary> = group
            .actions
            .iter()
            .filter(|a| state.action_passes_filter(a))
            .collect();
        if visible.is_empty() {
            continue;
        }
        y += GROUP_HEADER_H;
        if state.collapsed_groups.contains(&group.category) {
            continue;
        }
        for action in visible {
            if action.id == open_id {
                return Some(y + ROW_H);
            }
            y += ROW_H;
        }
    }
    None
}

fn empty_placeholder_card<'a>(
    icon: Icon,
    icon_color: iced::Color,
    label: &'static str,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{column, container, text};
    let p = *palette;

    let inner = column![
        tabler_icon(icon, 16.0, icon_color),
        text(label).size(FONT_XS).color(p.text_muted),
    ]
    .spacing(spf(Spacing::Xs))
    .align_x(iced::Alignment::Center);

    container(inner)
        .padding([18_u16, 12_u16])
        .width(Length::Fill)
        .align_x(iced::Alignment::Center)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: None,
            border: iced::Border {
                color: p.border_input,
                width: 0.5,
                radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn actions_footer<'a>(
    visible: usize,
    total: usize,
    palette: &'a ForgePalette,
) -> iced::widget::Container<'a, Message> {
    use iced::widget::{container, row, text};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);

    let left_str = format!(
        "Showing {} of {} \u{00b7} grouped by trigger",
        visible, total
    );
    let left_el = text(left_str).size(FONT_XS).color(p.text_faint).font(mono);

    let storage_el = text("Storage: \u{2014}")
        .size(FONT_XS)
        .color(p.text_faint)
        .font(mono);

    let dot_size = 6.0_f32;
    let green_dot = container(iced::widget::Space::new().width(dot_size).height(dot_size))
        .width(dot_size)
        .height(dot_size)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.success)),
            border: iced::Border {
                radius: (dot_size / 2.0).into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        });

    let saved_el = text("Auto-saved just now")
        .size(FONT_XS)
        .color(p.text_faint)
        .font(mono);

    let right = row![storage_el, green_dot, saved_el]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::alignment::Vertical::Center);

    let inner = row![container(left_el).width(Length::Fill), right,]
        .spacing(0)
        .align_y(iced::alignment::Vertical::Center);

    container(inner)
        .width(Length::Fill)
        .padding([7, 16])
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.shell)),
            border: iced::Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        })
}
