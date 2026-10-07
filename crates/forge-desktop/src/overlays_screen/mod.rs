mod base_sections;
mod bindings_panel;
mod catalog;
mod code_pane;
mod delete_prompt;
mod editor_pane;
mod editor_state;
mod event_wiring;
mod form_link;
mod form_modal;
mod hide_timing;
mod icon_choice;
mod kind_visuals;
mod latest_section;
mod look_change;
mod motion_notices;
mod motion_timing;
mod obs_card;
mod preview_shapes;
mod preview_stage;
mod property_panel;
mod receiver;
mod registry;
mod registry_pane;
mod server_status;
mod sizing_section;
mod sound_choice;
mod voice_choice;
mod writes;

use std::sync::Arc;

use forge_components::{
    BreadcrumbCrumb, Confirm, FONT_XS, ForgePalette, Icon, ToastKind, body_family, icon,
    page_frame, tr,
};
use forge_overlay::OverlayKindRegistry;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::actions::ActionsService;
use forge_runtime::{EventBus, LatestValues, OverlayServiceHandle, QueueSchedulerHandle};
use forge_server::ServerHandle;
use forge_soundboard::ClipLibrary;
use forge_speak_queue::SpeakQueueHandle;
use forge_storage::{MediaRepo, OverlayDefinition, OverlayId, OverlayRepo, SettingsRepo};
use gpui::{
    AnyElement, Context, Entity, EventEmitter, Pixels, Subscription, Window, div, prelude::*, px,
};

use crate::audio_router::AudioRouter;
use crate::hub_crumb::{core_category, hub_crumb};
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;
use crate::toasts::PushToast;

use catalog::MediaCatalog;
use delete_prompt::PendingDelete;
use editor_state::EditorState;
use event_wiring::{EventWiringView, WiringLaunch};
use form_link::OpenForm;
use kind_visuals::{KindVisuals, kind_visuals};
use latest_section::LatestSource;
use preview_stage::StageState;
use registry::RegistryState;
use server_status::ServerStatus;

const HEADER_GAP: Pixels = px(5.0);
const HEADER_GLYPH: Pixels = px(13.0);

struct WiringLink {
    view: Entity<EventWiringView>,
    _nav: Subscription,
    _repaint: Subscription,
}

struct OverlayHandles {
    repo: Arc<dyn OverlayRepo>,
    server: Option<ServerHandle>,
    rt_handle: tokio::runtime::Handle,
    kinds: Arc<OverlayKindRegistry>,
    service: OverlayServiceHandle,
    library: Arc<ClipLibrary>,
    media: Arc<dyn MediaRepo>,
    settings_repo: Arc<dyn SettingsRepo>,
    audio_router: Arc<AudioRouter>,
    speak: Option<SpeakQueueHandle>,
}

pub struct OverlaysView {
    handles: OverlayHandles,
    registry: RegistryState,
    served: ServerStatus,
    editor: EditorState,
    catalog: MediaCatalog,
    stage: StageState,
    form: Option<OpenForm>,
    pending_delete: Confirm<PendingDelete>,
    wiring: WiringLink,
    receiver: Option<OverlayId>,
    latest: LatestSource,
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
    pub audio_router: Arc<AudioRouter>,
    pub speak: Option<SpeakQueueHandle>,
    pub actions: Arc<ActionsService>,
    pub triggers: Arc<TriggerRegistry>,
    pub sub_actions: Arc<SubActionRegistry>,
    pub scheduler: QueueSchedulerHandle,
    pub latest_values: LatestValues,
    pub bus: Arc<EventBus>,
}

impl OverlaysView {
    pub fn new(launch: OverlaysLaunch, cx: &mut Context<Self>) -> Self {
        let server_running = launch
            .server
            .as_ref()
            .is_some_and(|handle| *handle.run_state().borrow());

        let editor = EditorState::new(cx);
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

        let latest = LatestSource {
            values: launch.latest_values,
            bus: launch.bus,
            kinds: Arc::clone(&launch.kinds),
        };
        let mut view = Self {
            handles: OverlayHandles {
                repo: launch.repo,
                server: launch.server,
                rt_handle: launch.rt_handle,
                kinds: launch.kinds,
                service: launch.service,
                library: launch.library,
                media: launch.media,
                settings_repo: launch.settings_repo,
                audio_router: launch.audio_router,
                speak: launch.speak,
            },
            registry: RegistryState::default(),
            served: ServerStatus::new(server_running),
            editor,
            catalog: MediaCatalog::default(),
            stage: StageState::default(),
            form: None,
            pending_delete: Confirm::default(),
            wiring: WiringLink {
                view: wiring,
                _nav: nav,
                _repaint: repaint,
            },
            receiver: None,
            latest,
        };
        view.load(cx);
        view.load_receiver(cx);
        view.load_clips(cx);
        view.load_images(cx);
        view.load_icon_favorites(cx);
        view.start_server_bridge(cx);
        view
    }

    fn visuals(&self, definition: &OverlayDefinition, palette: &ForgePalette) -> KindVisuals {
        kind_visuals(definition, &self.handles.kinds, palette)
    }

    fn report(&mut self, message: &str, cx: &mut Context<Self>) {
        tracing::warn!(error = %message, "overlay registry operation failed");
        cx.push_toast(ToastKind::Error, message.to_owned());
        cx.notify();
    }

    fn render_header_right(&self, palette: &ForgePalette) -> AnyElement {
        let (dot, summary) = match self
            .served
            .bind_address
            .as_deref()
            .filter(|_| self.served.running)
        {
            Some(address) => (
                palette.success,
                tr!(
                    "overlays_header_summary",
                    enabled = self.enabled_count() as i64,
                    total = self.registry.overlays.len() as i64,
                    port = crate::overlay_url::extract_port(address)
                ),
            ),
            None => (
                palette.text_faint,
                tr!(
                    "overlays_header_summary_stopped",
                    enabled = self.enabled_count() as i64,
                    total = self.registry.overlays.len() as i64
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
                hub_crumb(core_category(&Screen::Overlays), cx),
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
            .children(self.render_latest_reset_confirm(&palette, cx))
            .child(self.wiring.view.clone())
            .children(self.render_icon_picker(&palette, cx))
            .children(self.render_code_confirms(&palette, cx))
    }
}

impl EventEmitter<NavRequested> for OverlaysView {}
