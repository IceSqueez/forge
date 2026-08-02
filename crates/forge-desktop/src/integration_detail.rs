use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, Confirm, ConfirmTone, Density, FONT_SM, FONT_XS, ForgePalette,
    Icon, InputEvent, OverlayPosition, Radius, SearchState, Spacing, TextInput, ToastKind, badge,
    body_family, confirm_modal, icon, mono_family, overlay, page_frame, platform_hero, radius,
    spacing, status_dot, tr, with_alpha,
};
use forge_events::{EventPublisher, EventSource};
use forge_obs::ObsClient;
use forge_platform_core::{
    BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinStatus, CapabilityFlags, ConnectionState,
    ControlFailure, DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthValue, HeroBadge,
    HeroBadgeTone, QuickAction, QuickActions, SectionIcon,
};
use forge_registry::TriggerRegistry;
use forge_runtime::{ActionEngineHandle, EventBus, LiveViewerAggregatorHandle, LiveViewerCount};
use forge_storage::{CredentialsRepo, HistoryRepo, SettingsRepo};
use forge_types::{PlatformId, SubActionStep};
use futures_util::StreamExt as _;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, FontWeight, Rgba, Subscription, Window,
    div, prelude::*, px,
};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::async_bridge;
use crate::builtin_sections::{SectionRefresh, content_sections, health_grid};
use crate::connect_flow::{ConnectFlow, ConnectFlowEvent, ConnectFlowLaunch, ConnectedBundle};
use crate::integration_quick_action_modal::{QuickActionModal, QuickActionModalEvent};
use crate::integrations::{
    BuiltinObject, BuiltinRegistry, KickInstallSeed, ObsInstallSeed, VTubeInstallSeed,
    YoutubeInstallSeed, kick_builtin_object, twitch_builtin_object, youtube_builtin_object,
};
use crate::obs_credentials_form::ObsConnected;
use crate::obs_settings_modal::{ObsSettingsModal, ObsSettingsModalEvent};
use crate::platforms::PlatformConnectivity;
use crate::presentation::ActivePresentation;
use crate::run_history_modal::{RunHistoryDismissed, RunHistoryModal};
use crate::screen::Screen;
use crate::sidebar::NavRequested;
use crate::toasts::PushToast;

struct ActiveConnect {
    view: Entity<ConnectFlow>,
    _subs: Vec<Subscription>,
}

pub struct IntegrationDetail {
    status: Arc<dyn BuiltinStatus>,
    health: Arc<dyn BuiltinHealth>,
    content: Arc<dyn BuiltinContent>,
    quick: Arc<dyn QuickActions>,
    control: Option<Arc<dyn BuiltinControl>>,
    rt_handle: tokio::runtime::Handle,
    action_engine: ActionEngineHandle,
    obs_source: Option<Arc<ObsClient>>,
    credentials: Arc<dyn CredentialsRepo>,
    settings: Arc<dyn SettingsRepo>,
    history: Arc<dyn HistoryRepo>,
    trigger_registry: Arc<TriggerRegistry>,
    bus: Arc<dyn EventPublisher>,
    event_bus: Arc<EventBus>,
    live_viewers: LiveViewerAggregatorHandle,
    builtins: BuiltinRegistry,
    kick_install_seed: Option<KickInstallSeed>,
    youtube_install_seed: Option<YoutubeInstallSeed>,
    obs_install_seed: ObsInstallSeed,
    vtube_install_seed: VTubeInstallSeed,
    connect: Option<ActiveConnect>,
    is_twitch: bool,
    is_obs: bool,
    is_vtube: bool,
    twitch_reauth_required: bool,
    icon: SectionIcon,
    display_name: String,
    version: Option<String>,
    endpoint: Option<String>,
    connection: ConnectionState,
    capability_flags: CapabilityFlags,
    header_actions: Vec<HeaderAction>,
    health_metrics: [HealthMetric; 4],
    sections: Vec<DetailSection>,
    pub(crate) quick_actions: Vec<QuickAction>,
    pub(crate) qa_search: SearchState,
    eventsub_tally: HashMap<String, u64>,
    viewer_samples: VecDeque<(Instant, u64)>,
    pending_disconnect: Confirm<()>,
    quick_action_modal: Option<Entity<QuickActionModal>>,
    obs_settings_modal: Option<Entity<ObsSettingsModal>>,
    history_modal: Option<Entity<RunHistoryModal>>,
    _qa_modal_sub: Option<Subscription>,
    _obs_modal_sub: Option<Subscription>,
    _history_modal_sub: Option<Subscription>,
    _conn_obs: Subscription,
    _qa_search_sub: Subscription,
}

const VIEWER_DELTA_WINDOW: Duration = Duration::from_secs(15 * 60);
const VIEWER_RING_CAP: usize = 256;
const DETAIL_TICK: Duration = Duration::from_secs(30);
const OBS_CONNECTION_PREFIX: &str = "obs.connection.";
const HISTORY_LIMIT: u32 = 50;

pub struct ObsSignedOut;
pub struct VTubeSignedOut;

impl EventEmitter<NavRequested> for IntegrationDetail {}
impl EventEmitter<ObsConnected> for IntegrationDetail {}
impl EventEmitter<ObsSignedOut> for IntegrationDetail {}
impl EventEmitter<VTubeSignedOut> for IntegrationDetail {}

impl IntegrationDetail {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        object: BuiltinObject,
        rt_handle: tokio::runtime::Handle,
        action_engine: ActionEngineHandle,
        credentials: Arc<dyn CredentialsRepo>,
        settings: Arc<dyn SettingsRepo>,
        history: Arc<dyn HistoryRepo>,
        trigger_registry: Arc<TriggerRegistry>,
        bus: Arc<dyn EventPublisher>,
        event_bus: Arc<EventBus>,
        live_viewers: LiveViewerAggregatorHandle,
        builtins: BuiltinRegistry,
        kick_install_seed: Option<KickInstallSeed>,
        youtube_install_seed: Option<YoutubeInstallSeed>,
        obs_install_seed: ObsInstallSeed,
        vtube_install_seed: VTubeInstallSeed,
        connectivity: Entity<PlatformConnectivity>,
        cx: &mut Context<Self>,
    ) -> Self {
        let BuiltinObject {
            icon,
            status,
            health,
            content,
            quick,
            control,
            obs_client: obs_source,
        } = object;
        let conn_obs = cx.observe(&connectivity, |this, _, cx| this.reload(cx));

        let is_twitch = status.id().as_str() == "twitch";
        let is_obs = status.id().as_str() == "obs";
        let is_vtube = status.id().as_str() == "vtube";
        let palette = cx.palette();
        let qa_search = SearchState::from_field(cx.new(|cx| {
            forge_components::search_input(tr!("integration_qa_filter_placeholder"), palette, cx)
                .compact()
                .with_font_size(px(11.5))
        }));
        let qa_search_sub = cx.subscribe(qa_search.field(), Self::on_qa_search);
        let connect_platform = connect_platform_for(status.id().as_str(), control.is_some());
        let display_name = status.display_name().to_owned();
        let version = status.version().map(ToOwned::to_owned);
        let endpoint = status.endpoint().map(ToOwned::to_owned);
        let connection = status.connection();
        let capability_flags = status.capability_flags();
        let header_actions = status.header_actions();
        let health_metrics = health.metrics();
        let sections = content.sections();
        let quick_actions = quick.actions();

        let mut health_stream = health.stream();
        cx.spawn(async move |this, cx| {
            while let Some(delta) = health_stream.next().await {
                if this
                    .update(cx, |detail, cx| detail.apply_health_delta(delta, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        if is_twitch {
            Self::spawn_eventsub_tally(&event_bus, cx);
        }
        if is_obs {
            Self::spawn_obs_connection_watch(&event_bus, cx);
        }
        if is_twitch || matches!(status.id().as_str(), "youtube" | "kick") {
            Self::spawn_viewer_sampler(&live_viewers, cx);
            Self::spawn_detail_ticker(cx);
        }

        let mut this = Self {
            status,
            health,
            content,
            quick,
            control,
            rt_handle,
            action_engine,
            obs_source,
            credentials,
            settings,
            history,
            trigger_registry,
            bus,
            event_bus,
            live_viewers,
            builtins,
            kick_install_seed,
            youtube_install_seed,
            obs_install_seed,
            vtube_install_seed,
            connect: None,
            is_twitch,
            is_obs,
            is_vtube,
            twitch_reauth_required: false,
            icon,
            display_name,
            version,
            endpoint,
            connection,
            capability_flags,
            header_actions,
            health_metrics,
            sections,
            quick_actions,
            qa_search,
            eventsub_tally: HashMap::new(),
            viewer_samples: VecDeque::new(),
            pending_disconnect: Confirm::default(),
            quick_action_modal: None,
            obs_settings_modal: None,
            history_modal: None,
            _qa_modal_sub: None,
            _obs_modal_sub: None,
            _history_modal_sub: None,
            _conn_obs: conn_obs,
            _qa_search_sub: qa_search_sub,
        };

        if let Some(platform) = connect_platform {
            this.open_connect_flow(platform, cx);
        }
        this
    }

    fn open_connect_flow(&mut self, platform: PlatformId, cx: &mut Context<Self>) {
        let launch = ConnectFlowLaunch {
            platform,
            display_name: self.display_name.clone(),
            rt_handle: self.rt_handle.clone(),
            credentials: Arc::clone(&self.credentials),
            bus: Arc::clone(&self.bus),
            event_bus: Arc::clone(&self.event_bus),
            live_viewers: self.live_viewers.clone(),
            kick_install_seed: self.kick_install_seed.clone(),
            youtube_install_seed: self.youtube_install_seed.clone(),
        };
        let view = cx.new(|cx| ConnectFlow::new(launch, cx));
        let subs = vec![
            cx.subscribe(&view, Self::on_connect_flow_event),
            cx.observe(&view, |_, _, cx| cx.notify()),
        ];
        self.connect = Some(ActiveConnect { view, _subs: subs });
        cx.notify();
    }

    fn on_connect_flow_event(
        &mut self,
        _view: Entity<ConnectFlow>,
        event: &ConnectFlowEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ConnectFlowEvent::Connected(bundle) => self.adopt_connected(bundle, cx),
            ConnectFlowEvent::Leave => self.navigate_to(Screen::Platforms, cx),
        }
    }

    fn adopt_connected(&mut self, bundle: &ConnectedBundle, cx: &mut Context<Self>) {
        let object = match bundle {
            ConnectedBundle::Twitch(b) => twitch_builtin_object(Arc::clone(b)),
            ConnectedBundle::Youtube(b) => youtube_builtin_object(Arc::clone(b)),
            ConnectedBundle::Kick(b) => kick_builtin_object(Arc::clone(b)),
        };
        self.adopt_builtin(object, cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.display_name = self.status.display_name().to_owned();
        self.version = self.status.version().map(ToOwned::to_owned);
        self.endpoint = self.status.endpoint().map(ToOwned::to_owned);
        self.connection = self.status.connection();
        self.capability_flags = self.status.capability_flags();
        self.header_actions = self.status.header_actions();
        self.health_metrics = self.health.metrics();
        self.sections = self.content.sections();
        self.quick_actions = self.quick.actions();
        cx.notify();
    }

    fn apply_health_delta(&mut self, delta: HealthDelta, cx: &mut Context<Self>) {
        let idx = delta.index as usize;
        if idx < self.health_metrics.len() {
            self.health_metrics[idx].value = delta.new_value;
            self.sections = self.content.sections();
            cx.notify();
        }
    }

    fn spawn_eventsub_tally(bus: &Arc<EventBus>, cx: &mut Context<Self>) {
        let mut sub = bus.subscribe();
        cx.spawn(async move |this, cx| {
            while let async_bridge::EventBatch::Ready(batch) =
                async_bridge::recv_event_batch(&mut sub).await
            {
                let reauth = batch.iter().any(|e| {
                    e.kind == "platform.reauth_required" && e.source == EventSource::Twitch
                });
                let tails: Vec<String> = batch
                    .iter()
                    .filter_map(|e| e.kind.strip_prefix("twitch.").map(str::to_owned))
                    .collect();
                if tails.is_empty() && !reauth {
                    continue;
                }
                if this
                    .update(cx, |this, cx| {
                        if reauth {
                            this.twitch_reauth_required = true;
                        }
                        this.apply_eventsub_tally(tails, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn spawn_obs_connection_watch(bus: &Arc<EventBus>, cx: &mut Context<Self>) {
        let mut sub = bus.subscribe();
        cx.spawn(async move |this, cx| {
            while let async_bridge::EventBatch::Ready(batch) =
                async_bridge::recv_event_batch(&mut sub).await
            {
                let transitioned = batch.iter().any(|e| {
                    e.source == EventSource::Obs && e.kind.starts_with(OBS_CONNECTION_PREFIX)
                });
                if !transitioned {
                    continue;
                }
                if this.update(cx, |this, cx| this.reload(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn spawn_viewer_sampler(live_viewers: &LiveViewerAggregatorHandle, cx: &mut Context<Self>) {
        let mut stream = live_viewers.subscribe();
        cx.spawn(async move |this, cx| {
            while let Some(count) = stream.next().await {
                let n = match count {
                    LiveViewerCount::Reporting(n) => n,
                    LiveViewerCount::Empty => continue,
                };
                if this
                    .update(cx, |this, cx| this.push_viewer_sample(n, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn spawn_detail_ticker(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(DETAIL_TICK).await;
                let alive = this.update(cx, |this, cx| {
                    this.sections = this.content.sections();
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn on_qa_search(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if self.qa_search.on_changed(event) {
            cx.notify();
        }
    }

    fn apply_eventsub_tally(&mut self, tails: Vec<String>, cx: &mut Context<Self>) {
        for tail in tails {
            *self.eventsub_tally.entry(tail).or_insert(0) += 1;
        }
        cx.notify();
    }

    fn push_viewer_sample(&mut self, count: u64, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.viewer_samples.push_back((now, count));
        while self.viewer_samples.len() > VIEWER_RING_CAP {
            self.viewer_samples.pop_front();
        }
        let cutoff = now.checked_sub(VIEWER_DELTA_WINDOW + Duration::from_secs(60));
        if let Some(cutoff) = cutoff {
            while self
                .viewer_samples
                .front()
                .is_some_and(|(t, _)| *t < cutoff)
            {
                self.viewer_samples.pop_front();
            }
        }
        cx.notify();
    }

    fn viewer_delta(&self) -> Option<i64> {
        let (newest_t, newest) = self.viewer_samples.back().copied()?;
        let oldest = self.viewer_samples.front().copied()?;
        let target = newest_t.checked_sub(VIEWER_DELTA_WINDOW)?;
        let baseline = self
            .viewer_samples
            .iter()
            .rev()
            .find(|(t, _)| *t <= target)
            .copied()
            .unwrap_or(oldest);
        Some(newest as i64 - baseline.1 as i64)
    }

    pub(crate) fn navigate_to(&mut self, screen: Screen, cx: &mut Context<Self>) {
        cx.emit(NavRequested(screen));
    }

    fn on_header_action(
        &mut self,
        action: HeaderAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            HeaderAction::Disconnect => self.request_disconnect(cx),
            HeaderAction::Reconnect => self.dispatch_control(ControlVerb::Reconnect, cx),
            HeaderAction::RefreshToken => self.dispatch_control(ControlVerb::RefreshToken, cx),
            HeaderAction::Settings if self.is_obs => self.open_obs_settings(window, cx),
            HeaderAction::Settings => {
                cx.push_toast(ToastKind::Info, tr!("integration_settings_coming_soon"));
            }
        }
    }

    fn request_disconnect(&mut self, cx: &mut Context<Self>) {
        self.pending_disconnect.request(());
        cx.notify();
    }

    fn open_obs_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rt_handle = self.rt_handle.clone();
        let credentials = Arc::clone(&self.credentials);
        let settings = Arc::clone(&self.settings);
        let bus = Arc::clone(&self.bus);
        let seed = self.obs_install_seed.clone();
        let modal =
            cx.new(|cx| ObsSettingsModal::new(rt_handle, credentials, settings, bus, seed, cx));
        modal.update(cx, |m, cx| m.focus(window, cx));
        self._obs_modal_sub = Some(cx.subscribe(&modal, Self::on_obs_settings_event));
        self.obs_settings_modal = Some(modal);
        cx.notify();
    }

    fn on_obs_settings_event(
        &mut self,
        _modal: Entity<ObsSettingsModal>,
        event: &ObsSettingsModalEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ObsSettingsModalEvent::Close => self.close_obs_settings(cx),
            ObsSettingsModalEvent::Saved => {
                self.close_obs_settings(cx);
                cx.emit(ObsConnected);
            }
            ObsSettingsModalEvent::Disconnect => {
                self.close_obs_settings(cx);
                self.request_disconnect(cx);
            }
        }
    }

    fn close_obs_settings(&mut self, cx: &mut Context<Self>) {
        self.obs_settings_modal = None;
        self._obs_modal_sub = None;
        cx.notify();
    }

    fn dispatch_control(&mut self, verb: ControlVerb, cx: &mut Context<Self>) {
        let Some(ctrl) = self.control.clone() else {
            return;
        };
        if self.is_twitch && matches!(verb, ControlVerb::RefreshToken) {
            let task = self
                .rt_handle
                .spawn(async move { ctrl.refresh_token().await });
            cx.spawn(async move |this, cx| {
                let Ok(result) = task.await else {
                    return;
                };
                this.update(cx, |this, cx| match result {
                    Ok(()) => {
                        this.twitch_reauth_required = false;
                        cx.notify();
                    }
                    Err(ControlFailure::Unauthorized) => {
                        this.twitch_reauth_required = true;
                        cx.notify();
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "twitch token refresh failed");
                        cx.push_toast(ToastKind::Error, tr!("integration_control_failed"));
                    }
                })
                .ok();
            })
            .detach();
            return;
        }
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                match verb {
                    ControlVerb::Reconnect => ctrl.reconnect().await,
                    ControlVerb::Disconnect => ctrl.disconnect().await,
                    ControlVerb::RefreshToken => ctrl.refresh_token().await,
                }
            },
            move |this, result, cx| match result {
                Ok(()) => this.reload(cx),
                Err(err) => {
                    tracing::warn!(error = %err, "integration control failed");
                    cx.push_toast(ToastKind::Error, tr!("integration_control_failed"));
                    this.reload(cx);
                }
            },
            cx,
        );
    }

    fn cancel_disconnect(&mut self, cx: &mut Context<Self>) {
        self.pending_disconnect.cancel();
        cx.notify();
    }

    fn confirm_disconnect(&mut self, cx: &mut Context<Self>) {
        if self.pending_disconnect.take().is_some() {
            if self.is_obs {
                self.sign_out_obs(cx);
            } else if self.is_vtube {
                self.sign_out_vtube(cx);
            } else if let Some(platform) = platform_of(self.status.id().as_str()) {
                self.reset_to_connect(platform, cx);
            } else {
                self.dispatch_control(ControlVerb::Disconnect, cx);
            }
        }
        cx.notify();
    }

    pub(crate) fn on_quick_action(
        &mut self,
        idx: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(action) = self.quick_actions.get(idx) else {
            return;
        };
        if !action.enabled {
            return;
        }
        let action = action.clone();
        let obs_source = self.obs_source.clone();
        let rt_handle = self.rt_handle.clone();
        let modal = cx.new(|cx| QuickActionModal::new(action, obs_source, rt_handle, cx));
        modal.update(cx, |m, cx| m.focus(window, cx));
        self._qa_modal_sub = Some(cx.subscribe(&modal, Self::on_quick_action_modal_event));
        self.quick_action_modal = Some(modal);
        cx.notify();
    }

    fn on_quick_action_modal_event(
        &mut self,
        _modal: Entity<QuickActionModal>,
        event: &QuickActionModalEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            QuickActionModalEvent::Run { step, label } => {
                self.run_quick_action(step.clone(), label.clone(), cx);
                self.close_quick_action_modal(cx);
            }
            QuickActionModalEvent::Cancel => self.close_quick_action_modal(cx),
        }
    }

    fn run_quick_action(&mut self, step: SubActionStep, label: String, cx: &mut Context<Self>) {
        let builtin_id = self.status.id().as_str().to_owned();
        let engine = self.action_engine.clone();
        let toast_label = label.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                engine
                    .execute_quick_action(step, builtin_id, label, None)
                    .await
            },
            move |_detail, result, cx| match result {
                Ok(()) => cx.push_toast(
                    ToastKind::Success,
                    tr!("integration_quick_action_ran", label = toast_label),
                ),
                Err(err) => {
                    tracing::warn!(error = %err, "quick action failed");
                    cx.push_toast(ToastKind::Error, tr!("integration_quick_action_failed"));
                }
            },
            cx,
        );
    }

    fn close_quick_action_modal(&mut self, cx: &mut Context<Self>) {
        self.quick_action_modal = None;
        self._qa_modal_sub = None;
        cx.notify();
    }

    pub(crate) fn open_run_history(&mut self, cx: &mut Context<Self>) {
        let registry = Arc::clone(&self.trigger_registry);
        let modal = cx.new(|_| RunHistoryModal::new(self.display_name.clone(), registry));
        self._history_modal_sub = Some(cx.subscribe(&modal, Self::on_run_history_event));
        self.history_modal = Some(modal.clone());

        let history = Arc::clone(&self.history);
        let builtin_id = self.status.id().as_str().to_owned();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                history
                    .recent_for_builtin(&builtin_id, HISTORY_LIMIT)
                    .await
                    .map_err(|err| err.to_string())
            },
            move |_detail, result, cx| match result {
                Ok(runs) => modal.update(cx, |modal, cx| modal.set_runs(runs, cx)),
                Err(message) => {
                    tracing::warn!(error = %message, "quick action run history load failed");
                    cx.push_toast(ToastKind::Error, tr!("integration_run_history_failed"));
                }
            },
            cx,
        );
        cx.notify();
    }

    fn on_run_history_event(
        &mut self,
        _modal: Entity<RunHistoryModal>,
        _event: &RunHistoryDismissed,
        cx: &mut Context<Self>,
    ) {
        self.history_modal = None;
        self._history_modal_sub = None;
        cx.notify();
    }

    fn adopt_builtin(&mut self, object: BuiltinObject, cx: &mut Context<Self>) {
        self.builtins.install(object.clone());
        self.icon = object.icon;
        self.status = object.status;
        self.health = object.health;
        self.content = object.content;
        self.quick = object.quick;
        self.control = object.control;
        self.connect = None;
        self.eventsub_tally.clear();
        self.viewer_samples.clear();
        self.reload(cx);
    }

    fn reset_to_connect(&mut self, platform: PlatformId, cx: &mut Context<Self>) {
        self.builtins.remove(self.status.id());
        let credentials = Arc::clone(&self.credentials);
        let control = self.control.take();
        let key = credential_key(platform);
        self.rt_handle.spawn(async move {
            if let Some(ctrl) = control {
                let _ = ctrl.disconnect().await;
            }
            let _ = credentials
                .delete(&forge_storage::CredentialId::new(key))
                .await;
        });
        self.twitch_reauth_required = false;
        self.eventsub_tally.clear();
        self.viewer_samples.clear();
        self.open_connect_flow(platform, cx);
    }

    fn sign_out_obs(&mut self, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.credentials);
        let control = self.control.take();
        self.rt_handle.spawn(async move {
            if let Some(ctrl) = control {
                let _ = ctrl.disconnect().await;
            }
            let _ = forge_obs::credentials::clear(&*credentials).await;
        });
        self.obs_install_seed.clear();
        self.obs_source = None;
        cx.emit(ObsSignedOut);
        cx.notify();
    }

    fn sign_out_vtube(&mut self, cx: &mut Context<Self>) {
        let credentials = Arc::clone(&self.credentials);
        let control = self.control.take();
        self.rt_handle.spawn(async move {
            if let Some(ctrl) = control {
                let _ = ctrl.disconnect().await;
            }
            let _ = forge_vtube::credentials::clear(&*credentials).await;
        });
        self.vtube_install_seed.clear();
        cx.emit(VTubeSignedOut);
        cx.notify();
    }

    fn resync_content(&mut self, cx: &mut Context<Self>) {
        if let Some(obs) = &self.obs_source {
            obs.request_catalog_resync();
        }
        self.reload(cx);
    }

    fn augmented_sections(&self) -> Vec<DetailSection> {
        let mut sections = self.sections.clone();
        for section in &mut sections {
            if let DetailSection::TwoColumn { right, .. } = section {
                Self::fill_subscription_counts(right, &self.eventsub_tally);
            } else {
                Self::fill_subscription_counts(section, &self.eventsub_tally);
            }
        }
        sections
    }

    fn fill_subscription_counts(section: &mut DetailSection, tally: &HashMap<String, u64>) {
        if let DetailSection::SubscriptionList { items, .. } = section {
            for row in items.iter_mut() {
                if row.error_label.is_none() {
                    row.event_count = Some(tally.get(&row.name).copied().unwrap_or(0));
                }
            }
        }
    }

    fn augmented_health(&self) -> [HealthMetric; 4] {
        let mut metrics = self.health_metrics.clone();
        if let Some(metric) = metrics.iter_mut().find(|m| m.label == "Viewers")
            && let HealthValue::Text { secondary, .. } = &mut metric.value
        {
            let delta = self.viewer_delta().unwrap_or(0);
            let sign = if delta >= 0 { "+" } else { "" };
            *secondary = Some(tr!(
                "integration_viewers_delta",
                delta = format!("{sign}{delta}"),
                window = "15m"
            ));
        }
        metrics
    }

    fn hero_badges(&self, palette: &ForgePalette) -> Vec<AnyElement> {
        self.status
            .name_badges()
            .into_iter()
            .map(|b| hero_badge_elem(&b, palette).into_any_element())
            .collect()
    }

    fn token_countdown(&self) -> Option<String> {
        let expiry = self.status.token_expiry()?;
        let remaining = expiry.duration_since(std::time::SystemTime::now()).ok()?;
        let total = remaining.as_secs();
        if total == 0 {
            return None;
        }
        let hours = total / 3600;
        let minutes = (total % 3600) / 60;
        Some(tr!(
            "integration_token_expires_in",
            time = format!("{hours}h {minutes}m")
        ))
    }

    fn header_right_connected(&self, palette: &ForgePalette, _density: Density) -> AnyElement {
        let mut row = div().flex().items_center().gap(px(8.0)).child(
            div()
                .flex()
                .items_center()
                .gap(px(5.0))
                .child(status_dot(palette.success, px(7.0)))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.success)
                        .child(tr!("integration_status_authenticated")),
                ),
        );
        if let Some(suffix) = self.token_countdown() {
            row = row
                .child(
                    div()
                        .text_size(FONT_XS)
                        .text_color(palette.text_faint)
                        .child("\u{00b7}"),
                )
                .child(
                    div()
                        .font_family(mono_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_muted)
                        .child(suffix),
                );
        }
        row.into_any_element()
    }

    fn header_right_connection(&self, palette: &ForgePalette) -> AnyElement {
        let (dot, text, label) = match self.connection {
            ConnectionState::Connected => (
                palette.success,
                palette.success,
                tr!("common_status_connected"),
            ),
            ConnectionState::Connecting => (
                palette.info,
                palette.info,
                tr!("integration_state_connecting_title"),
            ),
            ConnectionState::Reconnecting => (
                palette.warning,
                palette.warning,
                tr!("integration_state_reconnecting_title"),
            ),
            ConnectionState::Disconnected => (
                palette.text_faint,
                palette.text_muted,
                tr!("common_status_not_connected"),
            ),
        };
        div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .child(status_dot(dot, px(7.0)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(text)
                    .child(label),
            )
            .into_any_element()
    }

    fn is_oauth_platform(&self) -> bool {
        matches!(self.status.id().as_str(), "twitch" | "youtube" | "kick")
    }

    fn hero_subtitle(&self) -> String {
        if self.is_oauth_platform()
            && let Some(prefix) = self.status.endpoint()
        {
            return prefix.to_owned();
        }
        self.endpoint.clone().unwrap_or_default()
    }

    fn header_card(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (letter, brand) = hero_identity(self.icon.as_str(), &self.display_name, palette);
        let hero_name = self
            .status
            .hero_name()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| self.display_name.clone());
        let sub = self.hero_subtitle();
        let name_badges = self.hero_badges(palette);

        let mut right = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density));
        if let Some(version) = &self.version {
            right = right.child(pill(version.clone(), palette.text_muted, palette));
        }
        if self.capability_flags.limited {
            let label = self
                .capability_flags
                .label
                .clone()
                .unwrap_or_else(|| tr!("widget_header_capability_limited"));
            right = right.child(pill(label.to_uppercase(), palette.warning, palette));
        }
        for (i, action) in self.header_actions.iter().enumerate() {
            right = right.child(self.action_button(i, action.clone(), palette, density, cx));
        }

        platform_hero(letter, brand, hero_name, sub, palette)
            .density(density)
            .name_badges(name_badges)
            .right(right)
            .into_any_element()
    }

    fn action_button(
        &self,
        idx: usize,
        action: HeaderAction,
        palette: &ForgePalette,
        _density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = header_action_label(&action);
        let text_color = match action {
            HeaderAction::Disconnect => palette.random,
            _ => palette.text_secondary,
        };
        let glyph = match action {
            HeaderAction::Reconnect | HeaderAction::RefreshToken => Icon::Refresh,
            HeaderAction::Disconnect => Icon::Logout,
            HeaderAction::Settings => Icon::Settings,
        };
        let hover_border = palette.border_input;
        div()
            .id(("header-action", idx))
            .flex()
            .items_center()
            .gap(px(5.0))
            .py(px(5.0))
            .px(px(11.0))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.on_header_action(action.clone(), window, cx)
            }))
            .child(icon(glyph, FONT_XS, text_color))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(text_color)
                    .child(label),
            )
            .into_any_element()
    }

    fn disconnect_overlay(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let card = confirm_modal(
            tr!("integration_disconnect_title"),
            tr!("builtin_disconnect_confirm_hint"),
            ConfirmTone::Warning,
            palette,
        )
        .item_name(self.display_name.clone())
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "integration-disconnect-cancel",
            tr!("widget_confirm_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_disconnect(cx)),
        )
        .on_confirm(
            "integration-disconnect-confirm",
            tr!("widget_header_action_disconnect"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_disconnect(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("integration-disconnect-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_disconnect(cx));
            })
    }

    fn twitch_reauth_banner(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cta = div()
            .id("twitch-reauth")
            .flex_none()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.warning)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.warning, 0.85)))
            .on_click(cx.listener(|this, _, _, cx| this.reset_to_connect(PlatformId::Twitch, cx)))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child(tr!("twitch_reauth_btn")),
            );
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.warning)
            .bg(palette.shell)
            .child(icon(Icon::AlertTriangle, px(14.0), palette.warning))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(tr!("twitch_reauth_title")),
                    )
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("twitch_reauth_detail")),
                    ),
            )
            .child(cta)
            .into_any_element()
    }

    fn state_banner(&self, palette: &ForgePalette, density: Density) -> Option<AnyElement> {
        let (accent, glyph, title, detail): (Rgba, Icon, String, String) = match self.connection {
            ConnectionState::Connected => return None,
            ConnectionState::Connecting => (
                palette.info,
                Icon::Refresh,
                tr!("integration_state_connecting_title"),
                tr!("integration_state_connecting_detail"),
            ),
            ConnectionState::Reconnecting => (
                palette.warning,
                Icon::Refresh,
                tr!("integration_state_reconnecting_title"),
                tr!("integration_state_reconnecting_detail"),
            ),
            ConnectionState::Disconnected => (
                palette.text_muted,
                Icon::PlugConnected,
                tr!("common_status_not_connected"),
                tr!("integration_state_disconnected_detail"),
            ),
        };

        let text_col = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(title),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(detail),
            );

        Some(
            div()
                .w_full()
                .flex()
                .items_start()
                .gap(spacing(Spacing::Sm, density))
                .py(spacing(Spacing::Sm, density))
                .px(spacing(Spacing::Md, density))
                .rounded(radius(Radius::Md))
                .border(BORDER_THIN)
                .border_color(accent)
                .bg(palette.elevated)
                .child(icon(glyph, FONT_SM, accent))
                .child(text_col)
                .into_any_element(),
        )
    }
}

impl Render for IntegrationDetail {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let body = match &self.connect {
            Some(connect) => connect.view.clone().into_any_element(),
            None => {
                let header_card = self.header_card(&palette, density, cx);
                let reconnecting = matches!(
                    self.connection,
                    ConnectionState::Connecting | ConnectionState::Reconnecting
                );
                let state_banner = self.state_banner(&palette, density);
                let reauth_banner = (self.is_twitch && self.twitch_reauth_required)
                    .then(|| self.twitch_reauth_banner(&palette, density, cx));
                let health = health_grid(&self.augmented_health(), reconnecting, &palette, density);
                let on_refresh: SectionRefresh =
                    Rc::new(cx.listener(|this, _: &ClickEvent, _, cx| this.resync_content(cx)));
                let content = content_sections(
                    &self.augmented_sections(),
                    Some(&on_refresh),
                    &palette,
                    density,
                );
                let quick = self.quick_actions_card(&palette, density, cx);

                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .children(reauth_banner)
                    .children(state_banner)
                    .child(header_card)
                    .child(health)
                    .child(content)
                    .child(quick)
                    .into_any_element()
            }
        };

        let scroll = div()
            .id("integration-detail-scroll")
            .flex_1()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(div().w_full().py(px(18.0)).px(px(22.0)).child(body));

        let disconnect_overlay = self
            .pending_disconnect
            .is_pending()
            .then(|| self.disconnect_overlay(&palette, cx));

        let ancestor_crumb = match self.status.id().as_str() {
            "twitch" | "youtube" | "kick" => BreadcrumbCrumb::link(
                tr!("platforms_breadcrumb"),
                "integration-crumb-ancestor",
                cx.listener(|this, _: &ClickEvent, _, cx| this.navigate_to(Screen::Platforms, cx)),
            ),
            "obs" | "vtube" => BreadcrumbCrumb::link(
                tr!("stream_apps_breadcrumb"),
                "integration-crumb-ancestor",
                cx.listener(|this, _: &ClickEvent, _, cx| this.navigate_to(Screen::StreamApps, cx)),
            ),
            _ => BreadcrumbCrumb::leaf(tr!("server_breadcrumb_builtin")),
        };

        let is_oauth_platform = self.is_oauth_platform();
        let header_right = match &self.connect {
            Some(connect) => Some(connect.view.read(cx).status_indicator(&palette, density)),
            None if is_oauth_platform && self.connection == ConnectionState::Connected => {
                Some(self.header_right_connected(&palette, density))
            }
            None if is_oauth_platform => None,
            None => Some(self.header_right_connection(&palette)),
        };
        let mut frame = page_frame(
            vec![
                ancestor_crumb,
                BreadcrumbCrumb::leaf(self.display_name.clone()),
            ],
            &palette,
        );
        if let Some(status) = header_right {
            frame = frame.header_right(status);
        }
        let frame = frame.body(scroll);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(disconnect_overlay)
            .children(self.quick_action_modal.clone())
            .children(self.obs_settings_modal.clone())
            .children(self.history_modal.clone())
    }
}

fn hero_identity(icon_str: &str, display_name: &str, palette: &ForgePalette) -> (String, Rgba) {
    match icon_str {
        "brand-twitch" => ("T".to_owned(), palette.brand),
        "brand-youtube" => ("Y".to_owned(), palette.random),
        "brand-kick" => ("K".to_owned(), palette.info),
        "broadcast" => ("O".to_owned(), palette.success),
        "mood-smile" => ("V".to_owned(), palette.warning),
        "brand-discord" => ("D".to_owned(), palette.brand),
        "piano" => ("M".to_owned(), palette.accent_teal),
        "keyboard" => ("H".to_owned(), palette.info),
        _ => {
            let initial = display_name
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase().to_string())
                .unwrap_or_else(|| "?".to_owned());
            (initial, palette.brand)
        }
    }
}

enum ControlVerb {
    Reconnect,
    Disconnect,
    RefreshToken,
}

fn header_action_label(action: &HeaderAction) -> String {
    match action {
        HeaderAction::Reconnect => tr!("widget_header_action_reconnect"),
        HeaderAction::RefreshToken => tr!("widget_header_action_refresh_token"),
        HeaderAction::Disconnect => tr!("widget_header_action_disconnect"),
        HeaderAction::Settings => tr!("widget_header_action_settings"),
    }
}

fn pill(label: String, text_color: Rgba, palette: &ForgePalette) -> impl IntoElement {
    badge(palette.surface_overlay, text_color, label, true, FONT_XS)
        .weight(FontWeight::NORMAL)
        .padding_xy(px(0.0), px(6.0))
        .radius(radius(Radius::Md))
        .flex_none()
}

fn hero_badge_elem(badge_spec: &HeroBadge, palette: &ForgePalette) -> impl IntoElement + use<> {
    let text_color = match badge_spec.tone {
        HeroBadgeTone::Neutral => palette.text_muted,
        HeroBadgeTone::Positive => palette.success,
    };
    badge(
        palette.surface_overlay,
        text_color,
        badge_spec.label.clone(),
        badge_spec.monospace,
        FONT_XS,
    )
    .weight(FontWeight::NORMAL)
    .padding_xy(px(2.0), px(6.0))
    .radius(radius(Radius::Sm))
    .flex_none()
}

fn platform_of(id: &str) -> Option<PlatformId> {
    match id {
        "twitch" => Some(PlatformId::Twitch),
        "youtube" => Some(PlatformId::YouTube),
        "kick" => Some(PlatformId::Kick),
        _ => None,
    }
}

fn credential_key(platform: PlatformId) -> &'static str {
    match platform {
        PlatformId::Twitch => forge_platform_twitch::TWITCH_CREDENTIAL_ID,
        PlatformId::YouTube => forge_platform_youtube::CREDENTIAL_KEY,
        PlatformId::Kick => forge_platform_kick::CREDENTIAL_KEY,
    }
}

fn connect_platform_for(id: &str, has_control: bool) -> Option<PlatformId> {
    if has_control {
        return None;
    }
    platform_of(id)
}
