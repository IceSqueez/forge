//! Actions screen — list pane: page header, filter chips, search, the group
//! headers and tree rows, the row overflow menu, inline rename, the add-action
//! modal and the delete confirm.

use super::*;
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;
use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ChipGlyph, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY,
    Density, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, MenuPlacement, ModalSize,
    OverlayPosition, Radius, Spacing, TextArea, TextInput, ToastAction, ToastKind, breadcrumb,
    chip, confirm_modal, icon, menu_button, menu_divider, menu_item, modal, overlay,
    primary_button, primary_button_with_icon, secondary_button, spacing, status_dot, toggle,
};
use forge_types::{Action, ActionId, ExecutionMode, Queue};
use gpui::{AnyElement, App, ClickEvent, Context, Entity, Rgba, SharedString, Window, div, px};
use std::sync::Arc;
use std::time::Duration;

/// A left-panel notice line (loading / empty), inked `color`, padded like a group
/// header.
fn tree_notice(label: &'static str, color: Rgba, _palette: &ForgePalette) -> impl IntoElement {
    div()
        .w_full()
        .px(TREE_GUTTER)
        .py(spacing(Spacing::Sm, Density::Cozy))
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_XS)
        .text_color(color)
        .child(label)
}

/// A modal form block: an uppercase mono caption over `control`.
fn modal_section(
    palette: &ForgePalette,
    label: &'static str,
    control: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(label),
        )
        .child(control)
}

impl ScreenActionsView {
    // --- pure lookup helpers ----------------------------------------------

    pub(super) fn find(&self, id: ActionId) -> Option<&ActionSummary> {
        self.groups
            .iter()
            .flat_map(|g| g.actions.iter())
            .find(|a| a.id == id)
    }

    fn total_actions(&self) -> usize {
        self.groups.iter().map(|g| g.actions.len()).sum()
    }

    /// Whether a filter tab admits a group's category. `Other` shows only under
    /// `All`.
    fn category_visible(filter: ActionsFilter, category: ActionCategory) -> bool {
        match filter {
            ActionsFilter::All => true,
            ActionsFilter::Chat => category == ActionCategory::Chat,
            ActionsFilter::Timers => category == ActionCategory::Timers,
            ActionsFilter::Points => category == ActionCategory::Points,
        }
    }

    /// A row survives the current filter + search. Combining both here reproduces the
    /// source's per-action gate: a group in a hidden category yields no surviving rows
    /// and is skipped whole.
    fn action_passes(&self, group: &ActionGroup, action: &ActionSummary) -> bool {
        if !Self::category_visible(self.filter, group.category) {
            return false;
        }
        if self.search.is_empty() {
            return true;
        }
        action
            .name
            .to_lowercase()
            .contains(&self.search.to_lowercase())
    }

    // --- interaction handlers ---------------------------------------------

    pub(super) fn on_search_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Changed(text) = event {
            self.search = text.to_string();
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

    fn select(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.detail = None;
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

    fn toggle_menu(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.menu_open = if self.menu_open == Some(id) {
            None
        } else {
            Some(id)
        };
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
    }

    /// Persists a new enabled state: loads the action, flips the flag, saves it, then
    /// reconciles the tree with a full re-pull.
    fn set_enabled(&mut self, id: ActionId, enabled: bool, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
        let repo = Arc::clone(&self.action_repo);
        self.spawn_reload(
            async move {
                let mut action = repo
                    .get(id)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "action not found".to_owned())?;
                action.enabled = enabled;
                repo.save(&action).await.map_err(|e| e.to_string())?;
                repo.list().await.map_err(|e| e.to_string())
            },
            cx,
        );
    }

    /// Duplicates the action into a fresh persisted row (`… (copy)`), then reconciles
    /// the tree with a full re-pull so the copy lands with its real [`ActionId`].
    pub(super) fn duplicate(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
        let repo = Arc::clone(&self.action_repo);
        let new_id = ActionId::new();
        self.spawn_reload(
            async move {
                let source = repo
                    .get(id)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "source action not found".to_owned())?;
                let new_name = format!("{} (copy)", source.name);
                repo.duplicate(id, new_id, &new_name)
                    .await
                    .map_err(|e| e.to_string())?;
                repo.list().await.map_err(|e| e.to_string())
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
        let field = cx.new(|cx| {
            let mut input = TextInput::new("Name", cx)
                .with_palette(palette)
                .static_chrome(palette.brand, Radius::Sm);
            input.set_content(seed, cx);
            input
        });
        field.read(cx).focus(window);
        let sub = cx.subscribe(&field, Self::on_rename_event);
        self.menu_open = None;
        self.renaming = Some(Renaming {
            id,
            field,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_rename_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Submitted(text) => self.commit_rename(text.to_string(), cx),
            InputEvent::Cancelled => {
                self.renaming = None;
                cx.notify();
            }
            InputEvent::Changed(_) => {}
        }
    }

    /// Persists an inline rename: loads the action, writes the new name, saves it, then
    /// reconciles with a full re-pull. Guards against a blank name and a case-insensitive
    /// collision with another action (raising a toast rather than writing a duplicate).
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
            .any(|a| a.id != id && a.name.eq_ignore_ascii_case(&trimmed));
        if taken {
            cx.push_toast(
                ToastKind::Error,
                format!("Name \u{201c}{trimmed}\u{201d} is already taken"),
            );
            return;
        }
        let repo = Arc::clone(&self.action_repo);
        self.spawn_reload(
            async move {
                let mut action = repo
                    .get(id)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "action not found".to_owned())?;
                action.name = trimmed;
                repo.save(&action).await.map_err(|e| e.to_string())?;
                repo.list().await.map_err(|e| e.to_string())
            },
            cx,
        );
    }

    // --- delete (two-phase confirm) ---------------------------------------

    fn request_delete(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.pending_delete = Some(id);
        self.menu_open = None;
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete = None;
        cx.notify();
    }

    /// Soft-deletes the confirmed action: archives it (the row and its telemetry
    /// survive, invisible to `list`), re-pulls, then raises an undo toast whose action
    /// restores it through the same reconcile path.
    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.pending_delete.take() else {
            return;
        };
        let name = self.find(id).map(|a| a.name.clone()).unwrap_or_default();
        cx.notify();

        let repo = Arc::clone(&self.action_repo);
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<Action>, String>>();
        self.rt_handle.spawn(async move {
            let outcome = async {
                repo.archive(id).await.map_err(|e| e.to_string())?;
                repo.list().await.map_err(|e| e.to_string())
            }
            .await;
            let _ = tx.send(outcome);
        });

        let restore_repo = Arc::clone(&self.action_repo);
        let restore_rt = self.rt_handle.clone();
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(actions)) => {
                let _ = this.update(cx, |this, cx| {
                    this.apply_actions(actions, cx);
                    this.raise_undo_toast(id, name, restore_repo, restore_rt, cx);
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    /// Fires the post-archive undo toast. Its action restores the archived action
    /// (still present, only marked archived) and reconciles the tree with a fresh pull.
    fn raise_undo_toast(
        &self,
        id: ActionId,
        name: String,
        repo: Arc<dyn ActionRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let message = format!("Deleted \u{201c}{name}\u{201d}");
        cx.push_toast_full(
            ToastKind::Undo,
            message,
            None,
            Some(ToastAction::new("Undo", move |_window, app: &mut App| {
                let repo = Arc::clone(&repo);
                let rt_handle = rt_handle.clone();
                Self::reload_entity(
                    view.clone(),
                    rt_handle,
                    async move {
                        repo.restore(id).await.map_err(|e| e.to_string())?;
                        repo.list().await.map_err(|e| e.to_string())
                    },
                    app,
                );
            })),
            Duration::from_millis(6000),
        );
    }

    // --- add-action modal -------------------------------------------------

    fn open_add_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let name = cx.new(|cx| TextInput::new("My automation", cx).with_palette(palette));
        let group = cx.new(|cx| TextInput::new("Examples", cx).with_palette(palette));
        let description = cx.new(|cx| {
            TextArea::new("Plays a sound, shows overlay alert…", cx).with_palette(palette)
        });
        name.read(cx).focus(window);
        let name_sub = cx.subscribe(&name, |_this, _f, _e: &InputEvent, cx| cx.notify());
        self.add_modal = Some(AddActionForm {
            name,
            group,
            description,
            queues: Vec::new(),
            selected_queue: 0,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            random_pick: false,
            _name_sub: name_sub,
        });
        cx.notify();

        // A new action must carry a real queue, so the QUEUE picker is filled from the
        // queue repo. Until this pull lands the section shows a loading caption and
        // Create stays disabled.
        let repo = Arc::clone(&self.queue_repo);
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<Queue>, String>>();
        self.rt_handle.spawn(async move {
            let _ = tx.send(repo.list().await.map_err(|e| e.to_string()));
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(queues)) => {
                let _ = this.update(cx, |this, cx| this.apply_queue_options(queues, cx));
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    /// Fills the open add-modal's QUEUE picker with the pulled queues. A no-op if the
    /// modal was dismissed before the pull returned.
    fn apply_queue_options(&mut self, queues: Vec<Queue>, cx: &mut Context<Self>) {
        if let Some(form) = self.add_modal.as_mut() {
            form.queues = queues
                .into_iter()
                .map(|q| (q.id, SharedString::from(q.name)))
                .collect();
            form.selected_queue = 0;
            cx.notify();
        }
    }

    fn cancel_add_modal(&mut self, cx: &mut Context<Self>) {
        self.add_modal = None;
        cx.notify();
    }

    fn select_queue(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(form) = self.add_modal.as_mut() {
            form.selected_queue = index;
            cx.notify();
        }
    }

    fn toggle_modal_enabled(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.add_modal.as_mut() {
            form.enabled = !form.enabled;
            cx.notify();
        }
    }

    fn toggle_modal_concurrent(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.add_modal.as_mut() {
            form.concurrent = !form.concurrent;
            cx.notify();
        }
    }

    fn toggle_modal_bypass(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.add_modal.as_mut() {
            form.bypass_pause = !form.bypass_pause;
            cx.notify();
        }
    }

    fn toggle_modal_random(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.add_modal.as_mut() {
            form.random_pick = !form.random_pick;
            cx.notify();
        }
    }

    /// Persists a new action built from the modal, then reconciles the tree with a full
    /// re-pull and selects the freshly-saved row by its real [`ActionId`]. No-op while
    /// the name is blank or before the queue picker has loaded a real queue.
    fn submit_add_modal(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.add_modal.as_ref() else {
            return;
        };
        let name = form.name.read(cx).content().trim().to_owned();
        if name.is_empty() {
            return;
        }
        let Some(&(queue_id, _)) = form.queues.get(form.selected_queue) else {
            cx.push_toast(ToastKind::Error, "No queue available".to_owned());
            return;
        };
        let group_name = form.group.read(cx).content().trim().to_owned();
        let description = form.description.read(cx).content().trim().to_owned();
        let execution_mode = if form.random_pick {
            ExecutionMode::RandomPick
        } else {
            ExecutionMode::Sequential
        };
        let action = Action {
            id: ActionId::new(),
            name,
            group: (!group_name.is_empty()).then_some(group_name),
            queue_id,
            enabled: form.enabled,
            concurrent: form.concurrent,
            bypass_pause: form.bypass_pause,
            execution_mode,
            description: (!description.is_empty()).then_some(description),
            sub_actions: Vec::new(),
        };
        let new_id = action.id;
        self.add_modal = None;
        cx.notify();

        let repo = Arc::clone(&self.action_repo);
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<Action>, String>>();
        self.rt_handle.spawn(async move {
            let outcome = async {
                repo.save(&action).await.map_err(|e| e.to_string())?;
                repo.list().await.map_err(|e| e.to_string())
            }
            .await;
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(actions)) => {
                let _ = this.update(cx, |this, cx| {
                    this.apply_actions(actions, cx);
                    this.select(new_id, cx);
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    // --- render: page header ----------------------------------------------

    pub(super) fn render_header(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                chip(
                    "All",
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
                    "Chat",
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
                    "Timers",
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
                    "Points",
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
            .w(HEADER_DIV_W)
            .h(HEADER_DIV_H)
            .bg(palette.border_regular);

        let search = div().w(SEARCH_W).child(self.search_field.clone());

        let new_btn = primary_button_with_icon(Icon::Plus, "New action", palette).on_click(
            "actions-new",
            cx.listener(|this, _: &ClickEvent, window, cx| this.open_add_modal(window, cx)),
        );

        let cluster = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(chips)
            .child(divider)
            .child(search)
            .child(new_btn);

        breadcrumb(
            vec![
                BreadcrumbCrumb::leaf("Automation"),
                BreadcrumbCrumb::leaf("Actions"),
            ],
            palette,
        )
        .right(cluster)
        .into_any_element()
    }

    // --- render: left tree ------------------------------------------------

    pub(super) fn render_tree(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let mut col = div().flex().flex_col();

        if self.total_actions() == 0 {
            let caption = if self.loading {
                "Loading actions…"
            } else {
                "No actions yet"
            };
            col = col.child(tree_notice(caption, palette.text_faint, palette));
        } else {
            for (index, group) in self.groups.iter().enumerate() {
                let surviving: Vec<&ActionSummary> = group
                    .actions
                    .iter()
                    .filter(|a| self.action_passes(group, a))
                    .collect();
                if surviving.is_empty() {
                    continue;
                }
                col = col.child(self.render_group_header(index, group, palette, cx));
                if !group.collapsed {
                    for action in surviving {
                        col = col.child(self.render_row(action, palette, cx));
                    }
                }
            }
        }

        div()
            .id("actions-tree")
            .w(LEFT_PANEL_W)
            .flex_none()
            .h_full()
            .py(spacing(Spacing::Xs, Density::Cozy))
            .bg(palette.shell)
            .border_r(BORDER_THIN)
            .border_color(palette.border_regular)
            .overflow_y_scroll()
            .child(col)
            .into_any_element()
    }

    fn render_group_header(
        &self,
        index: usize,
        group: &ActionGroup,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chevron = if group.collapsed {
            Icon::ChevronRight
        } else {
            Icon::ChevronDown
        };
        let hover_bg = palette.elevated;
        div()
            .id(SharedString::from(format!("actions-group-{index}")))
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .px(TREE_GUTTER)
            .py(px(6.0))
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_group(index, cx)))
            .child(icon(chevron, TREE_GLYPH, palette.text_muted))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(group.name.clone()),
            )
            .child(div().flex_1())
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(group.actions.len().to_string()),
            )
            .into_any_element()
    }

    fn render_row(
        &self,
        action: &ActionSummary,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = action.id;
        let selected = self.selected == Some(id);
        let hovered = self.hovered == Some(id);
        let menu_open = self.menu_open == Some(id);
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
        let row_bg: Rgba = if selected || hovered {
            palette.elevated
        } else {
            gpui::transparent_black().into()
        };

        // The name column: an inline rename field while this row is being renamed,
        // otherwise the (clipped, non-wrapping) name label.
        let name_el: AnyElement = match renaming {
            Some(renaming) => div()
                .flex_1()
                .min_w(px(0.0))
                .child(renaming.field.clone())
                .into_any_element(),
            None => div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(name_color)
                .child(action.name.clone())
                .into_any_element(),
        };

        // The select area: state icon + name, indented, filling the row's free width.
        // A rename field swallows its own clicks, so selecting is disabled mid-rename.
        let mut select_area = div()
            .id(SharedString::from(format!("actions-row-select-{id}")))
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .pl(ROW_INDENT)
            .pr(px(8.0))
            .child(icon(state_icon, TREE_GLYPH, state_color))
            .child(name_el);
        if renaming.is_none() {
            select_area = select_area
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select(id, cx)));
        }

        // The right slot: the "N sub" count, swapped for the `⋮` overflow menu while
        // the row is hovered or its menu is open, inside a fixed 46px slot so the edge
        // never shifts.
        let show_menu = hovered || menu_open;
        let slot_inner: AnyElement = if show_menu {
            self.render_row_menu(action, menu_open, palette, cx)
        } else {
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(format!("{} sub", action.sub_action_count))
                .into_any_element()
        };
        let right_slot = div().pr(ROW_GUTTER).child(
            div()
                .w(RIGHT_SLOT_W)
                .flex()
                .items_center()
                .justify_end()
                .child(slot_inner),
        );

        div()
            .id(SharedString::from(format!("actions-row-{id}")))
            .w_full()
            .h(ROW_HEIGHT)
            .flex()
            .items_center()
            .bg(row_bg)
            .on_hover(
                cx.listener(move |this, hovered: &bool, _, cx| this.set_hover(id, *hovered, cx)),
            )
            .child(div().w(STRIPE_W).h_full().bg(stripe_color))
            .child(select_area)
            .child(right_slot)
            .into_any_element()
    }

    fn render_row_menu(
        &self,
        action: &ActionSummary,
        menu_open: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = action.id;
        let toggle_label = if action.enabled { "Disable" } else { "Enable" };
        let next_enabled = !action.enabled;
        let view = cx.entity();

        menu_button(Icon::DotsVertical, menu_open, palette)
            .placement(MenuPlacement::BottomRight)
            .items(vec![
                menu_item(
                    SharedString::from(format!("actions-menu-rename-{id}")),
                    "Rename…",
                    cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.start_rename(id, window, cx)
                    }),
                )
                .icon(Icon::Pencil)
                .into(),
                menu_item(
                    SharedString::from(format!("actions-menu-dup-{id}")),
                    "Duplicate",
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
                    "Delete…",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(id, cx)),
                )
                .icon(Icon::Eraser)
                .color(palette.random)
                .into(),
            ])
            .on_toggle(
                SharedString::from(format!("actions-menu-trigger-{id}")),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_menu(id, cx)),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_menu(cx));
            })
            .into_any_element()
    }

    // --- render: add-action modal -----------------------------------------

    pub(super) fn render_add_modal(
        &self,
        form: &AddActionForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name_len = form.name.read(cx).content().chars().count().min(NAME_LIMIT);
        // Create needs a non-blank name and a real queue to file the action under.
        let valid = !form.name.read(cx).content().trim().is_empty() && !form.queues.is_empty();

        // NAME: field + N/64 counter.
        let name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(div().flex_1().child(form.name.clone()))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(format!("{name_len}/{NAME_LIMIT}")),
            );
        let name_section = modal_section(palette, "NAME", name_row);

        // GROUP: field led by a brand dot.
        let group_field = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(status_dot(palette.brand, GROUP_DOT))
            .child(div().flex_1().child(form.group.clone()));
        let group_section = modal_section(palette, "GROUP", group_field);

        // QUEUE: inline selectable chips (the kit carries no lightweight dropdown, so
        // the queue is chosen from chips — the Globals variant-kind picker approach).
        // Filled from the real queue repo; a loading caption stands in until it lands.
        let queue_control: AnyElement = if form.queues.is_empty() {
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child("Loading queues…")
                .into_any_element()
        } else {
            let mut queue_chips = div()
                .flex()
                .flex_wrap()
                .gap(spacing(Spacing::Xxs, Density::Cozy));
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
        let queue_section = modal_section(palette, "QUEUE", queue_control);

        let two_col = div()
            .flex()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(div().flex_1().child(group_section))
            .child(div().flex_1().child(queue_section));

        let desc_section = modal_section(
            palette,
            "DESCRIPTION",
            div().child(form.description.clone()),
        );

        let behavior_header = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child("BEHAVIOR");

        let enabled = self.modal_toggle_row(
            "Enabled",
            "Action runs when a trigger fires.",
            form.enabled,
            palette.success,
            "actions-modal-enabled",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_enabled(cx)),
            palette,
        );
        let concurrent = self.modal_toggle_row(
            "Concurrent execution",
            "Allow parallel runs in this queue.",
            form.concurrent,
            palette.info,
            "actions-modal-concurrent",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_concurrent(cx)),
            palette,
        );
        let bypass = self.modal_toggle_row(
            "Bypass queue pause",
            "Always run even if queue is paused.",
            form.bypass_pause,
            palette.warning,
            "actions-modal-bypass",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_bypass(cx)),
            palette,
        );
        let random = self.modal_toggle_row(
            "Random pick",
            "Run ONE random sub-action per trigger instead of all.",
            form.random_pick,
            palette.random,
            "actions-modal-random",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_random(cx)),
            palette,
        );

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(name_section)
            .child(two_col)
            .child(desc_section)
            .child(behavior_header)
            .child(enabled)
            .child(concurrent)
            .child(bypass)
            .child(random);

        let cancel = secondary_button("Cancel", palette).on_click(
            "actions-modal-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_add_modal(cx)),
        );
        let create = primary_button("Create action", palette)
            .disabled(!valid)
            .on_click(
                "actions-modal-create",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit_add_modal(cx)),
            );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(cancel)
            .child(create);

        let card = modal("New action", body, palette)
            .size(ModalSize::Md)
            .footer(footer)
            .kbd_hint("ESC to cancel")
            .on_close(
                "actions-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_add_modal(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-modal-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_add_modal(cx));
            })
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn modal_toggle_row(
        &self,
        label: &'static str,
        description: &'static str,
        on: bool,
        accent: Rgba,
        id: &'static str,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
        palette: &ForgePalette,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(label),
                    )
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XXS)
                            .text_color(palette.text_muted)
                            .child(description),
                    ),
            )
            .child(toggle(on, palette).on_color(accent).on_click(id, handler))
            .into_any_element()
    }

    // --- render: delete confirm -------------------------------------------

    pub(super) fn render_delete_confirm(
        &self,
        id: ActionId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self.find(id).map(|a| a.name.clone()).unwrap_or_default();
        let card = confirm_modal(
            "Delete action?",
            "This will remove the action and all of its sub-actions and triggers.",
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint("to cancel")
        .on_cancel(
            "actions-delete-cancel",
            "Cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "actions-delete-confirm",
            "Delete",
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
