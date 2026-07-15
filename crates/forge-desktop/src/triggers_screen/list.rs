use super::*;
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;
use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ChipGlyph, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY,
    Density, FONT_XXS, ForgePalette, Icon, InputEvent, MenuPlacement, ModalSize, OverlayPosition,
    Radius, Spacing, TextInput, ToastAction, ToastKind, badge, breadcrumb, chip, confirm_modal,
    ghost_button_with_icon, icon, menu_button, menu_divider, menu_item, modal, overlay,
    primary_button, primary_button_with_icon, secondary_button, spacing, status_dot, toggle,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, Entity, FontWeight, Rgba, SharedString, Window, div,
    px,
};
use std::sync::Arc;
use std::time::Duration;

impl TriggersRegistryView {
    pub(super) fn find(&self, id: TriggerInstanceId) -> Option<&TriggerInstanceRow> {
        self.instances.iter().find(|i| i.id == id)
    }

    fn used_count(&self) -> usize {
        self.instances
            .iter()
            .filter(|i| i.used_in_count > 0)
            .count()
    }

    fn disabled_count(&self) -> usize {
        self.instances.iter().filter(|i| !i.enabled).count()
    }

    fn platform_counts(&self) -> Vec<(Platform, usize)> {
        Platform::ORDER
            .into_iter()
            .filter_map(|p| {
                let count = self
                    .instances
                    .iter()
                    .filter(|i| Platform::from_kind_id(&i.kind_id) == Some(p))
                    .count();
                (count > 0).then_some((p, count))
            })
            .collect()
    }

    fn has_active_filter(&self) -> bool {
        !self.search.trim().is_empty()
            || !self.platforms.is_empty()
            || self.usage_filter != UsageFilter::All
    }

    fn kind_label(&self, kind_id: &str) -> String {
        self.registry
            .get(kind_id)
            .map(|d| d.label().to_owned())
            .unwrap_or_else(|| kind_id.to_owned())
    }

    fn passes(&self, instance: &TriggerInstanceRow) -> bool {
        if !self.platforms.is_empty()
            && !Platform::from_kind_id(&instance.kind_id)
                .is_some_and(|p| self.platforms.contains(&p))
        {
            return false;
        }
        match self.usage_filter {
            UsageFilter::Used if instance.used_in_count == 0 => return false,
            UsageFilter::Unused if instance.used_in_count > 0 => return false,
            _ => {}
        }
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        instance.name.to_lowercase().contains(&q)
            || instance.kind_id.to_lowercase().contains(&q)
            || self
                .kind_label(&instance.kind_id)
                .to_lowercase()
                .contains(&q)
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

    fn toggle_platform(&mut self, platform: Platform, cx: &mut Context<Self>) {
        if let Some(pos) = self.platforms.iter().position(|&p| p == platform) {
            self.platforms.remove(pos);
        } else {
            self.platforms.push(platform);
        }
        cx.notify();
    }

    fn clear_platforms(&mut self, cx: &mut Context<Self>) {
        self.platforms.clear();
        cx.notify();
    }

    fn set_usage_filter(&mut self, filter: UsageFilter, cx: &mut Context<Self>) {
        self.usage_filter = if self.usage_filter == filter {
            UsageFilter::All
        } else {
            filter
        };
        cx.notify();
    }

    fn clear_filters(&mut self, cx: &mut Context<Self>) {
        self.search.clear();
        let field = self.search_field.clone();
        field.update(cx, |input, cx| input.set_content("", cx));
        self.platforms.clear();
        self.usage_filter = UsageFilter::All;
        cx.notify();
    }

    fn select(&mut self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        if self.selected != Some(id) {
            self.detail = None;
        }
        self.selected = Some(id);
        self.load_detail(id, cx);
        cx.notify();
    }

    fn set_hover(&mut self, id: TriggerInstanceId, hovered: bool, cx: &mut Context<Self>) {
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

    fn toggle_menu(&mut self, id: TriggerInstanceId, cx: &mut Context<Self>) {
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

    fn persist_enabled(&mut self, id: TriggerInstanceId, enabled: bool, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
        let repo = Arc::clone(&self.repo);
        self.spawn_reload(
            async move {
                repo.set_enabled(id, enabled)
                    .await
                    .map_err(|e| e.to_string())?;
                load_rows(&*repo).await
            },
            cx,
        );
    }

    pub(super) fn toggle_enable(&mut self, id: TriggerInstanceId, cx: &mut Context<Self>) {
        let Some(instance) = self.find(id) else {
            return;
        };
        if !instance.enabled {
            self.persist_enabled(id, true, cx);
        } else if instance.used_in_count > 0 {
            self.confirm_disable = Some(id);
            cx.notify();
        } else {
            self.persist_enabled(id, false, cx);
        }
    }

    fn confirm_disable_now(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.confirm_disable.take() {
            self.persist_enabled(id, false, cx);
        } else {
            cx.notify();
        }
    }

    fn cancel_disable(&mut self, cx: &mut Context<Self>) {
        self.confirm_disable = None;
        cx.notify();
    }

    pub(super) fn request_delete(&mut self, id: TriggerInstanceId, cx: &mut Context<Self>) {
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
        let Some(instance) = self.find(id).filter(|i| i.used_in_count == 0) else {
            cx.notify();
            return;
        };
        let name = instance.name.clone();
        cx.notify();

        let repo = Arc::clone(&self.repo);
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<TriggerInstanceRow>, String>>();
        self.rt_handle.spawn(async move {
            let outcome = async {
                repo.archive(id).await.map_err(|e| e.to_string())?;
                load_rows(&*repo).await
            }
            .await;
            let _ = tx.send(outcome);
        });

        let restore_repo = Arc::clone(&self.repo);
        let restore_rt = self.rt_handle.clone();
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(rows)) => {
                let _ = this.update(cx, |this, cx| {
                    this.apply_rows(rows, cx);
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
        id: TriggerInstanceId,
        name: String,
        repo: Arc<dyn forge_storage::TriggerInstanceRepo>,
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
                        load_rows(&*repo).await
                    },
                    app,
                );
            })),
            Duration::from_millis(6000),
        );
    }

    pub(super) fn start_rename(
        &mut self,
        id: TriggerInstanceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(instance) = self.find(id) else {
            return;
        };
        let palette = cx.palette();
        let seed = instance.name.clone();
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
        self.rename = Some(RenameForm {
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
                self.rename = None;
                cx.notify();
            }
            InputEvent::Changed(_) => {}
        }
    }

    fn submit_rename(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.rename.as_ref() {
            let name = form.field.read(cx).content().to_string();
            self.commit_rename(name, cx);
        }
    }

    fn commit_rename(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(form) = self.rename.take() else {
            cx.notify();
            return;
        };
        let trimmed = name.trim().to_owned();
        cx.notify();
        if trimmed.is_empty() {
            return;
        }
        let id = form.id;
        let repo = Arc::clone(&self.repo);
        self.spawn_reload(
            async move {
                let mut instance = repo
                    .get(id)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "trigger instance not found".to_owned())?;
                instance.name = trimmed;
                repo.save(&instance).await.map_err(|e| e.to_string())?;
                load_rows(&*repo).await
            },
            cx,
        );
    }

    pub(super) fn render_header(&self, palette: &ForgePalette) -> AnyElement {
        let sep = || {
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(STATS_FS)
                .text_color(palette.text_faint)
                .child("\u{b7}")
        };
        let stat = |value: String, value_color: Rgba, label: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, Density::Cozy))
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(STATS_FS)
                        .text_color(value_color)
                        .child(value),
                )
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(STATS_FS)
                        .text_color(palette.text_muted)
                        .child(label),
                )
        };

        let stats = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(stat(
                self.instances.len().to_string(),
                palette.text_primary,
                "instances",
            ))
            .child(sep())
            .child(stat(self.used_count().to_string(), palette.success, "used"))
            .child(sep())
            .child(stat(
                self.disabled_count().to_string(),
                palette.warning,
                "disabled",
            ));

        breadcrumb(
            vec![
                BreadcrumbCrumb::leaf("Automation"),
                BreadcrumbCrumb::leaf("Triggers"),
            ],
            palette,
        )
        .right(stats)
        .into_any_element()
    }

    fn divider(&self, palette: &ForgePalette) -> AnyElement {
        div()
            .w(FILTER_DIV_W)
            .h(FILTER_DIV_H)
            .bg(palette.border_regular)
            .into_any_element()
    }

    pub(super) fn render_filter_bar(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut platform_chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy));
        for (platform, count) in self.platform_counts() {
            let active = self.platforms.contains(&platform);
            let label = format!("{} {}", platform.label(), count);
            platform_chips = platform_chips.child(
                chip(
                    label,
                    ChipGlyph::Dot(platform.dot(palette)),
                    active,
                    palette,
                )
                .on_click(
                    SharedString::from(format!("triggers-platform-{}", platform.label())),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.toggle_platform(platform, cx)
                    }),
                ),
            );
        }
        if !self.platforms.is_empty() {
            platform_chips = platform_chips.child(
                div()
                    .id("triggers-platform-clear")
                    .cursor_pointer()
                    .px(spacing(Spacing::Xs, Density::Cozy))
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("clear")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.clear_platforms(cx))),
            );
        }

        let usage_chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                chip(
                    "Used",
                    ChipGlyph::Dot(palette.success),
                    self.usage_filter == UsageFilter::Used,
                    palette,
                )
                .on_click(
                    "triggers-usage-used",
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.set_usage_filter(UsageFilter::Used, cx)
                    }),
                ),
            )
            .child(
                chip(
                    "Unused",
                    ChipGlyph::Dot(palette.text_faint),
                    self.usage_filter == UsageFilter::Unused,
                    palette,
                )
                .on_click(
                    "triggers-usage-unused",
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.set_usage_filter(UsageFilter::Unused, cx)
                    }),
                ),
            );

        let left = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(div().child(self.search_field.clone()))
            .child(self.divider(palette))
            .child(platform_chips)
            .child(self.divider(palette))
            .child(usage_chips);

        let new_trigger = primary_button_with_icon(Icon::Plus, "New trigger", palette).on_click(
            "triggers-new",
            cx.listener(|this, _: &ClickEvent, window, cx| this.open_create(window, cx)),
        );

        div()
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .py(FILTER_PAD_V)
            .px(spacing(Spacing::Md, Density::Cozy))
            .bg(palette.elevated)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(left)
            .child(new_trigger)
            .into_any_element()
    }

    fn columns(
        dot: AnyElement,
        name: AnyElement,
        kind: AnyElement,
        used: AnyElement,
        on: AnyElement,
        menu: AnyElement,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .w_full()
            .child(
                div()
                    .w(COL_DOT)
                    .flex_none()
                    .flex()
                    .items_center()
                    .child(dot),
            )
            .child(div().w(COL_NAME).flex_none().overflow_hidden().child(name))
            .child(div().flex_1().min_w(px(0.0)).child(kind))
            .child(
                div()
                    .w(COL_USED)
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(used),
            )
            .child(
                div()
                    .w(COL_ON)
                    .flex_none()
                    .flex()
                    .justify_center()
                    .child(on),
            )
            .child(
                div()
                    .w(COL_MENU)
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(menu),
            )
    }

    fn caption_cell(&self, palette: &ForgePalette, label: &'static str) -> AnyElement {
        div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(label)
            .into_any_element()
    }

    fn render_caption(&self, palette: &ForgePalette) -> AnyElement {
        let cols = Self::columns(
            div().into_any_element(),
            self.caption_cell(palette, "NAME"),
            self.caption_cell(palette, "KIND"),
            self.caption_cell(palette, "USED IN"),
            self.caption_cell(palette, "ON"),
            div().into_any_element(),
        );
        div()
            .w_full()
            .flex_none()
            .py(CAPTION_PAD_V)
            .px(CAPTION_PAD_H)
            .bg(palette.shell)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(cols)
            .into_any_element()
    }

    pub(super) fn render_list(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let visible: Vec<&TriggerInstanceRow> =
            self.instances.iter().filter(|i| self.passes(i)).collect();

        let inner = if visible.is_empty() {
            self.render_empty(palette, cx)
        } else {
            let mut col = div().flex().flex_col().child(self.render_caption(palette));
            for instance in visible {
                col = col.child(self.render_row(instance, palette, cx));
            }
            col.into_any_element()
        };

        div()
            .id("triggers-list")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(inner)
            .into_any_element()
    }

    fn render_row(
        &self,
        instance: &TriggerInstanceRow,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = instance.id;
        let selected = self.selected == Some(id);
        let hovered = self.hovered == Some(id);
        let dot_color = platform_dot_color(&instance.kind_id, palette);
        let descriptor = self.registry.get(&instance.kind_id);
        let kind_glyph = descriptor
            .map(|d| Icon::from_name(d.icon_name()))
            .unwrap_or(Icon::Bolt);
        let kind_label = descriptor
            .map(|d| d.label().to_owned())
            .unwrap_or_else(|| instance.kind_id.clone());

        let stripe_color = if selected {
            dot_color
        } else {
            gpui::transparent_black().into()
        };
        let row_bg: Rgba = if selected || hovered {
            palette.elevated
        } else {
            gpui::transparent_black().into()
        };
        let name_color = if !instance.enabled {
            palette.text_muted
        } else {
            palette.text_primary
        };

        let mut kind = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .min_w(px(0.0))
            .child(icon(kind_glyph, KIND_GLYPH, dot_color))
            .child(
                div()
                    .flex_shrink()
                    .overflow_hidden()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(KIND_FS)
                    .text_color(palette.text_muted)
                    .child(kind_label),
            );
        if instance.override_count > 0 {
            let label = if instance.override_count == 1 {
                "1 override".to_owned()
            } else {
                format!("{} overrides", instance.override_count)
            };
            kind = kind.child(badge(
                palette.surface_overlay,
                palette.bits,
                label,
                true,
                BADGE_FS,
            ));
        }

        let used: AnyElement = if instance.used_in_count > 0 {
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, Density::Cozy))
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(USED_FS)
                .text_color(palette.text_primary)
                .child("used in")
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(palette.success)
                        .child(instance.used_in_count.to_string()),
                )
                .into_any_element()
        } else {
            div()
                .italic()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(USED_FS)
                .text_color(palette.text_faint)
                .child("unused")
                .into_any_element()
        };

        let select_region = div()
            .id(SharedString::from(format!("triggers-row-select-{id}")))
            .flex_1()
            .flex()
            .items_center()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select(id, cx)))
            .child(
                div()
                    .w(COL_DOT)
                    .flex_none()
                    .flex()
                    .items_center()
                    .child(status_dot(dot_color, ROW_DOT)),
            )
            .child(
                div()
                    .w(COL_NAME)
                    .flex_none()
                    .overflow_hidden()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(NAME_FS)
                    .text_color(name_color)
                    .child(instance.name.clone()),
            )
            .child(div().flex_1().min_w(px(0.0)).child(kind))
            .child(
                div()
                    .w(COL_USED)
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(used),
            );

        let on_cell = div().w(COL_ON).flex_none().flex().justify_center().child(
            toggle(instance.enabled, palette).on_click(
                SharedString::from(format!("triggers-toggle-{id}")),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_enable(id, cx)),
            ),
        );

        let menu_cell = div()
            .w(COL_MENU)
            .flex_none()
            .flex()
            .justify_end()
            .child(self.render_row_menu(instance, palette, cx));

        let content = div()
            .w_full()
            .flex()
            .items_center()
            .pl(ROW_PAD_L)
            .pr(ROW_PAD_R)
            .py(ROW_PAD_V)
            .child(select_region)
            .child(on_cell)
            .child(menu_cell);

        div()
            .id(SharedString::from(format!("triggers-row-{id}")))
            .w_full()
            .flex()
            .bg(row_bg)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .when(!instance.enabled, |row| row.opacity(DISABLED_OPACITY))
            .on_hover(
                cx.listener(move |this, hovered: &bool, _, cx| this.set_hover(id, *hovered, cx)),
            )
            .child(div().w(STRIPE_W).flex_none().bg(stripe_color))
            .child(content)
            .into_any_element()
    }

    fn render_row_menu(
        &self,
        instance: &TriggerInstanceRow,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = instance.id;
        let menu_open = self.menu_open == Some(id);
        let block_delete = instance.used_in_count > 0;
        let view = cx.entity();

        menu_button(Icon::DotsVertical, menu_open, palette)
            .placement(MenuPlacement::BottomRight)
            .items(vec![
                menu_item(
                    SharedString::from(format!("triggers-menu-rename-{id}")),
                    "Rename\u{2026}",
                    cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.start_rename(id, window, cx)
                    }),
                )
                .icon(Icon::Pencil)
                .into(),
                menu_divider(),
                menu_item(
                    SharedString::from(format!("triggers-menu-delete-{id}")),
                    "Delete\u{2026}",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(id, cx)),
                )
                .icon(Icon::Eraser)
                .color(palette.random)
                .disabled(block_delete)
                .into(),
            ])
            .on_toggle(
                SharedString::from(format!("triggers-menu-trigger-{id}")),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_menu(id, cx)),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_menu(cx));
            })
            .into_any_element()
    }

    fn render_empty(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let has_filter = self.has_active_filter();
        let (glyph, glyph_color) = if has_filter {
            (Icon::MoodSmile, palette.text_faint)
        } else {
            (Icon::Bolt, palette.warning)
        };
        let title = if has_filter {
            "No matches"
        } else {
            "No custom trigger instances yet"
        };
        let body = if has_filter {
            "Try a different filter combination.".to_owned()
        } else {
            "Triggers are named, reusable configurations of an event source. \
             Multiple actions can share one trigger."
                .to_owned()
        };

        let action: AnyElement = if has_filter {
            ghost_button_with_icon(Icon::X, "Clear filters", palette)
                .on_click(
                    "triggers-empty-clear",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.clear_filters(cx)),
                )
                .into_any_element()
        } else {
            primary_button_with_icon(Icon::Plus, "Create your first trigger", palette)
                .on_click(
                    "triggers-empty-create",
                    cx.listener(|this, _: &ClickEvent, window, cx| this.open_create(window, cx)),
                )
                .into_any_element()
        };

        let tile = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(EMPTY_TILE)
            .rounded(EMPTY_TILE_RADIUS)
            .bg(palette.shell)
            .child(icon(glyph, EMPTY_GLYPH, glyph_color));

        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .py(EMPTY_PAD_V)
            .px(EMPTY_PAD_H)
            .child(tile)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(EMPTY_TITLE_FS)
                    .text_color(palette.text_primary)
                    .child(title),
            )
            .child(
                div()
                    .max_w(px(360.0))
                    .text_center()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(EMPTY_BODY_FS)
                    .text_color(palette.text_muted)
                    .child(body),
            )
            .child(action)
            .into_any_element()
    }

    pub(super) fn render_disable_confirm(
        &self,
        id: TriggerInstanceId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (name, count) = self
            .find(id)
            .map(|i| (i.name.clone(), i.used_in_count))
            .unwrap_or_default();
        let plural = if count == 1 { "action" } else { "actions" };
        let card = confirm_modal(
            format!("Disable {name}?"),
            format!(
                "Disabling this trigger will pause it for {count} {plural}. \
                 They won't fire until the trigger is re-enabled."
            ),
            ConfirmTone::Warning,
            palette,
        )
        .esc_hint("to cancel")
        .on_cancel(
            "triggers-disable-cancel",
            "Cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_disable(cx)),
        )
        .on_confirm(
            "triggers-disable-confirm",
            "Disable anyway",
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_disable_now(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("triggers-disable-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_disable(cx));
            })
            .into_any_element()
    }

    pub(super) fn render_delete_confirm(
        &self,
        id: TriggerInstanceId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (name, used_in) = self
            .find(id)
            .map(|i| (i.name.clone(), i.used_in_count))
            .unwrap_or_default();
        let message = if used_in > 0 {
            let plural = if used_in == 1 { "action" } else { "actions" };
            format!("This trigger is used by {used_in} {plural}. Remove it from them first.")
        } else {
            "This deletes the trigger instance permanently.".to_owned()
        };
        let card = confirm_modal(
            "Delete trigger?",
            message,
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint("to cancel")
        .on_cancel(
            "triggers-delete-cancel",
            "Cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "triggers-delete-confirm",
            "Delete",
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("triggers-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }

    pub(super) fn render_rename_modal(
        &self,
        form: &RenameForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let valid = !form.field.read(cx).content().trim().is_empty();

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("NAME"),
            )
            .child(div().child(form.field.clone()));

        let cancel = secondary_button("Cancel", palette).on_click(
            "triggers-rename-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.rename = None;
                cx.notify();
            }),
        );
        let save = primary_button("Save", palette).disabled(!valid).on_click(
            "triggers-rename-save",
            cx.listener(|this, _: &ClickEvent, _, cx| this.submit_rename(cx)),
        );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(cancel)
            .child(save);

        let card = modal("Rename trigger", body, palette)
            .size(ModalSize::Sm)
            .footer(footer)
            .kbd_hint("ENTER to save \u{b7} ESC to cancel")
            .on_close(
                "triggers-rename-close",
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.rename = None;
                    cx.notify();
                }),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("triggers-rename-scrim", move |_window, cx| {
                view.update(cx, |this, cx| {
                    this.rename = None;
                    cx.notify();
                });
            })
            .into_any_element()
    }
}
