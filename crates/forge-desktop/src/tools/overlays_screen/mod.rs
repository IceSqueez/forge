mod base_sections;
mod bindings_panel;
mod catalog;
mod code_pane;
mod color_choice;
mod color_removal;
mod custom_accents;
mod delete_prompt;
mod editor_pane;
mod editor_state;
mod event_wiring;
mod form_link;
mod form_modal;
mod hide_timing;
pub(crate) mod icon_choice;
pub(crate) mod icon_picker_host;
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
mod shortcuts;
mod sizing_section;
mod sound_choice;
mod voice_choice;
mod writes;

use std::sync::Arc;

use self::color_removal::{ColorRemoval, ColorRemovalLaunch};
use self::custom_accents::CustomAccents;

use forge_components::{
    ActivePresentation, BreadcrumbCrumb, Confirm, Density, FONT_XS, ForgePalette, Icon, ToastKind,
    icon, page_frame, scaled, tr,
};
use forge_overlay::OverlayKindRegistry;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::ActionsService;
use forge_runtime::{
    DateNamesHandle, EventBus, LatestValues, OverlaySampleSource, OverlayServiceHandle,
    QueueSchedulerHandle,
};
use forge_server::ServerHandle;
use forge_soundboard::ClipLibrary;
use forge_speak_queue::SpeakQueueHandle;
use forge_storage::{MediaRepo, OverlayDefinition, OverlayId, OverlayRepo, SettingsRepo};
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, Pixels, Subscription, Window, div, prelude::*,
    px,
};

use crate::app::navigable_list::ListCursor;
use crate::app::navigation::NavRequested;
use crate::app::navigation::Screen;
use crate::integrations::hub_crumb::{core_category, hub_crumb};
use crate::shared::picker_favorites::StoredFavorites;
use crate::shared::repo_failure::RepoFailure;
use forge_components::PushToast;
use forge_host::audio_router::AudioRouter;

use catalog::MediaCatalog;
use code_pane::{CodeServices, OverlayCodeEditor};
use delete_prompt::PendingDelete;
use editor_state::EditorState;
use event_wiring::{EventWiringView, WiringLaunch};
use form_link::OpenForm;
use icon_choice::ICON_FAVORITES;
use icon_picker_host::IconPickerHost;
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
    samples: Arc<dyn OverlaySampleSource>,
    date_names: DateNamesHandle,
    library: Arc<ClipLibrary>,
    settings_repo: Arc<dyn SettingsRepo>,
    audio_router: Arc<AudioRouter>,
    speak: Option<SpeakQueueHandle>,
}

#[derive(Debug)]
enum OverlayProblem {
    Missing,
    UnknownType,
    TokenShapedName,
    ReceiverNotSaved(RepoFailure),
    Failed(RepoFailure),
}

impl OverlayProblem {
    fn text(&self) -> String {
        match self {
            Self::Missing => tr!("overlays_toast_missing"),
            Self::UnknownType => tr!("overlays_toast_unknown_type"),
            Self::TokenShapedName => tr!("overlays_toast_token_shaped_name"),
            Self::ReceiverNotSaved(failure) => {
                tr!("overlays_receiver_save_failed", reason = failure.text())
            }
            Self::Failed(failure) => failure.text(),
        }
    }
}

fn report_problem(problem: OverlayProblem, cx: &mut App) {
    tracing::warn!(?problem, "overlay registry operation failed");
    cx.push_toast(ToastKind::Error, problem.text());
}

impl From<RepoFailure> for OverlayProblem {
    fn from(failure: RepoFailure) -> Self {
        Self::Failed(failure)
    }
}

pub(crate) struct OverlaysView {
    handles: OverlayHandles,
    registry: RegistryState,
    registry_keys: ListCursor<OverlayId>,
    served: ServerStatus,
    editor: EditorState,
    catalog: MediaCatalog,
    icon_host: Entity<IconPickerHost>,
    custom_accents: Entity<CustomAccents>,
    color_removal: Entity<ColorRemoval>,
    stage: StageState,
    form: Option<OpenForm>,
    pending_delete: Confirm<PendingDelete>,
    wiring: WiringLink,
    receiver: Option<OverlayId>,
    latest: LatestSource,
    _child_subscriptions: [Subscription; 7],
}

pub(crate) struct OverlaysLaunch {
    pub repo: Arc<dyn OverlayRepo>,
    pub server: Option<ServerHandle>,
    pub rt_handle: tokio::runtime::Handle,
    pub kinds: Arc<OverlayKindRegistry>,
    pub service: OverlayServiceHandle,
    pub samples: Arc<dyn OverlaySampleSource>,
    pub date_names: DateNamesHandle,
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
    pub(crate) fn new(launch: OverlaysLaunch, cx: &mut Context<Self>) -> Self {
        let server_running = launch
            .server
            .as_ref()
            .is_some_and(|handle| handle.run_state().is_running());

        let editor = EditorState::new(
            CodeServices {
                service: launch.service.clone(),
                samples: Arc::clone(&launch.samples),
                rt_handle: launch.rt_handle.clone(),
            },
            cx,
        );
        let wiring = cx.new(|cx| {
            EventWiringView::new(
                WiringLaunch {
                    actions: Arc::clone(&launch.actions),
                    triggers: Arc::clone(&launch.triggers),
                    sub_actions: Arc::clone(&launch.sub_actions),
                    scheduler: launch.scheduler,
                    kinds: Arc::clone(&launch.kinds),
                    settings: Arc::clone(&launch.settings_repo),
                    rt_handle: launch.rt_handle.clone(),
                },
                cx,
            )
        });
        let nav = cx.subscribe(&wiring, |_, _, event: &NavRequested, cx| {
            cx.emit(NavRequested(event.0.clone()));
        });
        let repaint = cx.observe(&wiring, |_, _, cx| cx.notify());

        let icon_favorites = StoredFavorites::shared(
            ICON_FAVORITES,
            Arc::clone(&launch.settings_repo),
            &launch.rt_handle,
            cx,
        );
        let custom_accents = cx.new(|cx| {
            CustomAccents::new(
                Arc::clone(&launch.settings_repo),
                launch.rt_handle.clone(),
                cx,
            )
        });
        let color_removal = cx.new(|_| {
            ColorRemoval::new(ColorRemovalLaunch {
                repo: Arc::clone(&launch.repo),
                settings: Arc::clone(&launch.settings_repo),
                service: launch.service.clone(),
                samples: Arc::clone(&launch.samples),
                kinds: Arc::clone(&launch.kinds),
                rt_handle: launch.rt_handle.clone(),
                accents: custom_accents.clone(),
            })
        });
        let icon_host = cx.new(|cx| {
            IconPickerHost::new(launch.media, launch.rt_handle.clone(), icon_favorites, cx)
        });
        let child_subscriptions = [
            cx.subscribe(&color_removal, Self::on_color_removal),
            cx.observe(&color_removal, |_, _: Entity<ColorRemoval>, cx| cx.notify()),
            cx.subscribe(&editor.code, Self::on_code_event),
            cx.observe(&editor.code, |_, _: Entity<OverlayCodeEditor>, cx| {
                cx.notify();
            }),
            cx.subscribe(&icon_host, Self::on_icon_host_event),
            cx.observe(&custom_accents, Self::on_custom_accents_changed),
            cx.observe(&icon_host, |_, _: Entity<IconPickerHost>, cx| cx.notify()),
        ];
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
                samples: launch.samples,
                date_names: launch.date_names,
                library: launch.library,
                settings_repo: launch.settings_repo,
                audio_router: launch.audio_router,
                speak: launch.speak,
            },
            registry: RegistryState::default(),
            registry_keys: ListCursor::reachable_by_tab(cx),
            served: ServerStatus::new(server_running),
            editor,
            catalog: MediaCatalog::default(),
            icon_host,
            custom_accents,
            color_removal,
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
            _child_subscriptions: child_subscriptions,
        };
        view.load(cx);
        view.load_receiver(cx);
        view.load_clips(cx);
        view.start_server_bridge(cx);
        view
    }

    fn visuals(&self, definition: &OverlayDefinition, palette: &ForgePalette) -> KindVisuals {
        kind_visuals(definition, &self.handles.kinds, palette)
    }

    fn report(&mut self, problem: OverlayProblem, cx: &mut Context<Self>) {
        report_problem(problem, cx);
        cx.notify();
    }

    fn render_header_right(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &App,
    ) -> AnyElement {
        let (dot, summary) = match self.served.bind_address.filter(|_| self.served.running) {
            Some(address) => (
                palette.success,
                tr!(
                    "overlays_header_summary",
                    enabled = i64::try_from(self.enabled_count()).unwrap_or(i64::MAX),
                    total = i64::try_from(self.registry.overlays.len()).unwrap_or(i64::MAX),
                    port = address.port()
                ),
            ),
            None => (
                palette.text_faint,
                tr!(
                    "overlays_header_summary_stopped",
                    enabled = i64::try_from(self.enabled_count()).unwrap_or(i64::MAX),
                    total = i64::try_from(self.registry.overlays.len()).unwrap_or(i64::MAX)
                ),
            ),
        };

        div()
            .flex()
            .items_center()
            .gap(scaled(HEADER_GAP, density))
            .child(icon(Icon::Browser, HEADER_GLYPH, dot))
            .child(
                div()
                    .font_family(cx.body_family())
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

        self.icon_host
            .update(cx, |host, cx| host.focus_picker(window, cx));

        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_row()
            .child(self.render_registry_pane(&palette, cx))
            .child(self.render_editor_pane(&palette, cx))
            .children(self.render_property_pane(&palette, density, cx));

        let frame = page_frame(
            vec![
                hub_crumb(core_category(&Screen::Overlays), cx),
                BreadcrumbCrumb::leaf(tr!("overlays_breadcrumb_overlays")),
            ],
            &palette,
        )
        .header_right(self.render_header_right(&palette, density, cx))
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
            .children(
                self.icon_host
                    .update(cx, |host, cx| host.render_overlay(&palette, cx)),
            )
            .children(self.render_code_confirms(&palette, cx))
            .children(
                self.color_removal
                    .update(cx, |removal, cx| removal.render_confirm(&palette, cx)),
            )
    }
}

impl EventEmitter<NavRequested> for OverlaysView {}
