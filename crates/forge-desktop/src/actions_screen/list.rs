use super::action_writes::{ActionSettings, ActionWrite};
use super::*;
use crate::async_bridge;
use crate::navigable_list::{ListStep, NavigableList, navigable_list};
use crate::repo_failure::RepoFailure;
use forge_components::ActivePresentation;
use forge_components::NAME_FIELD_LIMIT;
use forge_components::PushToast;
use forge_components::SEARCH_FIELD_WIDTH;
use forge_components::{
    BORDER_THIN, ChipGlyph, ConfirmTone, Density, FONT_XS, FONT_XXS, ForgePalette, Icon,
    InlineEditEvent, InputEvent, ModalSize, OverlayPosition, ResizeEdge, ResizeRange, Spacing,
    TextArea, TextInput, ToastKind, body_family, chip, confirm_modal, context_menu, header_stat,
    header_stats, icon, inline_edit, install_resize, menu_divider, menu_item, modal, mono_family,
    overlay, primary_button, primary_button_with_icon, scaled, secondary_button, spacing,
    status_dot, toggle, tr,
};
use forge_types::{Action, ActionId, DEFAULT_QUEUE_ID, ExecutionMode, Queue};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, MouseButton, MouseDownEvent, Pixels, Point, Rgba,
    SharedString, Window, div, px,
};
use std::sync::Arc;

struct TreeActionRow {
    id: ActionId,
    child_index: usize,
}

const DESCRIPTION_HEIGHT: Pixels = px(72.0);
const ROW_PAD_R: Pixels = px(8.0);

fn tree_notice(label: SharedString, color: Rgba, density: Density) -> impl IntoElement {
    div()
        .w_full()
        .px(TREE_GUTTER)
        .py(spacing(Spacing::Sm, density))
        .font_family(body_family())
        .text_size(FONT_XS)
        .text_color(color)
        .child(label)
}

fn modal_section(
    palette: &ForgePalette,
    label: impl Into<SharedString>,
    control: impl IntoElement,
    density: Density,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(label.into()),
        )
        .child(control)
}

impl NavigableList for ScreenActionsView {
    fn step_list(&mut self, step: ListStep, cx: &mut Context<Self>) {
        if self.renaming.is_some() {
            return;
        }
        let rows = self.tree_action_rows();
        let ids: Vec<ActionId> = rows.iter().map(|row| row.id).collect();
        if let Some(next) = self.tree_keys.step(&ids, self.selected.as_ref(), step) {
            if let Some(row) = rows.get(next) {
                self.tree_scroll.scroll_to_item(row.child_index);
            }
            cx.notify();
        }
    }

    fn activate_list(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.renaming.is_some() {
            return;
        }
        let ids: Vec<ActionId> = self.tree_action_rows().iter().map(|row| row.id).collect();
        if let Some(id) = self.tree_keys.activation_target(&ids) {
            self.select(id, cx);
        }
    }
}

impl ScreenActionsView {
    pub(super) fn find(&self, id: ActionId) -> Option<&ActionSummary> {
        self.groups
            .iter()
            .flat_map(|g| g.actions.iter())
            .find(|a| a.id == id)
    }

    fn total_actions(&self) -> usize {
        self.groups.iter().map(|g| g.actions.len()).sum()
    }

    fn enabled_count(&self) -> usize {
        self.groups
            .iter()
            .flat_map(|g| g.actions.iter())
            .filter(|a| a.enabled)
            .count()
    }

    fn action_passes(&self, action: &ActionSummary) -> bool {
        let in_filter = self.filter == ActionsFilter::All
            || self
                .filter_action_ids
                .get(&self.filter)
                .is_some_and(|ids| ids.contains(&action.id));
        in_filter && self.search.matches(&action.name)
    }

    pub(super) fn on_search_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if self.search.on_changed(event) {
            cx.notify();
        }
    }

    fn set_filter(&mut self, filter: ActionsFilter, cx: &mut Context<Self>) {
        self.filter = filter;
        cx.notify();
    }

    fn toggle_group(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(group) = self.groups.get_mut(index) {
            group.collapsed = !group.collapsed;
            cx.notify();
        }
    }

    pub(super) fn select(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.tree_keys.move_to(id);
        self.detail = None;
        self.telemetry = None;
        self.last_outcome = None;
        self.nav_path.clear();
        self.step_menu_open = None;
        self.sync_case_fields(cx);
        self.load_detail_for(id, cx);
        cx.notify();
    }

    fn set_hover(&mut self, id: ActionId, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            if self.hovered != Some(id) {
                self.hovered = Some(id);
                cx.notify();
            }
        } else if self.hovered == Some(id) {
            self.hovered = None;
            cx.notify();
        }
    }

    fn open_row_menu(&mut self, id: ActionId, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.menu_open = Some(id);
        self.menu_click_pos = Some(position);
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
    }

    pub(super) fn set_enabled(&mut self, id: ActionId, enabled: bool, cx: &mut Context<Self>) {
        self.menu_open = None;
        self.write_action(id, ActionWrite::SetEnabled(enabled), cx);
    }

    pub(super) fn duplicate(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.menu_open = None;
        self.write_action(
            id,
            ActionWrite::Duplicate {
                new_id: ActionId::new(),
                copy_suffix: tr!("actions_duplicate_name_suffix"),
            },
            cx,
        );
    }

    fn start_rename(&mut self, id: ActionId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action) = self.find(id) else {
            return;
        };
        let palette = cx.palette();
        let seed = action.name.clone();
        let editor = inline_edit(seed, palette, FONT_XS, window, cx);
        let sub = cx.subscribe(
            &editor,
            |this, _e, event: &InlineEditEvent, cx| match event {
                InlineEditEvent::Commit(next) => this.commit_rename(next.clone(), cx),
                InlineEditEvent::Cancel => this.cancel_rename(cx),
            },
        );
        self.menu_open = None;
        self.renaming = Some(Renaming {
            id,
            editor,
            _sub: sub,
        });
        cx.notify();
    }

    fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.renaming = None;
        cx.notify();
    }

    fn commit_rename(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(renaming) = self.renaming.take() else {
            cx.notify();
            return;
        };
        let trimmed = name.trim().to_owned();
        cx.notify();
        if trimmed.is_empty() {
            return;
        }
        let id = renaming.id;
        let taken = self
            .groups
            .iter()
            .flat_map(|g| g.actions.iter())
            .any(|a| a.id != id && a.name.to_lowercase() == trimmed.to_lowercase());
        if taken {
            cx.push_toast(
                ToastKind::Error,
                tr!("actions_rename_taken", name = trimmed.as_str()),
            );
            return;
        }
        self.write_action(id, ActionWrite::Rename(trimmed), cx);
    }

    pub(super) fn request_delete(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.pending_delete.request(id);
        self.menu_open = None;
        self.load_delete_scheduled_count(id, cx);
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete.cancel();
        self.delete_scheduled_count = None;
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.pending_delete.take() else {
            return;
        };
        let name = self.find(id).map(|a| a.name.clone()).unwrap_or_default();
        self.delete_scheduled_count = None;
        self.write_action(id, ActionWrite::Archive { name }, cx);
    }

    pub(super) fn raise_undo_toast(&self, id: ActionId, name: String, cx: &mut Context<Self>) {
        let view = cx.entity();
        let message = tr!("actions_deleted_toast", name = name.as_str());
        cx.push_undo_toast(message, move |_window, app: &mut App| {
            view.update(app, |this, cx| {
                this.write_action(id, ActionWrite::Restore, cx)
            });
        });
    }

    pub(super) fn open_action_modal(
        &mut self,
        base: Option<Action>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let palette = cx.palette();
        let seed_name = base.as_ref().map(|a| a.name.clone()).unwrap_or_default();
        let seed_group = base
            .as_ref()
            .and_then(|a| a.group.clone())
            .unwrap_or_default();
        let seed_desc = base
            .as_ref()
            .and_then(|a| a.description.clone())
            .unwrap_or_default();
        let name = cx.new(|cx| {
            let mut input =
                TextInput::new(tr!("actions_name_placeholder"), cx).with_palette(palette);
            input.set_content(seed_name, cx);
            input
        });
        let group = cx.new(|cx| {
            let mut input =
                TextInput::new(tr!("actions_group_placeholder"), cx).with_palette(palette);
            input.set_content(seed_group, cx);
            input
        });
        let description = cx.new(|cx| {
            let mut area = TextArea::new(tr!("actions_description_placeholder"), cx)
                .with_palette(palette)
                .with_height(DESCRIPTION_HEIGHT);
            area.set_content(seed_desc, cx);
            area
        });
        name.update(cx, |f, cx| f.focus(window, cx));
        let name_sub = cx.subscribe(&name, |_this, _f, _e: &InputEvent, cx| cx.notify());
        let (editing, enabled, concurrent, bypass_pause, random_pick, preselect_queue) = match &base
        {
            Some(a) => (
                Some(a.id),
                a.enabled,
                a.concurrent,
                a.bypass_pause,
                a.execution_mode == ExecutionMode::RandomPick,
                Some(a.queue_id),
            ),
            None => (None, true, false, false, false, None),
        };
        self.action_modal = Some(ActionForm {
            editing,
            base,
            name,
            group,
            description,
            queues: Vec::new(),
            selected_queue: 0,
            preselect_queue,
            enabled,
            concurrent,
            bypass_pause,
            random_pick,
            _name_sub: name_sub,
        });
        cx.notify();

        let repo = Arc::clone(&self.queue_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { repo.list().await.map_err(RepoFailure::from) },
            |this, result, cx| match result {
                Ok(queues) => this.apply_queue_options(queues, cx),
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
    }

    fn apply_queue_options(&mut self, queues: Vec<Queue>, cx: &mut Context<Self>) {
        if let Some(form) = self.action_modal.as_mut() {
            form.queues = queues
                .into_iter()
                .map(|q| (q.id, SharedString::from(q.name)))
                .collect();
            let target = form.preselect_queue.unwrap_or(DEFAULT_QUEUE_ID);
            form.selected_queue = form
                .queues
                .iter()
                .position(|(qid, _)| *qid == target)
                .unwrap_or(0);
            cx.notify();
        }
    }

    fn cancel_action_modal(&mut self, cx: &mut Context<Self>) {
        self.action_modal = None;
        cx.notify();
    }

    fn select_queue(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(form) = self.action_modal.as_mut() {
            form.selected_queue = index;
            cx.notify();
        }
    }

    fn toggle_modal_enabled(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.action_modal.as_mut() {
            form.enabled = !form.enabled;
            cx.notify();
        }
    }

    fn toggle_modal_concurrent(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.action_modal.as_mut() {
            form.concurrent = !form.concurrent;
            cx.notify();
        }
    }

    fn toggle_modal_bypass(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.action_modal.as_mut() {
            form.bypass_pause = !form.bypass_pause;
            cx.notify();
        }
    }

    fn toggle_modal_random(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.action_modal.as_mut() {
            form.random_pick = !form.random_pick;
            cx.notify();
        }
    }

    pub(super) fn submit_action_modal(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.action_modal.as_ref() else {
            return;
        };
        let name = form.name.read(cx).content().trim().to_owned();
        if name.is_empty() {
            return;
        }
        let Some(&(queue_id, _)) = form.queues.get(form.selected_queue) else {
            cx.push_toast(ToastKind::Error, tr!("actions_no_queue"));
            return;
        };
        let group_name = form.group.read(cx).content().trim().to_owned();
        let description = form.description.read(cx).content().trim().to_owned();
        let execution_mode = if form.random_pick {
            ExecutionMode::RandomPick
        } else {
            ExecutionMode::Sequential
        };
        let group = (!group_name.is_empty()).then_some(group_name);
        let description = (!description.is_empty()).then_some(description);
        let write_target = form.base.as_ref().map(|existing| existing.id);
        let (enabled, concurrent, bypass_pause) =
            (form.enabled, form.concurrent, form.bypass_pause);
        self.action_modal = None;
        match write_target {
            Some(id) => self.write_action(
                id,
                ActionWrite::Settings(ActionSettings {
                    name,
                    group,
                    queue_id,
                    enabled,
                    concurrent,
                    bypass_pause,
                    execution_mode,
                    description,
                }),
                cx,
            ),
            None => {
                let action = Action {
                    id: ActionId::new(),
                    name,
                    group,
                    queue_id,
                    enabled,
                    concurrent,
                    bypass_pause,
                    execution_mode,
                    description,
                    sub_actions: Vec::new(),
                };
                self.write_action(action.id, ActionWrite::Create(action), cx);
            }
        }
    }

    pub(super) fn render_stats(&self, palette: &ForgePalette, density: Density) -> AnyElement {
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
        )
        .into_any_element()
    }

    pub(super) fn render_filter_left(
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

    pub(super) fn render_filter_right(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        primary_button_with_icon(Icon::Plus, tr!("actions_modal_new_action_title"), palette)
            .on_click(
                "actions-new",
                cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.open_action_modal(None, window, cx)
                }),
            )
            .into_any_element()
    }

    fn tree_action_rows(&self) -> Vec<TreeActionRow> {
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
            let caption = if self.loading {
                tr!("actions_tree_loading")
            } else {
                tr!("actions_empty")
            };
            rows.push(
                tree_notice(SharedString::from(caption), palette.text_faint, density)
                    .into_any_element(),
            );
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

        let focus = self.tree_keys.focus().clone();
        let inner = navigable_list(div().id("actions-tree"), &focus, cx)
            .track_scroll(&self.tree_scroll)
            .flex_1()
            .min_h(px(0.0))
            .py(spacing(Spacing::Xs, density))
            .overflow_y_scroll()
            .children(rows);

        let panel = div()
            .w(self.tree_width)
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
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_group(index, cx)))
            .child(icon(chevron, TREE_GLYPH, palette.text_muted))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(group.name.clone()),
            )
            .child(div().flex_1())
            .child(
                div()
                    .font_family(mono_family())
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
        let row_bg: Rgba = if selected || hovered || self.tree_keys.is_at(&id) {
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
                .font_family(mono_family())
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
            .font_family(mono_family())
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

    pub(super) fn render_row_context_menu(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = self.menu_open?;
        let position = self.menu_click_pos?;
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

    pub(super) fn render_action_modal(
        &self,
        form: &ActionForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let name_len = form
            .name
            .read(cx)
            .content()
            .chars()
            .count()
            .min(NAME_FIELD_LIMIT);
        let valid = form.ready(cx);

        let name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(div().flex_1().child(form.name.clone()))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(format!("{name_len}/{NAME_FIELD_LIMIT}")),
            );
        let name_section = modal_section(
            palette,
            tr!("actions_modal_section_name"),
            name_row,
            density,
        );

        let group_field = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(status_dot(palette.brand, GROUP_DOT))
            .child(div().flex_1().child(form.group.clone()));
        let group_section = modal_section(
            palette,
            tr!("actions_modal_section_group"),
            group_field,
            density,
        );

        let queue_control: AnyElement = if form.queues.is_empty() {
            div()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(SharedString::from(tr!("actions_loading_queues")))
                .into_any_element()
        } else {
            let mut queue_chips = div().flex().flex_wrap().gap(spacing(Spacing::Xxs, density));
            for (i, (_, name)) in form.queues.iter().enumerate() {
                queue_chips = queue_chips.child(
                    chip(
                        name.clone(),
                        ChipGlyph::Dot(palette.brand),
                        form.selected_queue == i,
                        palette,
                    )
                    .on_click(
                        SharedString::from(format!("actions-queue-{i}")),
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.select_queue(i, cx)),
                    ),
                );
            }
            queue_chips.into_any_element()
        };
        let queue_section = modal_section(
            palette,
            tr!("actions_modal_section_queue"),
            queue_control,
            density,
        );

        let two_col = div()
            .flex()
            .gap(spacing(Spacing::Sm, density))
            .child(div().flex_1().child(group_section))
            .child(div().flex_1().child(queue_section));

        let desc_section = modal_section(
            palette,
            tr!("actions_modal_section_description"),
            div().child(form.description.clone()),
            density,
        );

        let behavior_header = div()
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(SharedString::from(tr!("actions_modal_section_behavior")));

        let enabled = Self::modal_toggle_row(
            tr!("actions_modal_enabled_label"),
            tr!("actions_modal_enabled_desc"),
            form.enabled,
            palette.success,
            "actions-modal-enabled",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_enabled(cx)),
            palette,
            density,
        );
        let concurrent = Self::modal_toggle_row(
            tr!("actions_modal_concurrent_label"),
            tr!("actions_modal_concurrent_desc"),
            form.concurrent,
            palette.info,
            "actions-modal-concurrent",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_concurrent(cx)),
            palette,
            density,
        );
        let bypass = Self::modal_toggle_row(
            tr!("actions_modal_bypass_label"),
            tr!("actions_modal_bypass_desc"),
            form.bypass_pause,
            palette.warning,
            "actions-modal-bypass",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_bypass(cx)),
            palette,
            density,
        );
        let random = Self::modal_toggle_row(
            tr!("actions_modal_random_pick_label"),
            tr!("actions_modal_random_pick_desc"),
            form.random_pick,
            palette.danger,
            "actions-modal-random",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_random(cx)),
            palette,
            density,
        );

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(name_section)
            .child(two_col)
            .child(desc_section)
            .child(behavior_header)
            .child(enabled)
            .child(concurrent)
            .child(bypass)
            .child(random);

        let cancel = secondary_button(tr!("actions_modal_cancel_btn"), palette).on_click(
            "actions-modal-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_action_modal(cx)),
        );
        let editing = form.editing.is_some();
        let submit_label = if editing {
            tr!("actions_modal_save_btn")
        } else {
            tr!("actions_modal_create_btn")
        };
        let create = primary_button(submit_label, palette)
            .disabled(!valid)
            .on_click(
                "actions-modal-create",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit_action_modal(cx)),
            );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, density))
            .child(cancel)
            .child(create);

        let modal_title = if editing {
            tr!("actions_modal_edit_action_title")
        } else {
            tr!("actions_modal_new_action_title")
        };
        let card = modal(modal_title, body, palette)
            .size(ModalSize::Md)
            .footer(footer)
            .on_close(
                "actions-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_action_modal(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-modal-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_action_modal(cx));
            })
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "each argument is one input of the toggle row plus the active density"
    )]
    fn modal_toggle_row(
        label: impl Into<SharedString>,
        description: impl Into<SharedString>,
        on: bool,
        accent: Rgba,
        id: &'static str,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(label.into()),
                    )
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_muted)
                            .child(description.into()),
                    ),
            )
            .child(toggle(on, palette).on_color(accent).on_click(id, handler))
            .into_any_element()
    }

    pub(super) fn render_delete_confirm(
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use forge_components::ThemeId;
    use forge_runtime::{ActionCancelRegistry, QueueScheduler, spawn_action_engine};
    use forge_runtime::{TIMER_TICK_KIND, TimerTickDescriptor};
    use forge_soundboard::ClipLibrary;
    use forge_storage::MockActionRepo;
    use forge_storage::MockQueueRepo;
    use forge_storage::MockSoundboardClipsRepo;
    use forge_storage::MockTriggerInstanceRepo;
    use forge_storage::{DataProvider, SoundboardClipsRepo};
    use forge_types::{PermissionRung, PlatformScope, TriggerInstance, TriggerInstanceId};

    use super::*;
    use crate::test_support::{
        Declares, StubEventLog, StubHistory, StubOverlays, StubTrigger, pump, runtime,
        stub_catalog, test_backend, trigger_registry,
    };
    use forge_components::Presentation;

    const CHAT_KIND: &str = "twitch.chat";
    const REWARD_KIND: &str = "twitch.reward_redemption";

    struct Seed {
        name: &'static str,
        group: &'static str,
        trigger_kinds: &'static [&'static str],
    }

    fn action(seed: &Seed) -> Action {
        Action {
            id: ActionId::new(),
            name: seed.name.to_owned(),
            group: Some(seed.group.to_owned()),
            queue_id: QueueId::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: Default::default(),
            description: None,
            sub_actions: Vec::new(),
        }
    }

    fn instance(kind_id: &str) -> TriggerInstance {
        TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: kind_id.to_owned(),
            name: kind_id.to_owned(),
            overrides: Default::default(),
            enabled: true,
            user_defined: true,
            platform_scope: PlatformScope::Any,
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        }
    }

    fn registry() -> TriggerRegistry {
        let mut registry = trigger_registry(vec![
            StubTrigger::new(CHAT_KIND, "Chat", TriggerCategory::Chat, Declares::Nothing),
            StubTrigger::new(
                REWARD_KIND,
                "Reward",
                TriggerCategory::ChannelPoints,
                Declares::Nothing,
            ),
        ]);
        registry.register(Box::new(TimerTickDescriptor)).unwrap();
        registry
    }

    struct Fixture {
        view: Entity<ScreenActionsView>,
        actions: Vec<Action>,
        rt: tokio::runtime::Runtime,
    }

    impl Fixture {
        fn new(cx: &mut gpui::TestAppContext, seeds: &[Seed]) -> Self {
            cx.update(|cx| {
                cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            });
            let rt = runtime();
            let actions: Vec<Action> = seeds.iter().map(action).collect();
            let mut instances = Vec::new();
            let mut users: HashMap<TriggerInstanceId, Vec<ActionId>> = HashMap::new();
            let mut linked: HashMap<ActionId, Vec<TriggerInstance>> = HashMap::new();
            for (seed, action) in seeds.iter().zip(&actions) {
                for kind in seed.trigger_kinds {
                    let instance = instance(kind);
                    users.insert(instance.id, vec![action.id]);
                    linked.entry(action.id).or_default().push(instance.clone());
                    instances.push(instance);
                }
            }

            let mut action_repo = MockActionRepo::new();
            let listed = actions.clone();
            action_repo
                .expect_list()
                .returning(move || Ok(listed.clone()));
            let found = actions.clone();
            action_repo
                .expect_get()
                .returning(move |id| Ok(found.iter().find(|a| a.id == id).cloned()));
            action_repo
                .expect_telemetry()
                .returning(|_| Ok(forge_storage::ActionTelemetry::default()));
            let mut trigger_repo = MockTriggerInstanceRepo::new();
            trigger_repo
                .expect_list_for_action()
                .returning(move |id| Ok(linked.get(&id).cloned().unwrap_or_default()));
            trigger_repo
                .expect_list_all()
                .returning(move || Ok(instances.clone()));
            trigger_repo
                .expect_actions_by_instance()
                .returning(move || Ok(users.clone()));

            let action_repo: Arc<dyn ActionRepo> = Arc::new(action_repo);
            let mut queues = MockQueueRepo::new();
            queues.expect_list().returning(|| Ok(Vec::new()));
            let queue_repo: Arc<dyn QueueRepo> = Arc::new(queues);
            let trigger_repo: Arc<dyn TriggerInstanceRepo> = Arc::new(trigger_repo);
            let mut stored_clips = MockSoundboardClipsRepo::new();
            stored_clips.expect_list().returning(|| Ok(Vec::new()));
            let clips: Arc<dyn SoundboardClipsRepo> = Arc::new(stored_clips);
            let service = Arc::new(ActionsService::new(
                Arc::clone(&action_repo),
                Arc::clone(&queue_repo),
                Arc::new(StubHistory),
                Arc::clone(&trigger_repo),
            ));
            let (backend, _writes) = test_backend();
            let mut globals = forge_storage::MockGlobalsRepo::new();
            globals.expect_list().returning(|| Ok(Vec::new()));
            let (bus, scheduler) = rt.block_on(async {
                let bus = EventBus::new(Arc::new(StubEventLog));
                let engine = spawn_action_engine(
                    Arc::clone(&bus),
                    stub_catalog(),
                    Arc::new(crate::test_support::StubActions),
                    Arc::new(StubHistory),
                    Arc::new(SubActionRegistry::new()),
                    Arc::new(ActionCancelRegistry::new()),
                );
                let scheduler = QueueScheduler::spawn(engine, Arc::clone(&bus), Vec::new());
                (bus, scheduler)
            });
            let handle = rt.handle().clone();
            let view = cx.update(|cx| {
                cx.new(|cx| {
                    ScreenActionsView::new(
                        action_repo,
                        queue_repo,
                        service,
                        trigger_repo,
                        Arc::new(crate::test_support::MemoryScripts::default())
                            as Arc<dyn ScriptRepo>,
                        Arc::new(ClipLibrary::new(clips, backend.media_repo())),
                        Arc::new(globals) as Arc<dyn GlobalsRepo>,
                        Arc::clone(&backend) as Arc<dyn SettingsRepo>,
                        Arc::new(StubOverlays),
                        Arc::new(OverlayKindRegistry::new()),
                        None,
                        None,
                        Arc::new(SubActionRegistry::new()),
                        Arc::new(registry()),
                        handle,
                        bus,
                        scheduler,
                        None,
                        cx,
                    )
                })
            });
            let fixture = Self { view, actions, rt };
            fixture.settle(cx);
            fixture
        }

        fn settle(&self, cx: &mut gpui::TestAppContext) {
            for _ in 0..3 {
                pump(&self.rt);
                cx.run_until_parked();
            }
        }

        fn set_filter(&self, cx: &mut gpui::TestAppContext, filter: ActionsFilter) {
            self.view.update(cx, |view, cx| view.set_filter(filter, cx));
        }

        fn visible(&self, cx: &mut gpui::TestAppContext, name: &str) -> bool {
            self.view.read_with(cx, |view, _| {
                let action = view
                    .groups
                    .iter()
                    .flat_map(|g| g.actions.iter())
                    .find(|a| a.name == name)
                    .unwrap();
                view.action_passes(action)
            })
        }

        fn sync_detail(&self, cx: &mut gpui::TestAppContext, name: &str, kinds: &[&str]) {
            let action = self
                .actions
                .iter()
                .find(|a| a.name == name)
                .unwrap()
                .clone();
            let detail = ActionDetail {
                action,
                trigger_instances: kinds.iter().map(|kind| instance(kind)).collect(),
                sub_action_avg_ms: Vec::new(),
                last_step_outcomes: Vec::new(),
            };
            self.view
                .update(cx, |view, _| view.sync_filter_membership(&detail));
        }
    }

    struct Hosting(Entity<ScreenActionsView>);

    impl Render for Hosting {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(self.0.clone())
        }
    }

    impl Fixture {
        fn in_window(&self, cx: &mut gpui::TestAppContext) -> gpui::VisualTestContext {
            cx.update(crate::actions::register_key_bindings);
            let hosted = self.view.clone();
            let window = cx.add_window(|_, _| Hosting(hosted));
            let vcx = gpui::VisualTestContext::from_window(window.into(), cx);
            vcx.run_until_parked();
            vcx
        }

        fn collapse(&self, cx: &mut gpui::TestAppContext, group: &str) {
            self.view.update(cx, |view, cx| {
                let name = group.to_uppercase();
                let index = view.groups.iter().position(|g| g.name == name).unwrap();
                view.toggle_group(index, cx);
            });
        }

        fn search(&self, cx: &mut gpui::TestAppContext, query: &str) {
            self.view.update(cx, |view, cx| {
                view.search
                    .on_changed(&InputEvent::Changed(query.to_owned().into()));
                cx.notify();
            });
        }

        fn name_of(&self, id: ActionId) -> &str {
            let found = self.actions.iter().find(|a| a.id == id);
            &found.unwrap().name
        }

        fn press(&self, vcx: &mut gpui::VisualTestContext, keys: &str) {
            vcx.simulate_keystrokes(keys);
            vcx.run_until_parked();
        }

        fn cursor_name(&self, vcx: &mut gpui::VisualTestContext) -> Option<String> {
            let ids: Vec<ActionId> = self.actions.iter().map(|a| a.id).collect();
            self.view.read_with(vcx, |view, _| {
                let at = ids.iter().find(|id| view.tree_keys.is_at(id));
                at.map(|id| self.name_of(*id).to_owned())
            })
        }

        fn selected_name(&self, vcx: &mut gpui::VisualTestContext) -> Option<String> {
            let selected = self.view.read_with(vcx, |view, _| view.selected);
            selected.map(|id| self.name_of(id).to_owned())
        }

        fn keyboard_walk(&self, vcx: &mut gpui::VisualTestContext, rows: usize) -> Vec<String> {
            (0..rows)
                .map(|_| {
                    self.press(vcx, "down");
                    self.cursor_name(vcx).unwrap()
                })
                .collect()
        }

        fn click_row(&self, vcx: &mut gpui::VisualTestContext, position: usize) {
            let bounds = self.view.read_with(vcx, |view, _| {
                let child = view.tree_action_rows()[position].child_index;
                view.tree_scroll.bounds_for_item(child).unwrap()
            });
            let name_area = gpui::point(bounds.left() + ROW_INDENT, bounds.center().y);
            vcx.simulate_click(name_area, gpui::Modifiers::none());
            self.settle(vcx);
        }

        fn clicked_walk(&self, vcx: &mut gpui::VisualTestContext) -> Vec<String> {
            let rows = self
                .view
                .read_with(vcx, |view, _| view.tree_action_rows().len());
            (0..rows)
                .map(|position| {
                    self.click_row(vcx, position);
                    self.selected_name(vcx).unwrap()
                })
                .collect()
        }
    }

    fn label(filter: ActionsFilter) -> &'static str {
        match filter {
            ActionsFilter::All => "all",
            ActionsFilter::Chat => "chat",
            ActionsFilter::Timers => "timers",
            ActionsFilter::Points => "points",
        }
    }

    fn seeds() -> Vec<Seed> {
        vec![
            Seed {
                name: "Hydrate",
                group: "Chat Commands",
                trigger_kinds: &[TIMER_TICK_KIND],
            },
            Seed {
                name: "Lurk",
                group: "Chat Commands",
                trigger_kinds: &[CHAT_KIND],
            },
            Seed {
                name: "Raid train",
                group: "Timers",
                trigger_kinds: &[],
            },
            Seed {
                name: "Stretch",
                group: "Misc",
                trigger_kinds: &[TIMER_TICK_KIND],
            },
            Seed {
                name: "Hug",
                group: "Misc",
                trigger_kinds: &[REWARD_KIND],
            },
            Seed {
                name: "Shoutout",
                group: "Misc",
                trigger_kinds: &[CHAT_KIND],
            },
            Seed {
                name: "Free hug",
                group: "Channel Points",
                trigger_kinds: &[],
            },
            Seed {
                name: "Spin the wheel",
                group: "Misc",
                trigger_kinds: &[TIMER_TICK_KIND, REWARD_KIND],
            },
        ]
    }

    #[gpui::test]
    fn trigger_filters_show_an_action_with_a_matching_trigger_outside_the_filter_group(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::new(cx, &seeds());
        for (filter, name) in [
            (ActionsFilter::Chat, "Shoutout"),
            (ActionsFilter::Timers, "Hydrate"),
            (ActionsFilter::Points, "Hug"),
        ] {
            fixture.set_filter(cx, filter);
            assert!(fixture.visible(cx, name), "{name} under {}", label(filter));
        }
    }

    #[gpui::test]
    fn trigger_filters_hide_an_action_without_a_trigger_of_their_category(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::new(cx, &seeds());
        for (filter, name) in [
            (ActionsFilter::Chat, "Hug"),
            (ActionsFilter::Chat, "Stretch"),
            (ActionsFilter::Timers, "Lurk"),
            (ActionsFilter::Timers, "Hug"),
            (ActionsFilter::Points, "Lurk"),
            (ActionsFilter::Points, "Hydrate"),
        ] {
            fixture.set_filter(cx, filter);
            assert!(!fixture.visible(cx, name), "{name} under {}", label(filter));
        }
    }

    #[gpui::test]
    fn an_action_with_timer_and_reward_triggers_shows_under_both_filters(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::new(cx, &seeds());
        for filter in [ActionsFilter::Timers, ActionsFilter::Points] {
            fixture.set_filter(cx, filter);
            assert!(fixture.visible(cx, "Spin the wheel"), "{}", label(filter));
        }
    }

    #[gpui::test]
    fn a_group_named_after_a_filter_puts_no_action_under_it_without_a_trigger(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::new(cx, &seeds());
        for (filter, name) in [
            (ActionsFilter::Timers, "Raid train"),
            (ActionsFilter::Chat, "Hydrate"),
            (ActionsFilter::Points, "Free hug"),
        ] {
            fixture.set_filter(cx, filter);
            assert!(!fixture.visible(cx, name), "{name} under {}", label(filter));
        }
    }

    #[gpui::test]
    fn removing_the_last_matching_trigger_drops_the_action_from_its_filter(
        cx: &mut gpui::TestAppContext,
    ) {
        for (filter, name) in [
            (ActionsFilter::Timers, "Hydrate"),
            (ActionsFilter::Points, "Hug"),
        ] {
            let fixture = Fixture::new(cx, &seeds());
            fixture.set_filter(cx, filter);
            fixture.sync_detail(cx, name, &[CHAT_KIND]);
            assert!(!fixture.visible(cx, name), "{name} under {}", label(filter));
        }
    }

    #[gpui::test]
    fn removing_one_trigger_kind_keeps_the_action_under_the_other_filter(
        cx: &mut gpui::TestAppContext,
    ) {
        for (remaining, kept, dropped) in [
            (
                TIMER_TICK_KIND,
                ActionsFilter::Timers,
                ActionsFilter::Points,
            ),
            (REWARD_KIND, ActionsFilter::Points, ActionsFilter::Timers),
        ] {
            let fixture = Fixture::new(cx, &seeds());
            fixture.sync_detail(cx, "Spin the wheel", &[remaining]);
            fixture.set_filter(cx, kept);
            assert!(
                fixture.visible(cx, "Spin the wheel"),
                "kept {}",
                label(kept)
            );
            fixture.set_filter(cx, dropped);
            assert!(
                !fixture.visible(cx, "Spin the wheel"),
                "dropped {}",
                label(dropped)
            );
        }
    }

    #[gpui::test]
    fn adding_timer_and_reward_triggers_puts_the_action_into_both_filters(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::new(cx, &seeds());
        fixture.sync_detail(cx, "Lurk", &[CHAT_KIND, TIMER_TICK_KIND, REWARD_KIND]);
        for filter in [ActionsFilter::Timers, ActionsFilter::Points] {
            fixture.set_filter(cx, filter);
            assert!(fixture.visible(cx, "Lurk"), "{}", label(filter));
        }
    }

    #[test]
    fn actions_by_trigger_filter_buckets_actions_per_filter_and_skips_unmapped_kinds() {
        let timer_a = instance(TIMER_TICK_KIND);
        let timer_b = instance(TIMER_TICK_KIND);
        let reward = instance(REWARD_KIND);
        let chat = instance(CHAT_KIND);
        let (first, second, redeemer, chatter) = (
            ActionId::new(),
            ActionId::new(),
            ActionId::new(),
            ActionId::new(),
        );
        let users: HashMap<TriggerInstanceId, Vec<ActionId>> = HashMap::from([
            (timer_a.id, vec![first]),
            (timer_b.id, vec![second]),
            (reward.id, vec![first, redeemer]),
            (chat.id, vec![chatter]),
        ]);
        let instances = vec![timer_a, timer_b, reward, chat];
        let mut repo = MockTriggerInstanceRepo::new();
        repo.expect_list_all()
            .returning(move || Ok(instances.clone()));
        repo.expect_actions_by_instance()
            .returning(move || Ok(users.clone()));
        let filter_by_kind = HashMap::from([
            (TIMER_TICK_KIND.to_owned(), ActionsFilter::Timers),
            (REWARD_KIND.to_owned(), ActionsFilter::Points),
        ]);

        let members = runtime()
            .block_on(actions_by_trigger_filter(&repo, &filter_by_kind))
            .unwrap();

        let expected = HashMap::from([
            (ActionsFilter::Timers, HashSet::from([first, second])),
            (ActionsFilter::Points, HashSet::from([first, redeemer])),
        ]);
        assert!(members == expected);
    }

    #[gpui::test]
    fn the_delete_note_reflects_whether_pending_scheduled_runs_are_known(
        cx: &mut gpui::TestAppContext,
    ) {
        let fixture = Fixture::new(cx, &seeds());
        let id = fixture.actions[0].id;
        let other = ActionId::new();
        let repo: Arc<dyn forge_storage::ScheduledRunRepo> =
            Arc::new(forge_storage::MockScheduledRunRepo::new());

        for (language, unknown) in [
            (
                forge_storage::Language::En,
                "Any pending scheduled runs of this action will be cancelled too.",
            ),
            (
                forge_storage::Language::Uk,
                "Також буде скасовано всі її заплановані запуски, що очікують.",
            ),
        ] {
            crate::i18n::install_language(language);
            let note =
                |with_repo: bool, count: Option<(ActionId, u64)>, cx: &mut gpui::TestAppContext| {
                    fixture.view.update(cx, |view, _| {
                        view.scheduled_runs = with_repo.then(|| Arc::clone(&repo));
                        view.delete_scheduled_count = count;
                        view.delete_scheduled_note(id)
                    })
                };
            let expected_unknown = Some(unknown.to_owned());
            assert_eq!(
                note(true, None, cx),
                expected_unknown,
                "{language:?} no count yet"
            );
            assert_eq!(
                note(true, Some((other, 3)), cx),
                expected_unknown,
                "{language:?} count belongs to another action"
            );
            assert_eq!(
                note(true, Some((id, 0)), cx),
                None,
                "{language:?} known zero"
            );
            assert_eq!(note(false, None, cx), None, "{language:?} no repo");
            assert!(
                note(true, Some((id, 3)), cx).is_some_and(|text| text != unknown),
                "{language:?} known count"
            );
        }
    }

    #[gpui::test]
    fn the_editor_pickers_star_into_the_app_wide_favorites_copies(cx: &mut gpui::TestAppContext) {
        use crate::picker_favorites::{SUB_ACTION_FAVORITES, StoredFavorites, TRIGGER_FAVORITES};
        let fixture = Fixture::new(cx, &[]);
        let (elsewhere, _writes) = test_backend();
        let handle = fixture.rt.handle().clone();

        let app_wide = cx.update(|cx| {
            let repo = elsewhere as Arc<dyn SettingsRepo>;
            (
                StoredFavorites::shared(SUB_ACTION_FAVORITES, Arc::clone(&repo), &handle, cx)
                    .entity_id(),
                StoredFavorites::shared(TRIGGER_FAVORITES, repo, &handle, cx).entity_id(),
            )
        });

        let own = fixture.view.read_with(cx, |view, _| {
            (
                view.sub_action_favorites.entity_id(),
                view.trigger_favorites.entity_id(),
            )
        });
        assert_eq!(own, app_wide);
    }

    #[gpui::test]
    fn arrows_walk_the_tree_in_the_order_it_renders_past_collapsed_groups_and_filters(
        cx: &mut gpui::TestAppContext,
    ) {
        let all = [
            "Free hug",
            "Hydrate",
            "Lurk",
            "Hug",
            "Shoutout",
            "Spin the wheel",
            "Stretch",
            "Raid train",
        ];
        for (filter, query, collapsed, expected) in [
            (ActionsFilter::All, "", None, all.to_vec()),
            (
                ActionsFilter::All,
                "",
                Some("Misc"),
                vec!["Free hug", "Hydrate", "Lurk", "Raid train"],
            ),
            (
                ActionsFilter::Timers,
                "",
                None,
                vec!["Hydrate", "Spin the wheel", "Stretch"],
            ),
            (
                ActionsFilter::Timers,
                "",
                Some("Chat Commands"),
                vec!["Spin the wheel", "Stretch"],
            ),
            (ActionsFilter::All, "hu", None, vec!["Free hug", "Hug"]),
        ] {
            let fixture = Fixture::new(cx, &seeds());
            let vcx = &mut fixture.in_window(cx);
            fixture.set_filter(vcx, filter);
            fixture.search(vcx, query);
            if let Some(group) = collapsed {
                fixture.collapse(vcx, group);
            }
            vcx.run_until_parked();

            assert_eq!(
                fixture.keyboard_walk(vcx, expected.len()),
                expected,
                "keys under {} {query:?} {collapsed:?}",
                label(filter)
            );
            assert_eq!(
                fixture.clicked_walk(vcx),
                expected,
                "rendered rows under {} {query:?} {collapsed:?}",
                label(filter)
            );
        }
    }

    #[gpui::test]
    fn enter_on_the_cursor_row_opens_the_action_a_click_on_that_row_opens(
        cx: &mut gpui::TestAppContext,
    ) {
        let opened = |fixture: &Fixture, vcx: &mut gpui::VisualTestContext| {
            fixture.settle(vcx);
            let detail = fixture.view.read_with(vcx, |view, _| {
                view.detail.as_ref().map(|detail| detail.action.id)
            });
            (
                fixture.selected_name(vcx),
                detail.map(|id| fixture.name_of(id).to_owned()),
            )
        };

        let by_key = Fixture::new(cx, &seeds());
        let vcx = &mut by_key.in_window(cx);
        by_key.press(vcx, "down down down enter");
        let entered = opened(&by_key, vcx);

        let by_mouse = Fixture::new(cx, &seeds());
        let vcx = &mut by_mouse.in_window(cx);
        by_mouse.click_row(vcx, 2);
        let clicked = opened(&by_mouse, vcx);

        let lurk = Some("Lurk".to_owned());
        assert_eq!(entered, (lurk.clone(), lurk));
        assert_eq!(entered, clicked);
    }

    #[gpui::test]
    fn a_row_that_stops_being_listed_takes_the_cursor_with_it(cx: &mut gpui::TestAppContext) {
        type Hide = fn(&Fixture, &mut gpui::VisualTestContext);
        let hides: [(&str, Hide); 3] = [
            ("collapse", |fixture, vcx| {
                fixture.collapse(vcx, "Chat Commands")
            }),
            ("filter", |fixture, vcx| {
                fixture.set_filter(vcx, ActionsFilter::Points)
            }),
            ("search", |fixture, vcx| fixture.search(vcx, "raid")),
        ];
        for (how, hide) in hides {
            let fixture = Fixture::new(cx, &seeds());
            let vcx = &mut fixture.in_window(cx);
            fixture.press(vcx, "down down");
            assert_eq!(fixture.cursor_name(vcx).as_deref(), Some("Hydrate"));

            hide(&fixture, vcx);
            vcx.run_until_parked();
            fixture.press(vcx, "enter");
            assert_eq!(fixture.selected_name(vcx), None, "{how}");

            let first = fixture
                .view
                .read_with(vcx, |view, _| view.tree_action_rows()[0].id);
            fixture.press(vcx, "down");
            assert_eq!(
                fixture.cursor_name(vcx).as_deref(),
                Some(fixture.name_of(first)),
                "{how}"
            );
        }
    }

    #[gpui::test]
    fn an_inline_rename_silences_the_list_keys(cx: &mut gpui::TestAppContext) {
        let fixture = Fixture::new(cx, &seeds());
        let vcx = &mut fixture.in_window(cx);
        fixture.press(vcx, "down");
        let renamed = fixture.actions[0].id;
        fixture.view.update_in(vcx, |view, window, cx| {
            view.start_rename(renamed, window, cx);
            view.step_list(ListStep::Next, cx);
            view.activate_list(window, cx);
        });
        vcx.run_until_parked();

        assert_eq!(fixture.cursor_name(vcx).as_deref(), Some("Free hug"));
        assert_eq!(fixture.selected_name(vcx), None);
    }
}
