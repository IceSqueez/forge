mod editor_pane;
mod event_options;
mod form_modal;
mod kind_visuals;
mod preview_stage;
mod property_panel;
mod registry_pane;

use std::collections::HashMap;
use std::sync::Arc;

use forge_components::{
    BreadcrumbCrumb, Confirm, ConfirmTone, FONT_XS, ForgePalette, Icon, OverlayPosition, ToastKind,
    body_family, confirm_modal, drive_overlay_focus, icon, overlay, page_frame, tr,
};
use forge_overlay::config::EVENT_KINDS_OPTIONS_KEY;
use forge_overlay::{OverlayKindRegistry, effective_overlay_config};
use forge_registry::TriggerRegistry;
use forge_runtime::OverlayServiceHandle;
use forge_server::ServerHandle;
use forge_storage::{OverlayConfig, OverlayDefinition, OverlayId, OverlayRepo};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FocusHandle, Pixels, Point, Subscription, Window, div,
    prelude::*, px,
};

use crate::async_bridge;
use crate::overlay_url::{overlay_origin, overlay_page_url};
use crate::presentation::ActivePresentation;
use crate::toasts::{PushToast, copy_to_clipboard};

use event_options::event_kind_options;
use form_modal::{OverlayFormEvent, OverlayFormLaunch, OverlayFormModal, OverlayTypeChoice};
use kind_visuals::{KindVisuals, kind_visuals};
use preview_stage::TestFireRun;
use property_panel::{OverlayPropertyPanel, PanelLaunch, PropertyPanelEvent};

const HEADER_GAP: Pixels = px(5.0);
const HEADER_GLYPH: Pixels = px(13.0);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EditorMode {
    Design,
    Code,
}

struct OpenForm {
    view: Entity<OverlayFormModal>,
    _sub: Subscription,
}

struct OpenPanel {
    view: Entity<OverlayPropertyPanel>,
    _sub: Subscription,
}

struct PendingDelete {
    id: OverlayId,
    display_name: String,
}

pub struct OverlaysView {
    repo: Arc<dyn OverlayRepo>,
    server: Option<ServerHandle>,
    rt_handle: tokio::runtime::Handle,
    kinds: Arc<OverlayKindRegistry>,
    triggers: Arc<TriggerRegistry>,
    service: OverlayServiceHandle,
    overlays: Vec<OverlayDefinition>,
    selected: Option<OverlayId>,
    mode: EditorMode,
    panel: Option<OpenPanel>,
    loading: bool,
    server_running: bool,
    bind_address: Option<String>,
    menu_open: Option<OverlayId>,
    menu_click_pos: Option<Point<Pixels>>,
    form: Option<OpenForm>,
    pending_delete: Confirm<PendingDelete>,
    fire: Option<TestFireRun>,
    fire_epoch: u64,
    overlay_focus: FocusHandle,
    focus_restore: Option<FocusHandle>,
}

impl OverlaysView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: Arc<dyn OverlayRepo>,
        server: Option<ServerHandle>,
        rt_handle: tokio::runtime::Handle,
        kinds: Arc<OverlayKindRegistry>,
        triggers: Arc<TriggerRegistry>,
        service: OverlayServiceHandle,
        cx: &mut Context<Self>,
    ) -> Self {
        let server_running = server
            .as_ref()
            .is_some_and(|handle| *handle.run_state().borrow());

        let mut view = Self {
            repo,
            server,
            rt_handle,
            kinds,
            triggers,
            service,
            overlays: Vec::new(),
            selected: None,
            mode: EditorMode::Design,
            panel: None,
            loading: false,
            server_running,
            bind_address: None,
            menu_open: None,
            menu_click_pos: None,
            form: None,
            pending_delete: Confirm::default(),
            fire: None,
            fire_epoch: 0,
            overlay_focus: cx.focus_handle(),
            focus_restore: None,
        };
        view.load(cx);
        view.start_server_bridge(cx);
        view
    }

    fn index_of(&self, id: &OverlayId) -> Option<usize> {
        self.overlays.iter().position(|item| &item.id == id)
    }

    fn selected_definition(&self) -> Option<&OverlayDefinition> {
        self.selected
            .as_ref()
            .and_then(|id| self.overlays.iter().find(|item| &item.id == id))
    }

    fn visuals(&self, definition: &OverlayDefinition, palette: &ForgePalette) -> KindVisuals {
        kind_visuals(definition, &self.kinds, palette)
    }

    fn enabled_count(&self) -> usize {
        self.overlays.iter().filter(|item| item.enabled).count()
    }

    /// `None` whenever the server is not serving, so the UI states that instead of showing a dead address.
    fn overlay_url(&self, id: &OverlayId) -> Option<String> {
        if !self.server_running {
            return None;
        }
        let origin = overlay_origin(self.bind_address.as_deref()?);
        Some(overlay_page_url(&origin, id.as_str()))
    }

    fn type_choices(&self) -> Vec<OverlayTypeChoice> {
        let mut choices: Vec<OverlayTypeChoice> = self
            .kinds
            .all()
            .map(|descriptor| OverlayTypeChoice {
                kind_id: descriptor.id().to_owned(),
                label: descriptor.label().to_owned(),
                summary: descriptor.summary().to_owned(),
                icon: Icon::from_name(descriptor.icon_name()),
            })
            .collect();
        choices.sort_by(|a, b| a.label.cmp(&b.label));
        choices
    }

    fn report(&mut self, message: &str, cx: &mut Context<Self>) {
        tracing::warn!(error = %message, "overlay registry operation failed");
        cx.push_toast(ToastKind::Error, message.to_owned());
        cx.notify();
    }

    fn start_server_bridge(&self, cx: &mut Context<Self>) {
        let Some(handle) = self.server.clone() else {
            return;
        };
        let rt_handle = self.rt_handle.clone();
        let mut run_state = handle.run_state();
        cx.spawn(async move |this, cx| {
            loop {
                let running = *run_state.borrow_and_update();
                let (tx, rx) = tokio::sync::oneshot::channel();
                let probe = handle.clone();
                rt_handle.spawn(async move {
                    let _ = tx.send(probe.bind_addr().await.to_string());
                });
                let bind_address = rx.await.ok();
                if this
                    .update(cx, |this, cx| {
                        this.apply_server_state(running, bind_address, cx)
                    })
                    .is_err()
                    || run_state.changed().await.is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn apply_server_state(
        &mut self,
        running: bool,
        bind_address: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let mut changed = self.server_running != running;
        self.server_running = running;
        if bind_address.is_some() && self.bind_address != bind_address {
            self.bind_address = bind_address;
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        let repo = Arc::clone(&self.repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { repo.list().await.map_err(|e| e.to_string()) },
            |this, result, cx| this.apply_list(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_list(
        &mut self,
        result: Result<Vec<OverlayDefinition>, String>,
        cx: &mut Context<Self>,
    ) {
        self.loading = false;
        match result {
            Ok(rows) => {
                self.overlays = rows;
                let selection_survived = self
                    .selected
                    .as_ref()
                    .is_some_and(|id| self.index_of(id).is_some());
                if !selection_survived {
                    self.selected = self.overlays.first().map(|item| item.id.clone());
                    self.clear_test();
                }
                self.sync_panel(cx);
            }
            Err(message) => self.report(&message, cx),
        }
        cx.notify();
    }

    fn select(&mut self, id: OverlayId, cx: &mut Context<Self>) {
        if self.selected.as_ref() == Some(&id) {
            return;
        }
        self.selected = Some(id);
        self.clear_test();
        self.sync_panel(cx);
        cx.notify();
    }

    pub(super) fn mode(&self) -> EditorMode {
        self.mode
    }

    pub(in crate::overlays_screen) fn panel_view(&self) -> Option<Entity<OverlayPropertyPanel>> {
        self.panel.as_ref().map(|open| open.view.clone())
    }

    fn set_mode(&mut self, mode: EditorMode, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        cx.notify();
    }

    /// Rebuilt only when the selection moves: a rebuild drops the live text inputs, so a reload
    /// caused by the user's own save must leave the panel they are editing alone.
    fn sync_panel(&mut self, cx: &mut Context<Self>) {
        let target = self
            .selected_definition()
            .filter(|definition| self.kinds.get(&definition.kind_id).is_some())
            .cloned();

        let Some(definition) = target else {
            self.panel = None;
            return;
        };
        if self
            .panel
            .as_ref()
            .is_some_and(|open| open.view.read(cx).overlay_id() == &definition.id)
        {
            return;
        }
        let Some(descriptor) = self.kinds.get(&definition.kind_id) else {
            self.panel = None;
            return;
        };

        let launch = PanelLaunch {
            overlay_id: definition.id.clone(),
            specs: descriptor.config_fields(),
            defaults: descriptor.default_config(),
            stored: definition.config.clone(),
            effective: effective_overlay_config(descriptor, &definition.config),
            choices: HashMap::from([(
                EVENT_KINDS_OPTIONS_KEY.to_owned(),
                event_kind_options(&self.triggers),
            )]),
            overridden_files: definition.source_overrides.clone(),
        };

        let view = cx.new(|cx| OverlayPropertyPanel::new(launch, cx));
        let sub = cx.subscribe(&view, Self::on_panel_event);
        self.panel = Some(OpenPanel { view, _sub: sub });
    }

    fn on_panel_event(
        &mut self,
        view: Entity<OverlayPropertyPanel>,
        event: &PropertyPanelEvent,
        cx: &mut Context<Self>,
    ) {
        let PropertyPanelEvent::Save(config) = event;
        let id = view.read(cx).overlay_id().clone();
        self.save_config(id, config.clone(), cx);
    }

    /// Re-reads the stored record so a background change to another field is not clobbered by the
    /// panel's cached copy; only the config document is replaced.
    fn save_config(&mut self, id: OverlayId, config: OverlayConfig, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.repo);
        let service = self.service.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let Some(mut definition) = repo.get(&id).await.map_err(|e| e.to_string())? else {
                    return Ok((false, None));
                };
                definition.config = config;
                repo.save(&definition).await.map_err(|e| e.to_string())?;
                let generated = service.materialize(&id).await.err().map(|e| e.to_string());
                Ok((true, generated))
            },
            |this, result: Result<(bool, Option<String>), String>, cx| match result {
                Ok((true, generated)) => {
                    if let Some(message) = generated {
                        this.report(&message, cx);
                    }
                    this.load(cx);
                }
                Ok((false, _)) => this.report(&tr!("overlays_toast_missing"), cx),
                Err(message) => this.report(&message, cx),
            },
            cx,
        );
    }

    fn toggle_enabled(&mut self, id: OverlayId, cx: &mut Context<Self>) {
        let Some(index) = self.index_of(&id) else {
            return;
        };
        let next = !self.overlays[index].enabled;
        self.overlays[index].enabled = next;
        cx.notify();

        let repo = Arc::clone(&self.repo);
        let target = id.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                repo.set_enabled(&target, next)
                    .await
                    .map_err(|e| e.to_string())
            },
            move |this, result: Result<bool, String>, cx| {
                let failure = match result {
                    Ok(true) => None,
                    Ok(false) => Some(tr!("overlays_toast_missing")),
                    Err(message) => Some(message),
                };
                let Some(message) = failure else {
                    return;
                };
                if let Some(index) = this.index_of(&id) {
                    this.overlays[index].enabled = !next;
                }
                this.report(&message, cx);
            },
            cx,
        );
    }

    fn copy_url(&mut self, id: &OverlayId, cx: &mut Context<Self>) {
        self.menu_open = None;
        match self.overlay_url(id) {
            Some(url) => copy_to_clipboard(url, cx),
            None => cx.push_toast(ToastKind::Info, tr!("overlays_toast_url_unavailable")),
        }
        cx.notify();
    }

    fn open_create_form(&mut self, cx: &mut Context<Self>) {
        let types = self.type_choices();
        let kind_id = types
            .first()
            .map(|choice| choice.kind_id.clone())
            .unwrap_or_default();
        self.open_form(
            OverlayFormLaunch {
                target: None,
                display_name: String::new(),
                kind_id,
                types,
            },
            cx,
        );
    }

    fn open_rename_form(&mut self, id: OverlayId, cx: &mut Context<Self>) {
        let Some(definition) = self.index_of(&id).map(|index| &self.overlays[index]) else {
            return;
        };
        let launch = OverlayFormLaunch {
            display_name: definition.display_name.clone(),
            kind_id: definition.kind_id.clone(),
            target: Some(id),
            types: self.type_choices(),
        };
        self.open_form(launch, cx);
    }

    fn open_form(&mut self, launch: OverlayFormLaunch, cx: &mut Context<Self>) {
        let view = cx.new(|cx| OverlayFormModal::new(launch, cx));
        let sub = cx.subscribe(&view, Self::on_form_event);
        self.form = Some(OpenForm { view, _sub: sub });
        self.menu_open = None;
        cx.notify();
    }

    fn close_form(&mut self, cx: &mut Context<Self>) {
        self.form = None;
        cx.notify();
    }

    fn on_form_event(
        &mut self,
        _view: Entity<OverlayFormModal>,
        event: &OverlayFormEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            OverlayFormEvent::Submit {
                target,
                display_name,
                kind_id,
            } => match target {
                Some(id) => self.rename(id.clone(), display_name.clone(), cx),
                None => self.create(display_name.clone(), kind_id.clone(), cx),
            },
            OverlayFormEvent::Cancel => self.close_form(cx),
        }
    }

    fn create(&mut self, display_name: String, kind_id: String, cx: &mut Context<Self>) {
        let Some(schema_version) = self
            .kinds
            .get(&kind_id)
            .map(|descriptor| descriptor.config_schema_version())
        else {
            self.report(&tr!("overlays_toast_unknown_type"), cx);
            return;
        };
        self.close_form(cx);

        let repo = Arc::clone(&self.repo);
        let service = self.service.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let definition = repo
                    .create(&display_name, &kind_id, schema_version)
                    .await
                    .map_err(|e| e.to_string())?;
                let generated = service
                    .materialize(&definition.id)
                    .await
                    .err()
                    .map(|e| e.to_string());
                Ok((definition, generated))
            },
            |this, result: Result<(OverlayDefinition, Option<String>), String>, cx| match result {
                Ok((definition, generated)) => {
                    this.selected = Some(definition.id);
                    cx.push_toast(ToastKind::Success, tr!("overlays_toast_created"));
                    if let Some(message) = generated {
                        this.report(&message, cx);
                    }
                    this.load(cx);
                }
                Err(message) => this.report(&message, cx),
            },
            cx,
        );
    }

    /// Reads the stored record before writing so a background config change is not clobbered by a
    /// stale cache. The directory keeps its identity; only the config document is rewritten.
    fn rename(&mut self, id: OverlayId, display_name: String, cx: &mut Context<Self>) {
        self.close_form(cx);
        let repo = Arc::clone(&self.repo);
        let service = self.service.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let Some(mut definition) = repo.get(&id).await.map_err(|e| e.to_string())? else {
                    return Ok((false, None));
                };
                definition.display_name = display_name;
                repo.save(&definition).await.map_err(|e| e.to_string())?;
                let generated = service.materialize(&id).await.err().map(|e| e.to_string());
                Ok((true, generated))
            },
            |this, result: Result<(bool, Option<String>), String>, cx| match result {
                Ok((true, generated)) => {
                    cx.push_toast(ToastKind::Success, tr!("overlays_toast_renamed"));
                    if let Some(message) = generated {
                        this.report(&message, cx);
                    }
                    this.load(cx);
                }
                Ok((false, _)) => this.report(&tr!("overlays_toast_missing"), cx),
                Err(message) => this.report(&message, cx),
            },
            cx,
        );
    }

    fn prompt_delete(&mut self, id: OverlayId, cx: &mut Context<Self>) {
        self.menu_open = None;
        if let Some(index) = self.index_of(&id) {
            self.pending_delete.request(PendingDelete {
                display_name: self.overlays[index].display_name.clone(),
                id,
            });
        }
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete.cancel();
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.pending_delete.take() else {
            return;
        };
        if self.selected.as_ref() == Some(&prompt.id) {
            self.selected = None;
            self.clear_test();
        }
        let repo = Arc::clone(&self.repo);
        let service = self.service.clone();
        let id = prompt.id;
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let removed = repo.delete(&id).await.map_err(|e| e.to_string())?;
                if !removed {
                    return Ok((false, None));
                }
                let swept = service
                    .remove_folder(&id)
                    .await
                    .err()
                    .map(|e| e.to_string());
                Ok((true, swept))
            },
            |this, result: Result<(bool, Option<String>), String>, cx| match result {
                Ok((true, swept)) => {
                    cx.push_toast(ToastKind::Success, tr!("overlays_toast_deleted"));
                    if let Some(message) = swept {
                        this.report(&message, cx);
                    }
                    this.load(cx);
                }
                Ok((false, _)) => this.report(&tr!("overlays_toast_missing"), cx),
                Err(message) => this.report(&message, cx),
            },
            cx,
        );
        cx.notify();
    }

    fn toggle_menu(&mut self, id: &OverlayId, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.menu_open = if self.menu_open.as_ref() == Some(id) {
            None
        } else {
            self.menu_click_pos = Some(position);
            Some(id.clone())
        };
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
    }

    fn render_header_right(&self, palette: &ForgePalette) -> AnyElement {
        let (dot, summary) = match self.bind_address.as_deref().filter(|_| self.server_running) {
            Some(address) => (
                palette.success,
                tr!(
                    "overlays_header_summary",
                    enabled = self.enabled_count() as i64,
                    total = self.overlays.len() as i64,
                    port = crate::overlay_url::extract_port(address)
                ),
            ),
            None => (
                palette.text_faint,
                tr!(
                    "overlays_header_summary_stopped",
                    enabled = self.enabled_count() as i64,
                    total = self.overlays.len() as i64
                ),
            ),
        };

        div()
            .flex()
            .items_center()
            .gap(HEADER_GAP)
            .child(icon(Icon::Browser, HEADER_GLYPH, dot))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(summary),
            )
            .into_any_element()
    }

    fn render_delete_confirm(
        &self,
        prompt: &PendingDelete,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let card = confirm_modal(
            tr!("overlays_confirm_delete_title"),
            tr!("overlays_confirm_delete_body"),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(prompt.display_name.clone())
        .on_cancel(
            "overlays-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "overlays-delete-confirm",
            tr!("common_delete"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let weak = cx.entity().downgrade();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .dismiss_on_escape(&self.overlay_focus)
            .on_dismiss("overlays-delete-dismiss", move |_window, cx| {
                let _ = weak.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }
}

impl Render for OverlaysView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        drive_overlay_focus(
            self.pending_delete.is_pending(),
            &self.overlay_focus,
            &mut self.focus_restore,
            window,
            cx,
        );

        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_row()
            .child(self.render_registry_pane(&palette, cx))
            .child(self.render_editor_pane(&palette, cx))
            .children(self.render_property_pane(&palette));

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("overlays_breadcrumb_builtin")),
                BreadcrumbCrumb::leaf(tr!("overlays_breadcrumb_overlays")),
            ],
            &palette,
        )
        .header_right(self.render_header_right(&palette))
        .density(density)
        .body(body);

        let delete = self
            .pending_delete
            .get()
            .map(|prompt| self.render_delete_confirm(prompt, &palette, cx));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(self.form.as_ref().map(|open| open.view.clone()))
            .children(delete)
    }
}
