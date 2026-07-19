use super::*;
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;
use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ChipGlyph, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY,
    Density, FONT_XS, FONT_XXS, ForgePalette, Icon, InlineEditEvent, InputEvent, ModalSize,
    OverlayPosition, ResizeEdge, ResizeRange, Spacing, TextArea, TextInput, ToastAction, ToastKind,
    breadcrumb, chip, confirm_modal, context_menu, icon, inline_edit, install_resize, menu_divider,
    menu_item, modal, overlay, primary_button, primary_button_with_icon, secondary_button, spacing,
    status_dot, toggle, tr,
};
use forge_types::{Action, ActionId, ExecutionMode, Queue};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, MouseButton, MouseDownEvent, Pixels, Point, Rgba,
    SharedString, Window, div, px,
};
use std::sync::Arc;
use std::time::Duration;

fn tree_notice(label: SharedString, color: Rgba, _palette: &ForgePalette) -> impl IntoElement {
    div()
        .w_full()
        .px(TREE_GUTTER)
        .py(spacing(Spacing::Sm, Density::Cozy))
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_XS)
        .text_color(color)
        .child(label)
}

fn modal_section(
    palette: &ForgePalette,
    label: impl Into<SharedString>,
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
                .child(label.into()),
        )
        .child(control)
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

    fn category_visible(filter: ActionsFilter, category: ActionCategory) -> bool {
        match filter {
            ActionsFilter::All => true,
            ActionsFilter::Chat => category == ActionCategory::Chat,
            ActionsFilter::Timers => category == ActionCategory::Timers,
            ActionsFilter::Points => category == ActionCategory::Points,
        }
    }

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
        self.telemetry = None;
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
            .any(|a| a.id != id && a.name.eq_ignore_ascii_case(&trimmed));
        if taken {
            cx.push_toast(
                ToastKind::Error,
                tr!("actions_rename_taken", name = trimmed.as_str()),
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

    pub(super) fn request_delete(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.pending_delete = Some(id);
        self.menu_open = None;
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete = None;
        cx.notify();
    }

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

    fn raise_undo_toast(
        &self,
        id: ActionId,
        name: String,
        repo: Arc<dyn ActionRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let message = tr!("actions_deleted_toast", name = name.as_str());
        cx.push_toast_full(
            ToastKind::Undo,
            message,
            None,
            Some(ToastAction::new(
                tr!("common_undo"),
                move |_window, app: &mut App| {
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
                },
            )),
            Duration::from_millis(6000),
        );
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
                .with_height(px(72.0));
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

    fn apply_queue_options(&mut self, queues: Vec<Queue>, cx: &mut Context<Self>) {
        if let Some(form) = self.action_modal.as_mut() {
            let default_id = queues.iter().find(|q| q.name == "Default").map(|q| q.id);
            form.queues = queues
                .into_iter()
                .map(|q| (q.id, SharedString::from(q.name)))
                .collect();
            let target = form.preselect_queue.or(default_id);
            form.selected_queue = target
                .and_then(|id| form.queues.iter().position(|(qid, _)| *qid == id))
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

    fn submit_action_modal(&mut self, cx: &mut Context<Self>) {
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
        let action = match form.base.clone() {
            Some(mut existing) => {
                existing.name = name;
                existing.group = group;
                existing.queue_id = queue_id;
                existing.enabled = form.enabled;
                existing.concurrent = form.concurrent;
                existing.bypass_pause = form.bypass_pause;
                existing.execution_mode = execution_mode;
                existing.description = description;
                existing
            }
            None => Action {
                id: ActionId::new(),
                name,
                group,
                queue_id,
                enabled: form.enabled,
                concurrent: form.concurrent,
                bypass_pause: form.bypass_pause,
                execution_mode,
                description,
                sub_actions: Vec::new(),
            },
        };
        let new_id = action.id;
        self.action_modal = None;
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
            .w(HEADER_DIV_W)
            .h(HEADER_DIV_H)
            .bg(palette.border_regular);

        let search = div().w(SEARCH_W).child(self.search_field.clone());

        let new_btn =
            primary_button_with_icon(Icon::Plus, tr!("actions_modal_new_action_title"), palette)
                .on_click(
                    "actions-new",
                    cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.open_action_modal(None, window, cx)
                    }),
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
                BreadcrumbCrumb::leaf(tr!("actions_breadcrumb_automation")),
                BreadcrumbCrumb::leaf(tr!("actions_breadcrumb_actions")),
            ],
            palette,
        )
        .right(cluster)
        .into_any_element()
    }

    pub(super) fn render_tree(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let mut col = div().flex().flex_col();

        if self.total_actions() == 0 {
            let caption = if self.loading {
                tr!("actions_tree_loading")
            } else {
                tr!("actions_empty")
            };
            col = col.child(tree_notice(
                SharedString::from(caption),
                palette.text_faint,
                palette,
            ));
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

        let inner = div()
            .id("actions-tree")
            .flex_1()
            .min_h(px(0.0))
            .py(spacing(Spacing::Xs, Density::Cozy))
            .overflow_y_scroll()
            .child(col)
            .children(self.render_row_context_menu(palette, cx));

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
                .font_family(DEFAULT_MONO_FAMILY)
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

        let slot_inner: AnyElement = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!(
                "action_editor_sub_count",
                count = action.sub_action_count as i64
            ))
            .into_any_element();
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
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    this.open_row_menu(id, ev.position, cx)
                }),
            )
            .child(div().w(STRIPE_W).h_full().bg(stripe_color))
            .child(select_area)
            .child(right_slot)
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
            .color(palette.random)
            .into(),
        ];

        Some(
            context_menu(position, palette)
                .items(items)
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
        let name_len = form.name.read(cx).content().chars().count().min(NAME_LIMIT);
        let valid = !form.name.read(cx).content().trim().is_empty() && !form.queues.is_empty();

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
        let name_section = modal_section(palette, tr!("actions_modal_section_name"), name_row);

        let group_field = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(status_dot(palette.brand, GROUP_DOT))
            .child(div().flex_1().child(form.group.clone()));
        let group_section = modal_section(palette, tr!("actions_modal_section_group"), group_field);

        let queue_control: AnyElement = if form.queues.is_empty() {
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(SharedString::from(tr!("actions_loading_queues")))
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
        let queue_section =
            modal_section(palette, tr!("actions_modal_section_queue"), queue_control);

        let two_col = div()
            .flex()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(div().flex_1().child(group_section))
            .child(div().flex_1().child(queue_section));

        let desc_section = modal_section(
            palette,
            tr!("actions_modal_section_description"),
            div().child(form.description.clone()),
        );

        let behavior_header = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(SharedString::from(tr!("actions_modal_section_behavior")));

        let enabled = self.modal_toggle_row(
            tr!("actions_modal_enabled_label"),
            tr!("actions_modal_enabled_desc"),
            form.enabled,
            palette.success,
            "actions-modal-enabled",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_enabled(cx)),
            palette,
        );
        let concurrent = self.modal_toggle_row(
            tr!("actions_modal_concurrent_label"),
            tr!("actions_modal_concurrent_desc"),
            form.concurrent,
            palette.info,
            "actions-modal-concurrent",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_concurrent(cx)),
            palette,
        );
        let bypass = self.modal_toggle_row(
            tr!("actions_modal_bypass_label"),
            tr!("actions_modal_bypass_desc"),
            form.bypass_pause,
            palette.warning,
            "actions-modal-bypass",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_bypass(cx)),
            palette,
        );
        let random = self.modal_toggle_row(
            tr!("actions_modal_random_pick_label"),
            tr!("actions_modal_random_pick_desc"),
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
            .gap(spacing(Spacing::Xs, Density::Cozy))
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
            .kbd_hint(tr!("actions_esc_hint"))
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

    #[allow(clippy::too_many_arguments)]
    fn modal_toggle_row(
        &self,
        label: impl Into<SharedString>,
        description: impl Into<SharedString>,
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
                            .child(label.into()),
                    )
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
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
        let card = confirm_modal(
            tr!("actions_delete_title"),
            tr!("actions_delete_body"),
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
