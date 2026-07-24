use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, Confirm, ConfirmTone, Density, FONT_SM, FONT_XS, ForgePalette,
    Icon, InputEvent, OverlayPosition, Picker, PickerEvent, PickerItem, PickerLabels, Radius,
    SearchState, Spacing, TextInput, ToastKind, badge, body_family, confirm_modal, icon,
    mono_family, overlay, page_frame, platform_hero, radius, spacing, status_dot, tr,
};
use forge_events::{EventPublisher, EventSource};
use forge_obs::{ObsClient, ObsSource};
use forge_platform_core::{
    BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinStatus, CapabilityFlags, ConnectionState,
    ControlFailure, DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthValue, HeroBadge,
    HeroBadgeTone, PickerKind, QuickAction, QuickActions, SectionIcon,
};
use forge_platform_twitch::TwitchIntegrationBundle;
use forge_runtime::{ActionEngineHandle, EventBus, LiveViewerAggregatorHandle, LiveViewerCount};
use forge_storage::CredentialsRepo;
use forge_types::{PlatformId, Variant};
use futures_util::StreamExt as _;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, FontWeight, Rgba, Subscription, Window,
    div, prelude::*, px,
};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::async_bridge::{self, ErrorSink};
use crate::builtin_sections::{content_sections, health_grid};
use crate::oauth_connect::{KickFlowHandle, LocalCallbackFlowPhase, YoutubeFlowHandle};
use crate::platforms::PlatformConnectivity;
use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;
use crate::toasts::PushToast;
use crate::twitch_panel::{TwitchDeviceState, TwitchFlowHandle};

pub struct IntegrationDetail {
    status: Arc<dyn BuiltinStatus>,
    health: Arc<dyn BuiltinHealth>,
    content: Arc<dyn BuiltinContent>,
    quick: Arc<dyn QuickActions>,
    control: Option<Arc<dyn BuiltinControl>>,
    pub(crate) rt_handle: tokio::runtime::Handle,
    action_engine: ActionEngineHandle,
    obs_source: Option<Arc<ObsClient>>,
    pub(crate) credentials: Arc<dyn CredentialsRepo>,
    pub(crate) bus: Arc<dyn EventPublisher>,
    pub(crate) live_viewers: LiveViewerAggregatorHandle,
    pub(crate) connect_platform: Option<PlatformId>,
    pub(crate) flow_phase: LocalCallbackFlowPhase,
    pub(crate) flow_auth_url: Option<String>,
    pub(crate) flow_error: Option<String>,
    pub(crate) youtube_flow: Option<YoutubeFlowHandle>,
    pub(crate) kick_flow: Option<KickFlowHandle>,
    is_twitch: bool,
    twitch_reauth_required: bool,
    pub(crate) twitch_flow: Option<TwitchFlowHandle>,
    pub(crate) twitch_device: Option<TwitchDeviceState>,
    icon: SectionIcon,
    pub(crate) display_name: String,
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
    pending_picker: Option<PendingPicker>,
    _conn_obs: Subscription,
    _qa_search_sub: Subscription,
}

const VIEWER_DELTA_WINDOW: Duration = Duration::from_secs(15 * 60);
/// Below this the ring has too little history to show an honest delta.
const VIEWER_DELTA_MIN_HISTORY: Duration = Duration::from_secs(14 * 60);
const VIEWER_RING_CAP: usize = 256;
const DETAIL_TICK: Duration = Duration::from_secs(30);

impl EventEmitter<NavRequested> for IntegrationDetail {}

impl Drop for IntegrationDetail {
    fn drop(&mut self) {
        if let Some(dev) = &self.twitch_device {
            dev.cancel.cancel();
        }
    }
}

impl IntegrationDetail {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        icon: SectionIcon,
        status: Arc<dyn BuiltinStatus>,
        health: Arc<dyn BuiltinHealth>,
        content: Arc<dyn BuiltinContent>,
        quick: Arc<dyn QuickActions>,
        control: Option<Arc<dyn BuiltinControl>>,
        obs_source: Option<Arc<ObsClient>>,
        rt_handle: tokio::runtime::Handle,
        action_engine: ActionEngineHandle,
        credentials: Arc<dyn CredentialsRepo>,
        bus: Arc<dyn EventPublisher>,
        event_bus: Arc<EventBus>,
        live_viewers: LiveViewerAggregatorHandle,
        connectivity: Entity<PlatformConnectivity>,
        cx: &mut Context<Self>,
    ) -> Self {
        let conn_obs = cx.observe(&connectivity, |this, _, cx| this.reload(cx));

        let is_twitch = status.id().as_str() == "twitch";
        let palette = cx.palette();
        let qa_search = SearchState::new(cx, palette, tr!("integration_qa_filter_placeholder"));
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

        if connect_platform == Some(PlatformId::Twitch) {
            cx.spawn(async move |this, cx| {
                let _ = this.update(cx, |this, cx| this.begin_twitch_device(cx));
            })
            .detach();
        }

        if is_twitch {
            Self::spawn_eventsub_tally(&event_bus, cx);
            Self::spawn_viewer_sampler(&live_viewers, cx);
            Self::spawn_detail_ticker(cx);
        }

        Self {
            status,
            health,
            content,
            quick,
            control,
            rt_handle,
            action_engine,
            obs_source,
            credentials,
            bus,
            live_viewers,
            connect_platform,
            flow_phase: LocalCallbackFlowPhase::Idle,
            flow_auth_url: None,
            flow_error: None,
            youtube_flow: None,
            kick_flow: None,
            is_twitch,
            twitch_reauth_required: false,
            twitch_flow: None,
            twitch_device: None,
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
            pending_picker: None,
            _conn_obs: conn_obs,
            _qa_search_sub: qa_search_sub,
        }
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
                if this.update(cx, |_, cx| cx.notify()).is_err() {
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

    /// `None` until the ring holds at least `VIEWER_DELTA_MIN_HISTORY` of samples.
    fn viewer_delta(&self) -> Option<i64> {
        let (newest_t, newest) = self.viewer_samples.back().copied()?;
        let oldest = self.viewer_samples.front().copied()?;
        if newest_t.saturating_duration_since(oldest.0) < VIEWER_DELTA_MIN_HISTORY {
            return None;
        }
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

    fn on_header_action(&mut self, action: HeaderAction, cx: &mut Context<Self>) {
        match action {
            HeaderAction::Disconnect => {
                self.pending_disconnect.request(());
                cx.notify();
            }
            HeaderAction::Reconnect => self.dispatch_control(ControlVerb::Reconnect, cx),
            HeaderAction::RefreshToken => self.dispatch_control(ControlVerb::RefreshToken, cx),
            HeaderAction::Settings => {
                cx.push_toast(ToastKind::Info, tr!("integration_settings_coming_soon"));
            }
        }
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
        async_bridge::report_failure(
            &self.rt_handle,
            async move {
                match verb {
                    ControlVerb::Reconnect => ctrl.reconnect().await,
                    ControlVerb::Disconnect => ctrl.disconnect().await,
                    ControlVerb::RefreshToken => ctrl.refresh_token().await,
                }
            },
            ErrorSink::Toast,
            tr!("integration_control_failed"),
            cx,
        );
    }

    fn cancel_disconnect(&mut self, cx: &mut Context<Self>) {
        self.pending_disconnect.cancel();
        cx.notify();
    }

    fn confirm_disconnect(&mut self, cx: &mut Context<Self>) {
        if self.pending_disconnect.take().is_some() {
            if self.is_twitch {
                self.reset_twitch_to_connect(cx);
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
        if let Some(kind) = action.picker {
            self.open_picker(idx, kind, window, cx);
            return;
        }
        let step = action.subaction_template.clone();
        let builtin_id = self.status.id().as_str().to_owned();
        let label = action.label.clone();
        let engine = self.action_engine.clone();
        async_bridge::report_failure(
            &self.rt_handle,
            async move {
                engine
                    .execute_quick_action(step, builtin_id, label, None)
                    .await
            },
            ErrorSink::Toast,
            tr!("integration_quick_action_failed"),
            cx,
        );
    }

    fn open_picker(
        &mut self,
        action_index: usize,
        kind: PickerKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let palette = cx.palette();
        let picker = cx.new(|cx| {
            let mut picker = Picker::new(picker_labels(kind), Vec::new(), palette, cx);
            picker.set_loading(true, cx);
            picker
        });
        let sub = cx.subscribe(&picker, Self::on_picker_event);
        picker.update(cx, |f, cx| f.focus(window, cx));

        match self.obs_source.clone() {
            Some(client) => self.spawn_picker_fetch(picker.clone(), client, kind, cx),
            None => picker.update(cx, |picker, cx| picker.set_loading(false, cx)),
        }

        self.pending_picker = Some(PendingPicker {
            picker,
            action_index,
            kind,
            current_scene: None,
            _sub: sub,
        });
        cx.notify();
    }

    fn spawn_picker_fetch(
        &self,
        picker: Entity<Picker>,
        client: Arc<ObsClient>,
        kind: PickerKind,
        cx: &mut Context<Self>,
    ) {
        async_bridge::run_async(
            &self.rt_handle,
            fetch_picker_items(client, kind),
            move |detail, result, cx| detail.apply_picker_items(&picker, result, cx),
            cx,
        );
    }

    fn apply_picker_items(
        &mut self,
        picker: &Entity<Picker>,
        result: Result<(Vec<PickerItem>, Option<String>), String>,
        cx: &mut Context<Self>,
    ) {
        if self
            .pending_picker
            .as_ref()
            .is_none_or(|pending| pending.picker != *picker)
        {
            return;
        }
        match result {
            Ok((items, current_scene)) => {
                if let Some(pending) = self.pending_picker.as_mut() {
                    pending.current_scene = current_scene;
                }
                picker.update(cx, |picker, cx| {
                    picker.set_items(items, cx);
                    picker.set_loading(false, cx);
                });
            }
            Err(reason) => {
                ErrorSink::Toast.report(reason, cx);
                picker.update(cx, |picker, cx| picker.set_loading(false, cx));
            }
        }
        cx.notify();
    }

    fn on_picker_event(
        &mut self,
        _picker: Entity<Picker>,
        event: &PickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PickerEvent::Selected(id) => self.pick_target(id.to_string(), cx),
            PickerEvent::Cancelled => self.cancel_picker(cx),
        }
    }

    fn cancel_picker(&mut self, cx: &mut Context<Self>) {
        self.pending_picker = None;
        cx.notify();
    }

    fn pick_target(&mut self, selected_id: String, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_picker.take() else {
            return;
        };
        let Some(action) = self.quick_actions.get(pending.action_index) else {
            cx.notify();
            return;
        };
        let mut step = action.subaction_template.clone();
        let label = action.label.clone();
        let builtin_id = self.status.id().as_str().to_owned();

        match pending.kind {
            PickerKind::Scene => {
                step.config
                    .insert("scene".to_owned(), Variant::String(selected_id));
            }
            PickerKind::Source => {
                if let Some(scene) = pending.current_scene {
                    step.config
                        .insert("scene".to_owned(), Variant::String(scene));
                }
                step.config
                    .insert("source".to_owned(), Variant::String(selected_id));
            }
            PickerKind::Input => {
                step.config
                    .insert("source".to_owned(), Variant::String(selected_id));
            }
            PickerKind::Hotkey | PickerKind::Expression | PickerKind::MidiPort => {
                cx.notify();
                return;
            }
        }

        let engine = self.action_engine.clone();
        async_bridge::report_failure(
            &self.rt_handle,
            async move {
                engine
                    .execute_quick_action(step, builtin_id, label, None)
                    .await
            },
            ErrorSink::Toast,
            tr!("integration_quick_action_failed"),
            cx,
        );
        cx.notify();
    }

    pub(crate) fn open_url(&self, url: String, cx: &mut Context<Self>) {
        async_bridge::open_external(
            &self.rt_handle,
            url,
            ErrorSink::Toast,
            tr!("integration_open_url_failed"),
            cx,
        );
    }

    pub(crate) fn install_twitch_bundle(
        &mut self,
        bundle: Arc<TwitchIntegrationBundle>,
        cx: &mut Context<Self>,
    ) {
        self.status = bundle.clone();
        self.health = bundle.clone();
        self.content = bundle.clone();
        self.quick = bundle.clone();
        self.control = Some(bundle as Arc<dyn BuiltinControl>);
        self.connect_platform = None;
        self.eventsub_tally.clear();
        self.viewer_samples.clear();
        self.flow_phase = LocalCallbackFlowPhase::Idle;
        self.flow_auth_url = None;
        self.flow_error = None;
        self.twitch_flow = None;
        self.reload(cx);
    }

    pub(crate) fn reset_twitch_to_connect(&mut self, cx: &mut Context<Self>) {
        if let Some(ctrl) = self.control.take() {
            self.rt_handle.spawn(async move {
                let _ = ctrl.disconnect().await;
            });
        }
        self.connect_platform = Some(PlatformId::Twitch);
        self.flow_phase = LocalCallbackFlowPhase::Idle;
        self.flow_auth_url = None;
        self.flow_error = None;
        self.twitch_reauth_required = false;
        let credentials = Arc::clone(&self.credentials);
        self.rt_handle.spawn(async move {
            let id = forge_storage::CredentialId::new("twitch:broadcaster");
            let _ = credentials.delete(&id).await;
        });
        self.begin_twitch_device(cx);
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
        if let Some(delta) = self.viewer_delta()
            && let Some(metric) = metrics.iter_mut().find(|m| m.label == "Viewers")
            && let HealthValue::Text { secondary, .. } = &mut metric.value
        {
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
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.on_header_action(action.clone(), cx)
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

        let body = match self.connect_platform {
            Some(platform) => self.oauth_screen(platform, &palette, density, cx),
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
                let content = content_sections(&self.augmented_sections(), &palette, density);
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
        let picker_overlay = self.pending_picker.as_ref().map(|pending| {
            let view = cx.entity();
            overlay(pending.picker.clone(), &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("integration-picker-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_picker(cx));
                })
                .into_any_element()
        });

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
        let header_right = match self.connect_platform {
            Some(PlatformId::Twitch) => Some(self.twitch_device_status(&palette, density)),
            Some(platform) => Some(self.connect_status(platform, &palette, density)),
            None if is_oauth_platform && self.connection == ConnectionState::Connected => {
                Some(self.header_right_connected(&palette, density))
            }
            None => None,
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
            .children(picker_overlay)
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

struct PendingPicker {
    picker: Entity<Picker>,
    action_index: usize,
    kind: PickerKind,
    current_scene: Option<String>,
    _sub: Subscription,
}

fn picker_labels(kind: PickerKind) -> PickerLabels {
    let title = match kind {
        PickerKind::Scene => tr!("builtin_picker_scene"),
        PickerKind::Source => tr!("builtin_picker_source"),
        PickerKind::Input => tr!("builtin_picker_audio_input"),
        PickerKind::Hotkey => tr!("builtin_picker_hotkey"),
        PickerKind::Expression => tr!("builtin_picker_expression"),
        PickerKind::MidiPort => tr!("builtin_picker_midi_port"),
    };
    PickerLabels {
        title: title.into(),
        placeholder: tr!("widget_picker_search_placeholder").into(),
        empty: tr!("widget_picker_no_results").into(),
        loading: tr!("widget_picker_loading").into(),
        cancel: tr!("common_cancel").into(),
    }
}

async fn fetch_picker_items(
    client: Arc<ObsClient>,
    kind: PickerKind,
) -> Result<(Vec<PickerItem>, Option<String>), String> {
    match kind {
        PickerKind::Scene => {
            let scenes = client.scenes().await.map_err(|e| e.to_string())?;
            let items = scenes
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: Icon::from_name("layout"),
                })
                .collect();
            Ok((items, None))
        }
        PickerKind::Source => {
            let scene = client
                .current_scene()
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "no active scene".to_owned())?;
            let sources = client.sources(&scene).await.map_err(|e| e.to_string())?;
            let items = sources
                .into_iter()
                .map(|source| PickerItem {
                    id: source.name.clone().into(),
                    label: source.name.into(),
                    sublabel: Some(if source.visible { "visible" } else { "hidden" }.into()),
                    icon: Icon::from_name("device-desktop"),
                })
                .collect();
            Ok((items, Some(scene)))
        }
        PickerKind::Input => {
            let inputs = client.audio_inputs().await.map_err(|e| e.to_string())?;
            let items = inputs
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: Icon::from_name("volume"),
                })
                .collect();
            Ok((items, None))
        }
        PickerKind::Hotkey | PickerKind::Expression | PickerKind::MidiPort => {
            Err("unsupported picker kind".to_owned())
        }
    }
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

fn connect_platform_for(id: &str, has_control: bool) -> Option<PlatformId> {
    if has_control {
        return None;
    }
    match id {
        "twitch" => Some(PlatformId::Twitch),
        "youtube" => Some(PlatformId::YouTube),
        "kick" => Some(PlatformId::Kick),
        _ => None,
    }
}
