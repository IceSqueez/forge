use super::*;
use forge_components::RevealsFocusIn;
use forge_components::SEARCH_FIELD_WIDTH;
use forge_components::{
    ActivePresentation, BORDER_THIN, ChipGlyph, ConfirmTone, FONT_XS, FONT_XXS, Icon, ResizeEdge,
    ResizeRange, chip, confirm_modal, context_menu, header_stat, header_stats, icon,
    install_resize, menu_divider, menu_item, scaled,
};
use gpui::{App, MouseButton, MouseDownEvent, Rgba};

pub(super) struct TreeActionRow {
    pub(super) id: ActionId,
    pub(super) child_index: usize,
}

const ROW_PAD_R: Pixels = px(8.0);

fn tree_notice(label: SharedString, color: Rgba, density: Density, cx: &App) -> impl IntoElement {
    div()
        .w_full()
        .px(TREE_GUTTER)
        .py(spacing(Spacing::Sm, density))
        .font_family(cx.body_family())
        .text_size(FONT_XS)
        .text_color(color)
        .child(label)
}

impl Render for ActionTree {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drive_overlay_focus(
            self.row_open.is_some(),
            &self.menu_focus,
            &mut self.menu_focus_restore,
            window,
            cx,
        );
        self.keys.claim_focus_once(window, cx);
        let palette = cx.palette();
        self.render_tree(&palette, cx)
    }
}

impl ActionTree {
    pub(in crate::actions_screen) fn render_stats(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let total = self.total_actions();
        let enabled = self.enabled_count();
        header_stats(
            vec![
                header_stat(
                    total.to_string(),
                    palette.text_primary,
                    tr!("actions_stat_actions"),
                ),
                header_stat(
                    enabled.to_string(),
                    palette.success,
                    tr!("actions_stat_enabled"),
                ),
                header_stat(
                    total.saturating_sub(enabled).to_string(),
                    palette.warning,
                    tr!("actions_stat_disabled"),
                ),
            ],
            palette,
            density,
            cx,
        )
        .into_any_element()
    }

    pub(in crate::actions_screen) fn render_filter_left(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                chip(
                    tr!("actions_filter_all"),
                    ChipGlyph::Dot(palette.brand),
                    self.filter == ActionsFilter::All,
                    palette,
                )
                .on_click(
                    "actions-chip-all",
                    cx.listener(|this, _, _, cx| this.set_filter(ActionsFilter::All, cx)),
                ),
            )
            .child(
                chip(
                    tr!("actions_filter_chat"),
                    ChipGlyph::DotIcon(palette.info, Icon::MessageCircle),
                    self.filter == ActionsFilter::Chat,
                    palette,
                )
                .on_click(
                    "actions-chip-chat",
                    cx.listener(|this, _, _, cx| this.set_filter(ActionsFilter::Chat, cx)),
                ),
            )
            .child(
                chip(
                    tr!("actions_filter_timers"),
                    ChipGlyph::DotIcon(palette.warning, Icon::Clock),
                    self.filter == ActionsFilter::Timers,
                    palette,
                )
                .on_click(
                    "actions-chip-timers",
                    cx.listener(|this, _, _, cx| this.set_filter(ActionsFilter::Timers, cx)),
                ),
            )
            .child(
                chip(
                    tr!("actions_filter_points"),
                    ChipGlyph::DotIcon(palette.accent_pink_light, Icon::Star),
                    self.filter == ActionsFilter::Points,
                    palette,
                )
                .on_click(
                    "actions-chip-points",
                    cx.listener(|this, _, _, cx| this.set_filter(ActionsFilter::Points, cx)),
                ),
            );

        let divider = div()
            .w(BORDER_THIN)
            .h(HEADER_DIV_H)
            .bg(palette.border_regular);

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .w(SEARCH_FIELD_WIDTH)
                    .child(self.search.field().clone()),
            )
            .child(divider)
            .child(chips)
            .into_any_element()
    }

    pub(super) fn tree_action_rows(&self) -> Vec<TreeActionRow> {
        let mut rows = Vec::new();
        let mut child_index = 0;
        for group in &self.groups {
            let mut surviving = group
                .actions
                .iter()
                .filter(|a| self.action_passes(a))
                .peekable();
            if surviving.peek().is_none() {
                continue;
            }
            child_index += 1;
            if group.collapsed {
                continue;
            }
            for action in surviving {
                rows.push(TreeActionRow {
                    id: action.id,
                    child_index,
                });
                child_index += 1;
            }
        }
        rows
    }

    pub(super) fn render_tree(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let mut rows: Vec<AnyElement> = Vec::new();

        if self.total_actions() == 0 {
            let notice = load_notice(
                &self.list_load,
                LoadNotice {
                    id: "actions-load",
                    loading: tr!("actions_tree_loading"),
                    width: NoticeWidth::NarrowColumn,
                },
                cx.listener(|this, _: &ClickEvent, _, cx| this.retry_list_load(cx)),
                palette,
                density,
                cx,
            );
            rows.push(notice.unwrap_or_else(|| {
                tree_notice(
                    SharedString::from(tr!("actions_empty")),
                    palette.text_faint,
                    density,
                    cx,
                )
                .into_any_element()
            }));
        } else {
            for (index, group) in self.groups.iter().enumerate() {
                let surviving: Vec<&ActionSummary> = group
                    .actions
                    .iter()
                    .filter(|a| self.action_passes(a))
                    .collect();
                if surviving.is_empty() {
                    continue;
                }
                rows.push(self.render_group_header(index, group, surviving.len(), palette, cx));
                if !group.collapsed {
                    for action in surviving {
                        rows.push(self.render_row(action, palette, cx));
                    }
                }
            }
        }

        let focus = self.keys.focus().clone();
        let inner = navigable_list(div().id("actions-tree"), &focus, cx)
            .track_scroll(&self.scroll)
            .reveals_focus_in(&self.scroll)
            .flex_1()
            .min_h(px(0.0))
            .py(spacing(Spacing::Xs, density))
            .overflow_y_scroll()
            .children(rows);

        let panel = div()
            .w(self.width)
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.shell)
            .border_r(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(inner);

        install_resize(
            panel,
            ActionsTreeResizeDrag,
            "actions-tree-resize",
            ResizeEdge::Right,
            ResizeRange {
                min: LEFT_PANEL_MIN,
                max: LEFT_PANEL_MAX,
            },
            palette,
            cx.listener(|this, width: &Pixels, _, cx| this.set_tree_width(*width, cx)),
        )
        .into_any_element()
    }

    fn render_group_header(
        &self,
        index: usize,
        group: &ActionGroup,
        shown_count: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let chevron = if group.collapsed {
            Icon::ChevronRight
        } else {
            Icon::ChevronDown
        };
        let hover_bg = palette.elevated;
        div()
            .id(("actions-group", index))
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .px(TREE_GUTTER)
            .py(spacing(Spacing::Xs, density))
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .key_activated(palette.text_primary)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_group(index, cx)))
            .child(icon(chevron, TREE_GLYPH, palette.text_muted))
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(group.name.clone()),
            )
            .child(div().flex_1())
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(shown_count.to_string()),
            )
            .into_any_element()
    }

    fn render_row(
        &self,
        action: &ActionSummary,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let id = action.id;
        let selected = self.selected == Some(id);
        let hovered = self.hovered == Some(id);
        let renaming = self.renaming.as_ref().filter(|r| r.id == id);

        let (state_icon, state_color) = if action.enabled {
            (Icon::CircleCheckFilled, palette.success)
        } else {
            (Icon::Circle, palette.text_faint)
        };
        let name_color = if !action.enabled {
            palette.text_faint
        } else if selected {
            palette.text_primary
        } else {
            palette.text_secondary
        };
        let stripe_color = if selected {
            palette.brand
        } else {
            gpui::transparent_black().into()
        };
        let row_bg: Rgba = if selected || hovered || self.keys.is_at(&id) {
            palette.elevated
        } else {
            gpui::transparent_black().into()
        };

        let name_el: AnyElement = match renaming {
            Some(renaming) => div()
                .flex_1()
                .min_w(px(0.0))
                .child(renaming.editor.clone())
                .into_any_element(),
            None => div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .font_family(cx.mono_family())
                .text_size(FONT_XS)
                .text_color(name_color)
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        if event.click_count >= 2 {
                            this.start_rename(id, window, cx);
                        }
                    }),
                )
                .child(action.name.clone())
                .into_any_element(),
        };

        let select_area = div()
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .pl(scaled(ROW_INDENT, density))
            .pr(scaled(ROW_PAD_R, density))
            .child(icon(state_icon, TREE_GLYPH, state_color))
            .child(name_el);

        let slot_inner: AnyElement = div()
            .font_family(cx.mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!(
                "action_editor_sub_count",
                count = i64::try_from(action.sub_action_count).unwrap_or(i64::MAX)
            ))
            .into_any_element();
        let right_slot = div().pr(ROW_GUTTER).child(
            div()
                .flex_none()
                .min_w(RIGHT_SLOT_W)
                .whitespace_nowrap()
                .flex()
                .items_center()
                .justify_end()
                .child(slot_inner),
        );

        let row = div()
            .id(SharedString::from(format!("actions-row-{id}")))
            .w_full()
            .h(ROW_HEIGHT)
            .flex()
            .items_center()
            .bg(row_bg)
            .on_hover(
                cx.listener(move |this, hovered: &bool, _, cx| this.set_hover(id, *hovered, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    this.open_row_menu(id, ev.position, cx)
                }),
            )
            .child(div().w(STRIPE_W).h_full().bg(stripe_color))
            .child(select_area)
            .child(right_slot);
        if renaming.is_some() {
            return row.into_any_element();
        }
        row.cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select(id, cx)))
            .into_any_element()
    }

    pub(in crate::actions_screen) fn render_row_context_menu(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = self.row_open?;
        let position = self.menu_pos?;
        let action = self
            .groups
            .iter()
            .flat_map(|g| &g.actions)
            .find(|a| a.id == id)?;
        let toggle_label = if action.enabled {
            tr!("actions_menu_disable")
        } else {
            tr!("actions_menu_enable")
        };
        let next_enabled = !action.enabled;
        let view = cx.entity();

        let items = vec![
            menu_item(
                SharedString::from(format!("actions-menu-rename-{id}")),
                tr!("actions_menu_rename"),
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.start_rename(id, window, cx)
                }),
            )
            .icon(Icon::Pencil)
            .into(),
            menu_item(
                SharedString::from(format!("actions-menu-dup-{id}")),
                tr!("actions_menu_duplicate"),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.duplicate(id, cx)),
            )
            .icon(Icon::Copy)
            .into(),
            menu_item(
                SharedString::from(format!("actions-menu-toggle-{id}")),
                toggle_label,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_enabled(id, next_enabled, cx)
                }),
            )
            .icon(Icon::Bolt)
            .into(),
            menu_divider(),
            menu_item(
                SharedString::from(format!("actions-menu-del-{id}")),
                tr!("actions_menu_delete"),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(id, cx)),
            )
            .icon(Icon::Eraser)
            .color(palette.danger)
            .into(),
        ];

        Some(
            context_menu(position, palette)
                .items(items)
                .dismiss_on_escape(&self.menu_focus)
                .on_dismiss(move |_window, cx| {
                    view.update(cx, |this, cx| this.close_menu(cx));
                })
                .into_any_element(),
        )
    }

    pub(in crate::actions_screen) fn render_delete_layer(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = self.pending_delete.get().copied()?;
        Some(self.render_delete_confirm(id, palette, cx))
    }

    fn render_delete_confirm(
        &self,
        id: ActionId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self.find(id).map(|a| a.name.clone()).unwrap_or_default();
        let body = match self.delete_scheduled_note(id) {
            Some(note) => tr!("actions_delete_body_with_note", note = note.as_str()),
            None => tr!("actions_delete_body"),
        };
        let card = confirm_modal(
            tr!("actions_delete_title"),
            body,
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "actions-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "actions-delete-confirm",
            tr!("common_delete"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }
}
