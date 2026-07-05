use forge_widgets::ForgePalette;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_SM, FONT_XS, FONT_XXS, Spacing, spf};
use iced::{Element, Length};

use crate::App;
use crate::actions::ActionsFilter;
use crate::message::{ActionsMsg, Message};

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
            container(
                text(forge_widgets::tr!("actions_loading"))
                    .size(FONT_XS)
                    .color(p.text_muted),
            )
            .padding([16, 14])
            .width(Length::Fill),
        );
    } else if total == 0 {
        tree_col = tree_col.push(
            container(
                text(forge_widgets::tr!("actions_empty"))
                    .size(FONT_XS)
                    .color(p.text_faint),
            )
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
                    let hovered = actions_state.hovered_action == Some(summary.id);
                    let rename_buf = actions_state
                        .renaming_action
                        .as_ref()
                        .filter(|(id, _)| *id == summary.id)
                        .map(|(_, n)| n.as_str());
                    tree_col = tree_col.push(actions_tree_row(
                        summary, selected, menu_open, hovered, rename_buf, palette,
                    ));
                }
            }
        }
    }

    let tree_scrollable = scrollable(tree_col).height(Length::Fill);

    let left_panel = container(tree_scrollable)
        .width(Length::Fixed(290.0))
        .height(Length::Fill)
        .padding(iced::Padding {
            top: 8.0,
            bottom: 8.0,
            left: 0.0,
            right: 0.0,
        })
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.shell)),
            ..iced::widget::container::Style::default()
        });

    let left_hairline = container(iced::widget::Space::new().width(0.5).height(Length::Fill))
        .width(0.5)
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.border_regular)),
            ..iced::widget::container::Style::default()
        });

    let right_panel = crate::action_editor_view::detail_pane(app, palette);

    let footer = actions_footer(visible, total, palette);

    let body = row![left_panel, left_hairline, right_panel]
        .spacing(0)
        .height(Length::Fill);

    let body_and_footer: Element<'_, Message> = container(column![body, footer].spacing(0))
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    let main_view: Element<'_, Message> = body_and_footer;

    let main_view: Element<'_, Message> =
        if let Some(form) = app.ui.actions.add_sub_action_modal.as_ref() {
            let modal_el = crate::actions_modals::add_sub_action_modal_view(form, &app.rt, palette);
            iced::widget::stack![main_view, modal_el].into()
        } else if let Some(form) = app.ui.actions.add_action_modal.as_ref() {
            let modal_el = crate::actions_modals::add_action_modal_view(form, palette);
            iced::widget::stack![main_view, modal_el].into()
        } else if let Some(picker_state) = app.ui.actions.trigger_picker.as_ref() {
            let picker_el = crate::actions_trigger_picker::view(picker_state, &app.rt, palette);
            iced::widget::stack![main_view, picker_el].into()
        } else {
            main_view
        };

    let main_view: Element<'_, Message> =
        match crate::action_editor_view::pending_delete_modal(app, palette) {
            Some(modal) => iced::widget::stack![main_view, modal].into(),
            None => main_view,
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
        text(forge_widgets::tr!("actions_breadcrumb_automation"))
            .size(FONT_SM)
            .color(p.text_muted),
        crumb_chevron_2,
        text(forge_widgets::tr!("actions_breadcrumb_actions"))
            .size(FONT_SM)
            .color(p.text_primary),
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(iced::alignment::Vertical::Center);

    let lbl_all = forge_widgets::tr!("actions_filter_all");
    let lbl_chat = forge_widgets::tr!("actions_filter_chat");
    let lbl_timers = forge_widgets::tr!("actions_filter_timers");
    let lbl_points = forge_widgets::tr!("actions_filter_points");

    let chip_all = forge_widgets::filter_chip(
        palette,
        &lbl_all,
        p.brand,
        state.filter == ActionsFilter::All,
        Message::Actions(ActionsMsg::FilterChanged(ActionsFilter::All)),
    );
    let chip_chat = forge_widgets::filter_chip(
        palette,
        &lbl_chat,
        p.info,
        state.filter == ActionsFilter::Chat,
        Message::Actions(ActionsMsg::FilterChanged(ActionsFilter::Chat)),
    );
    let chip_timers = forge_widgets::filter_chip(
        palette,
        &lbl_timers,
        p.warning,
        state.filter == ActionsFilter::Timers,
        Message::Actions(ActionsMsg::FilterChanged(ActionsFilter::Timers)),
    );
    let chip_points = forge_widgets::filter_chip(
        palette,
        &lbl_points,
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
        forge_widgets::tr!("actions_search_placeholder"),
        &state.search,
        |q| Message::Actions(ActionsMsg::SearchChanged(q)),
        palette,
    );

    let new_btn = forge_widgets::primary_button_small(
        forge_widgets::tr!("actions_new_btn"),
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
    let chevron_el = tabler_icon(chevron_icon, 11.0, p.text_muted);

    let cat_el = text(group.category.display_name())
        .size(FONT_XXS)
        .color(p.text_muted)
        .font(forge_widgets::font(forge_widgets::FontRole::Monospace));

    let count_el = text(group.actions.len().to_string())
        .size(FONT_XXS)
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

    button(container(inner).width(Length::Fill).padding([6, 14]))
        .on_press(Message::Actions(ActionsMsg::ToggleGroupCollapsed(cat)))
        .padding(0)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, status| {
            let bg_color = match status {
                iced::widget::button::Status::Hovered => p.elevated,
                _ => iced::Color::TRANSPARENT,
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg_color)),
                text_color: p.text_muted,
                border: iced::Border::default(),
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
    hovered: bool,
    rename_buf: Option<&'a str>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::{MenuPlacement, menu_button};
    use iced::widget::{button, container, mouse_area, row, text, text_input};

    // Fixed row height so the selection stripe, the row background, and the
    // overflow-menu button all share one clean bar (no per-widget height drift).
    const ROW_HEIGHT: f32 = 30.0;
    // Right slot width shared by the count ("N sub") and the `⋮` button so
    // swapping one for the other on hover never shifts the name column; wide
    // enough for a two-digit "NN sub" count.
    const RIGHT_SLOT_WIDTH: f32 = 46.0;

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);
    let action_id = summary.id;

    let state_icon = if summary.enabled {
        tabler_icon(Icon::CircleCheckFilled, 11.0, p.success)
    } else {
        tabler_icon(Icon::Circle, 11.0, p.text_faint)
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
            .size(FONT_XS)
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
                .size(FONT_XS)
                .color(name_color)
                .font(forge_widgets::font(forge_widgets::FontRole::Body))
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .width(Length::Fill)
        .clip(true)
        .into()
    };

    // Action rows show the sub-action count as "N sub"; only the group header
    // (see `actions_group_header`) shows a bare number (its action count).
    let count_el = text(format!("{} sub", summary.sub_action_count))
        .size(FONT_XXS)
        .color(p.text_faint)
        .font(mono);

    let stripe_color = if selected {
        p.brand
    } else {
        iced::Color::TRANSPARENT
    };
    let stripe = container(iced::widget::Space::new().width(2.0).height(Length::Fill))
        .width(2.0)
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(stripe_color)),
            ..iced::widget::container::Style::default()
        });

    // The row highlight is painted once by the outer container so it spans the
    // whole bar (name + count/menu); every inner widget stays transparent, which
    // is why the `⋮` slot is highlighted together with the name on select/hover.
    let select_btn = button(
        row![state_icon, name_el]
            .spacing(spf(Spacing::Xs))
            .height(Length::Fill)
            .align_y(iced::alignment::Vertical::Center),
    )
    .on_press(Message::Actions(ActionsMsg::ActionSelected(action_id)))
    .padding(iced::Padding {
        top: 0.0,
        bottom: 0.0,
        left: 32.0,
        right: 8.0,
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .style(
        move |_theme: &iced::Theme, _status| iced::widget::button::Style {
            background: Some(iced::Background::Color(iced::Color::TRANSPARENT)),
            text_color: name_color,
            border: iced::Border::default(),
            shadow: iced::Shadow::default(),
            snap: false,
        },
    );

    // The count and the `⋮` menu share one fixed-width, right-aligned slot so
    // swapping them on hover never shifts the right edge; the slot's right edge
    // sits at the same 14px gutter as the group header's count.
    let show_menu = hovered || menu_open;
    let slot_inner: Element<'a, Message> = if show_menu {
        let toggle_label = if summary.enabled {
            forge_widgets::tr!("actions_menu_disable")
        } else {
            forge_widgets::tr!("actions_menu_enable")
        };
        let menu_items: Vec<forge_widgets::MenuItem<Message>> = vec![
            forge_widgets::MenuItem::Item {
                label: forge_widgets::tr!("actions_menu_rename"),
                icon: Some(Icon::InfoCircle),
                on_press: Message::Actions(ActionsMsg::RenameStarted(action_id)),
                shortcut: None,
                color: None,
                disabled: false,
            },
            forge_widgets::MenuItem::Item {
                label: forge_widgets::tr!("actions_menu_duplicate"),
                icon: Some(Icon::Copy),
                on_press: Message::Actions(ActionsMsg::DuplicateAction(action_id)),
                shortcut: None,
                color: None,
                disabled: false,
            },
            forge_widgets::MenuItem::Item {
                label: toggle_label,
                icon: Some(Icon::Bolt),
                on_press: Message::Actions(ActionsMsg::ToggleEnabled(action_id, !summary.enabled)),
                shortcut: None,
                color: None,
                disabled: false,
            },
            forge_widgets::MenuItem::Divider,
            forge_widgets::MenuItem::Item {
                label: forge_widgets::tr!("actions_menu_delete"),
                icon: Some(Icon::Eraser),
                on_press: Message::Actions(ActionsMsg::DeleteRequested(action_id)),
                shortcut: None,
                color: Some(p.random),
                disabled: false,
            },
        ];
        menu_button(
            Icon::DotsVertical,
            menu_open,
            Message::Actions(ActionsMsg::ToggleActionMenu(action_id)),
            Message::Actions(ActionsMsg::DismissActionMenu),
            menu_items,
            MenuPlacement::BottomRight,
            palette,
        )
    } else {
        count_el.into()
    };

    let right_col = container(
        container(slot_inner)
            .width(Length::Fixed(RIGHT_SLOT_WIDTH))
            .align_x(iced::alignment::Horizontal::Right)
            .align_y(iced::Alignment::Center),
    )
    .height(Length::Fill)
    .padding(iced::Padding {
        top: 0.0,
        bottom: 0.0,
        left: 0.0,
        right: 14.0,
    })
    .align_y(iced::Alignment::Center);

    let row_bg = if selected || hovered {
        p.elevated
    } else {
        iced::Color::TRANSPARENT
    };
    let row_inner = row![stripe, select_btn, right_col]
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fixed(ROW_HEIGHT))
        .align_y(iced::Alignment::Center);

    let row_el = container(row_inner)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(row_bg)),
            ..iced::widget::container::Style::default()
        });

    mouse_area(row_el)
        .on_enter(Message::Actions(ActionsMsg::RowHover(action_id, true)))
        .on_exit(Message::Actions(ActionsMsg::RowHover(action_id, false)))
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

    let left_str = forge_widgets::tr!(
        "actions_footer_showing",
        visible = visible as i64,
        total = total as i64
    );
    let left_el = text(left_str).size(FONT_XS).color(p.text_faint).font(mono);

    let storage_el = text(forge_widgets::tr!("actions_footer_storage"))
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

    let saved_el = text(forge_widgets::tr!("actions_footer_autosaved"))
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
