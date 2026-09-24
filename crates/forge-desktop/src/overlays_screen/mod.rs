mod code_pane;
mod editor_pane;
mod event_wiring;
mod form_modal;
mod icon_choice;
mod kind_visuals;
mod preview_shapes;
mod preview_stage;
mod property_panel;
mod registry_pane;
mod sound_choice;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use forge_components::{
    BreadcrumbCrumb, Confirm, ConfirmTone, FONT_XS, ForgePalette, Icon, OverlayPosition, ToastKind,
    body_family, confirm_modal, icon, overlay, page_frame, tr,
};
use forge_overlay::config::SOUND_OPTIONS_KEY;
use forge_overlay::{
    ConfigSection, MediaIssue, OverlayKindRegistry, SectionedField, effective_overlay_config,
};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::actions::ActionsService;
use forge_runtime::{OverlayServiceHandle, QueueSchedulerHandle};
use forge_server::ServerHandle;
use forge_soundboard::{ClipLibrary, SoundboardError};
use forge_storage::{
    MediaRepo, OverlayConfig, OverlayDefinition, OverlayId, OverlayRepo, SettingsRepo, StoredClip,
};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, Pixels, Point, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::async_bridge;
use crate::overlay_url::{overlay_origin, overlay_page_url, resolve_routable_host};
use crate::presentation::ActivePresentation;
use crate::sidebar::NavRequested;
use crate::toasts::{PushToast, copy_to_clipboard};

use code_pane::{CodeState, LeaveIntent};
use event_wiring::{EventWiringView, WiringLaunch};
use form_modal::{OverlayFormEvent, OverlayFormLaunch, OverlayFormModal, OverlayTypeChoice};
use icon_choice::{IconImage, OpenIconPicker};
use kind_visuals::{KindVisuals, kind_visuals};
use preview_stage::{StageState, TestFireRun};
use property_panel::{AdoptClipRequested, OverlayPropertyPanel, PanelLaunch, PropertyPanelEvent};

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
    _media_sub: Subscription,
    _icon_sub: Subscription,
}

struct PendingDelete {
    id: OverlayId,
    display_name: String,
}

struct WiringLink {
    view: Entity<EventWiringView>,
    _nav: Subscription,
    _repaint: Subscription,
}

struct ServedEndpoint {
    bind_address: String,
    routable_host: Option<String>,
}

/// What a regeneration pass leaves for the screen to say: a failure to report, and the claimed
/// files whose copy is gone from disk.
#[derive(Default)]
struct Regenerated {
    failure: Option<String>,
    missing: Vec<String>,
    media_issues: Vec<MediaIssue>,
}

/// `false` means the record is gone, so the caller reports a miss rather than recreating it.
pub(super) async fn store_config(
    repo: &dyn OverlayRepo,
    id: &OverlayId,
    config: OverlayConfig,
) -> Result<bool, String> {
    let Some(mut definition) = repo.get(id).await.map_err(|e| e.to_string())? else {
        return Ok(false);
    };
    definition.config = config;
    repo.save(&definition).await.map_err(|e| e.to_string())?;
    Ok(true)
}

async fn regenerate(service: &OverlayServiceHandle, id: &OverlayId) -> Regenerated {
    match service.materialize(id).await {
        Ok(report) => Regenerated {
            failure: None,
            missing: report.missing_overrides,
            media_issues: report.media_issues,
        },
        Err(error) => Regenerated {
            failure: Some(error.to_string()),
            missing: Vec::new(),
            media_issues: Vec::new(),
        },
    }
}

pub struct OverlaysView {
    repo: Arc<dyn OverlayRepo>,
    server: Option<ServerHandle>,
    rt_handle: tokio::runtime::Handle,
    kinds: Arc<OverlayKindRegistry>,
    service: OverlayServiceHandle,
    library: Arc<ClipLibrary>,
    media: Arc<dyn MediaRepo>,
    settings_repo: Arc<dyn SettingsRepo>,
    clip_choices: Vec<(String, String)>,
    clips_gen: async_bridge::Generation,
    icon_images: Vec<IconImage>,
    icon_favorites: HashSet<SharedString>,
    icon_picker: Option<OpenIconPicker>,
    images_gen: async_bridge::Generation,
    media_issues: HashMap<OverlayId, Vec<MediaIssue>>,
    overlays: Vec<OverlayDefinition>,
    selected: Option<OverlayId>,
    mode: EditorMode,
    code: CodeState,
    panel: Option<OpenPanel>,
    loading: bool,
    server_running: bool,
    bind_address: Option<String>,
    routable_host: Option<String>,
    menu_open: Option<OverlayId>,
    menu_click_pos: Option<Point<Pixels>>,
    form: Option<OpenForm>,
    pending_delete: Confirm<PendingDelete>,
    fire: Option<TestFireRun>,
    fire_epoch: u64,
    stage: StageState,
    wiring: WiringLink,
}

pub struct OverlaysLaunch {
    pub repo: Arc<dyn OverlayRepo>,
    pub server: Option<ServerHandle>,
    pub rt_handle: tokio::runtime::Handle,
    pub kinds: Arc<OverlayKindRegistry>,
    pub service: OverlayServiceHandle,
    pub library: Arc<ClipLibrary>,
    pub media: Arc<dyn MediaRepo>,
    pub settings_repo: Arc<dyn SettingsRepo>,
    pub actions: Arc<ActionsService>,
    pub triggers: Arc<TriggerRegistry>,
    pub sub_actions: Arc<SubActionRegistry>,
    pub scheduler: QueueSchedulerHandle,
}

impl OverlaysView {
    pub fn new(launch: OverlaysLaunch, cx: &mut Context<Self>) -> Self {
        let server_running = launch
            .server
            .as_ref()
            .is_some_and(|handle| *handle.run_state().borrow());

        let code = CodeState::new(cx);
        let wiring = cx.new(|_| {
            EventWiringView::new(WiringLaunch {
                actions: Arc::clone(&launch.actions),
                triggers: Arc::clone(&launch.triggers),
                sub_actions: Arc::clone(&launch.sub_actions),
                scheduler: launch.scheduler,
                kinds: Arc::clone(&launch.kinds),
                rt_handle: launch.rt_handle.clone(),
            })
        });
        let nav = cx.subscribe(&wiring, |_, _, event: &NavRequested, cx| {
            cx.emit(NavRequested(event.0.clone()));
        });
        let repaint = cx.observe(&wiring, |_, _, cx| cx.notify());

        let mut view = Self {
            repo: launch.repo,
            server: launch.server,
            rt_handle: launch.rt_handle,
            kinds: launch.kinds,
            service: launch.service,
            library: launch.library,
            media: launch.media,
            settings_repo: launch.settings_repo,
            clip_choices: Vec::new(),
            clips_gen: async_bridge::Generation::default(),
            icon_images: Vec::new(),
            icon_favorites: HashSet::new(),
            icon_picker: None,
            images_gen: async_bridge::Generation::default(),
            media_issues: HashMap::new(),
            overlays: Vec::new(),
            selected: None,
            mode: EditorMode::Design,
            code,
            panel: None,
            loading: false,
            server_running,
            bind_address: None,
            routable_host: None,
            menu_open: None,
            menu_click_pos: None,
            form: None,
            pending_delete: Confirm::default(),
            fire: None,
            fire_epoch: 0,
            stage: StageState::default(),
            wiring: WiringLink {
                view: wiring,
                _nav: nav,
                _repaint: repaint,
            },
        };
        view.load(cx);
        view.load_clips(cx);
        view.load_images(cx);
        view.load_icon_favorites(cx);
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
        let origin = overlay_origin(self.bind_address.as_deref()?, self.routable_host.as_deref());
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

    fn apply_regenerated(
        &mut self,
        id: &OverlayId,
        regenerated: Regenerated,
        cx: &mut Context<Self>,
    ) {
        self.note_missing_overrides(regenerated.missing);
        self.set_media_issues(id, regenerated.media_issues, cx);
        if let Some(message) = regenerated.failure {
            self.report(&message, cx);
        }
    }

    fn set_media_issues(
        &mut self,
        id: &OverlayId,
        issues: Vec<MediaIssue>,
        cx: &mut Context<Self>,
    ) {
        if issues.is_empty() {
            self.media_issues.remove(id);
        } else {
            self.media_issues.insert(id.clone(), issues.clone());
        }
        let Some(panel) = self
            .panel
            .as_ref()
            .filter(|open| open.view.read(cx).overlay_id() == id)
            .map(|open| open.view.clone())
        else {
            return;
        };
        panel.update(cx, |panel, cx| panel.set_media_issues(issues, cx));
    }

    fn issues_of(&self, id: &OverlayId) -> &[MediaIssue] {
        self.media_issues
            .get(id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn media_badge(&self, id: &OverlayId) -> Option<String> {
        sound_choice::badge_note(self.issues_of(id))
    }

    fn load_clips(&mut self, cx: &mut Context<Self>) {
        let ticket = self.clips_gen.next();
        let library = Arc::clone(&self.library);
        async_bridge::run_async(
            &self.rt_handle,
            async move { library.list().await },
            move |this, result: Result<Vec<StoredClip>, SoundboardError>, cx| {
                if !this.clips_gen.is_current(ticket) {
                    return;
                }
                match result {
                    Ok(clips) => this.apply_clips(&clips, cx),
                    Err(error) => {
                        tracing::warn!(%error, "soundboard clips unavailable for the sound picker");
                    }
                }
            },
            cx,
        );
    }

    fn apply_clips(&mut self, clips: &[StoredClip], cx: &mut Context<Self>) {
        self.clip_choices = sound_choice::clip_choices(clips);
        let choices = self.clip_choices.clone();
        let Some(panel) = self.panel.as_ref().map(|open| open.view.clone()) else {
            return;
        };
        panel.update(cx, |panel, cx| panel.set_sound_choices(choices, cx));
    }

    fn on_adopt_requested(
        &mut self,
        view: Entity<OverlayPropertyPanel>,
        event: &AdoptClipRequested,
        cx: &mut Context<Self>,
    ) {
        let library = Arc::clone(&self.library);
        let clip = event.clip;
        let key = event.key.clone();
        let value = event.value.clone();
        async_bridge::run_async_entity(
            &self.rt_handle,
            view,
            async move {
                let Some(row) = library.get(clip).await? else {
                    return Ok(None);
                };
                let verdict = library.adopt_now(&row).await?;
                Ok::<_, SoundboardError>(Some((row.name, verdict)))
            },
            move |panel, result, cx| {
                panel.settle_clip_pick(key, value, sound_choice::pick_outcome(result), cx);
            },
            cx,
        );
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
                    let bind_address = probe.bind_addr().await.to_string();
                    let routable_host = resolve_routable_host(&bind_address);
                    let _ = tx.send(ServedEndpoint {
                        bind_address,
                        routable_host,
                    });
                });
                let endpoint = rx.await.ok();
                if this
                    .update(cx, |this, cx| {
                        this.apply_server_state(running, endpoint, cx)
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
        endpoint: Option<ServedEndpoint>,
        cx: &mut Context<Self>,
    ) {
        let mut changed = self.server_running != running;
        self.server_running = running;
        if let Some(endpoint) = endpoint
            && (self.bind_address.as_deref() != Some(endpoint.bind_address.as_str())
                || self.routable_host != endpoint.routable_host)
        {
            self.bind_address = Some(endpoint.bind_address);
            self.routable_host = endpoint.routable_host;
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
                let live = &self.overlays;
                self.media_issues
                    .retain(|id, _| live.iter().any(|item| &item.id == id));
                let selection_survived = self
                    .selected
                    .as_ref()
                    .is_some_and(|id| self.index_of(id).is_some());
                if !selection_survived {
                    self.selected = self.overlays.first().map(|item| item.id.clone());
                    self.clear_test();
                }
                self.sync_panel(cx);
                self.sync_preview();
                self.sync_source(cx);
                self.sync_wiring(cx);
            }
            Err(message) => self.report(&message, cx),
        }
        cx.notify();
    }

    fn sync_wiring(&mut self, cx: &mut Context<Self>) {
        let selection = self.selected_definition().cloned();
        self.wiring
            .view
            .update(cx, |wiring, cx| wiring.focus_overlay(selection, cx));
    }

    fn select(&mut self, id: OverlayId, cx: &mut Context<Self>) {
        if self.selected.as_ref() == Some(&id) {
            return;
        }
        if self.code_dirty(cx) {
            self.request_leave(LeaveIntent::Overlay(id), cx);
            return;
        }
        self.selected = Some(id);
        self.clear_test();
        self.sync_panel(cx);
        self.sync_preview();
        self.sync_source(cx);
        self.sync_wiring(cx);
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
        if mode == EditorMode::Design && self.code_dirty(cx) {
            self.request_leave(LeaveIntent::Design, cx);
            return;
        }
        self.mode = mode;
        self.sync_source(cx);
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

        let effective = effective_overlay_config(descriptor, &definition.config);
        let specs: Vec<SectionedField> = descriptor
            .config_fields()
            .into_iter()
            .filter(|sectioned| {
                !(descriptor.content_is_machine_filled()
                    && sectioned.section == ConfigSection::Content)
            })
            .collect();

        let launch = PanelLaunch {
            overlay_id: definition.id.clone(),
            specs,
            defaults: descriptor.default_config(),
            stored: definition.config.clone(),
            effective,
            choices: HashMap::from([(SOUND_OPTIONS_KEY.to_owned(), self.clip_choices.clone())]),
            icon_images: self.icon_images.clone(),
            overridden_files: definition.source_overrides.clone(),
            repo: Arc::clone(&self.repo),
            service: self.service.clone(),
            rt_handle: self.rt_handle.clone(),
        };

        let issues = self.issues_of(&definition.id).to_vec();
        let view = cx.new(|cx| OverlayPropertyPanel::new(launch, cx));
        view.update(cx, |panel, cx| panel.set_media_issues(issues, cx));
        let sub = cx.subscribe(&view, Self::on_panel_event);
        let media_sub = cx.subscribe(&view, Self::on_adopt_requested);
        let icon_sub = cx.subscribe(&view, Self::on_icon_pick_requested);
        self.panel = Some(OpenPanel {
            view,
            _sub: sub,
            _media_sub: media_sub,
            _icon_sub: icon_sub,
        });
        self.load_clips(cx);
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
        let target = id.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                if !store_config(repo.as_ref(), &id, config).await? {
                    return Ok((false, Regenerated::default()));
                }
                Ok((true, regenerate(&service, &id).await))
            },
            move |this, result: Result<(bool, Regenerated), String>, cx| match result {
                Ok((true, regenerated)) => {
                    this.apply_regenerated(&target, regenerated, cx);
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

        let service = self.service.clone();
        let target = id.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                service
                    .set_enabled(&target, next)
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
                let regenerated = regenerate(&service, &definition.id).await;
                Ok((definition, regenerated))
            },
            |this, result: Result<(OverlayDefinition, Regenerated), String>, cx| match result {
                Ok((definition, regenerated)) => {
                    let created = definition.id;
                    this.selected = Some(created.clone());
                    cx.push_toast(ToastKind::Success, tr!("overlays_toast_created"));
                    this.apply_regenerated(&created, regenerated, cx);
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
        let target = id.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let Some(mut definition) = repo.get(&id).await.map_err(|e| e.to_string())? else {
                    return Ok((false, Regenerated::default()));
                };
                definition.display_name = display_name;
                repo.save(&definition).await.map_err(|e| e.to_string())?;
                Ok((true, regenerate(&service, &id).await))
            },
            move |this, result: Result<(bool, Regenerated), String>, cx| match result {
                Ok((true, regenerated)) => {
                    cx.push_toast(ToastKind::Success, tr!("overlays_toast_renamed"));
                    this.apply_regenerated(&target, regenerated, cx);
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
                id: id.clone(),
            });
            self.wiring
                .view
                .update(cx, |wiring, cx| wiring.count_for_delete(id, cx));
        }
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete.cancel();
        self.wiring
            .view
            .update(cx, |wiring, _| wiring.clear_delete_count());
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.pending_delete.take() else {
            return;
        };
        self.wiring
            .view
            .update(cx, |wiring, _| wiring.clear_delete_count());
        if self.selected.as_ref() == Some(&prompt.id) {
            self.selected = None;
            self.clear_test();
        }
        let service = self.service.clone();
        let id = prompt.id;
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let removed = service.delete(&id).await.map_err(|e| e.to_string())?;
                if !removed {
                    return Ok((false, None));
                }
                if let Err(error) = service.release_media(&id).await {
                    tracing::warn!(overlay = %id, %error, "overlay media references not released");
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

    fn delete_body(&self, id: &OverlayId, cx: &Context<Self>) -> String {
        let base = tr!("overlays_confirm_delete_body");
        match self.wiring.view.read(cx).delete_feed_count(id) {
            Some(count) => format!(
                "{base} {}",
                tr!("overlays_confirm_delete_feeds", count = count as i64)
            ),
            None => base,
        }
    }

    fn render_delete_confirm(
        &self,
        prompt: &PendingDelete,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let card = confirm_modal(
            tr!("overlays_confirm_delete_title"),
            self.delete_body(&prompt.id, cx),
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

        self.focus_icon_picker(window, cx);

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
            .child(self.wiring.view.clone())
            .children(self.render_icon_picker(&palette, cx))
            .children(self.render_code_confirms(&palette, cx))
    }
}

impl EventEmitter<NavRequested> for OverlaysView {}
