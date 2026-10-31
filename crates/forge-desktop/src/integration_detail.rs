mod attachments;
mod banners;
mod bridges;
mod connect_lifecycle;
mod controls;
mod header;
mod quick_actions;
mod section_data;
#[cfg(test)]
mod tests;

use self::quick_actions::reachable_quick_actions;
use crate::builtin_sections::{
    SectionHooks, SectionRefresh, SectionStepRun, StepClick, content_sections, health_grid,
};
use crate::collection_manager::CollectionManager;
use crate::connect_flow::ConnectFlow;
use crate::donation_settings::{DonationSettingsView, remove_confirm};
use crate::hub_crumb::hub_crumb;
use crate::in_flight_steps::InFlightSteps;
use crate::integration_catalog::declaration_of;
use crate::integration_quick_action_modal::QuickActionModal;
use crate::integrations::{BuiltinObject, ObsInstallSeed, VTubeInstallSeed};
use crate::navigation::NavRequested;
use crate::obs_credentials_form::ObsConnected;
use crate::obs_settings_modal::ObsSettingsModal;
use crate::platform_credentials::PlatformCredentialManagers;
use crate::platforms::PlatformConnectivity;
use crate::run_history_modal::RunHistoryModal;
use crate::twitch_bot_section::TwitchBotSection;
use forge_components::{
    ActivePresentation, BreadcrumbCrumb, Confirm, SearchState, page_frame, scaled, tr,
};
use forge_events::EventPublisher;
use forge_obs::ObsClient;
use forge_platform_core::{
    BuiltinCollections, BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinOptionSource,
    BuiltinStatus, CapabilityFlags, ConnectionState, DetailSection, HeaderAction, HealthMetric,
    QuickAction, QuickActions, SectionIcon,
};
use forge_registry::TriggerRegistry;
use forge_runtime::{ActionEngineHandle, EventBus, IntegrationSlot, LiveViewerAggregatorHandle};
use forge_storage::{CredentialsRepo, HistoryRepo, SettingsRepo};
use forge_types::PlatformId;
use gpui::{
    ClickEvent, Context, Entity, EventEmitter, Subscription, Task, Window, div, prelude::*, px,
};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

struct ActiveConnect {
    view: Entity<ConnectFlow>,
    _subs: Vec<Subscription>,
}

struct DonationSettingsLink {
    view: Entity<DonationSettingsView>,
    _repaint: Subscription,
}

pub struct IntegrationDetail {
    status: Arc<dyn BuiltinStatus>,
    health: Arc<dyn BuiltinHealth>,
    content: Arc<dyn BuiltinContent>,
    quick: Arc<dyn QuickActions>,
    control: Option<Arc<dyn BuiltinControl>>,
    collections: Option<Arc<dyn BuiltinCollections>>,
    option_source: Option<Arc<dyn BuiltinOptionSource>>,
    rt_handle: tokio::runtime::Handle,
    action_engine: ActionEngineHandle,
    obs_source: Option<Arc<ObsClient>>,
    credentials: Arc<dyn CredentialsRepo>,
    settings: Arc<dyn SettingsRepo>,
    history: Arc<dyn HistoryRepo>,
    trigger_registry: Arc<TriggerRegistry>,
    bus: Arc<dyn EventPublisher>,
    twitch_slot: Option<IntegrationSlot<BuiltinObject>>,
    kick_slot: Option<IntegrationSlot<BuiltinObject>>,
    youtube_slot: Option<IntegrationSlot<BuiltinObject>>,
    obs_install_seed: ObsInstallSeed,
    vtube_install_seed: VTubeInstallSeed,
    connect: Option<ActiveConnect>,
    donation_settings: Option<DonationSettingsLink>,
    twitch_bot: Option<Entity<TwitchBotSection>>,
    twitch_sign_in: Option<forge_platform_twitch::TwitchSignIn>,
    youtube_sign_in: Option<forge_platform_youtube::YoutubeSignIn>,
    kick_sign_in: Option<forge_platform_kick::KickSignIn>,
    platform_credentials: PlatformCredentialManagers,
    _twitch_bot_subs: Vec<Subscription>,
    is_twitch: bool,
    is_obs: bool,
    is_vtube: bool,
    twitch_reauth_required: bool,
    obs_password_rejected: bool,
    icon: SectionIcon,
    display_name: String,
    version: Option<String>,
    connection: ConnectionState,
    capability_flags: CapabilityFlags,
    header_actions: Vec<HeaderAction>,
    health_metrics: [HealthMetric; 4],
    sections: Vec<DetailSection>,
    pub(crate) quick_actions: Vec<QuickAction>,
    pub(crate) qa_search: SearchState,
    eventsub_tally: HashMap<String, u64>,
    row_steps_in_flight: InFlightSteps,
    viewer_samples: VecDeque<(Instant, u64)>,
    pending_disconnect: Confirm<()>,
    signing_out: bool,
    quick_action_modal: Option<Entity<QuickActionModal>>,
    collection_manager: Option<Entity<CollectionManager>>,
    obs_settings_modal: Option<Entity<ObsSettingsModal>>,
    history_modal: Option<Entity<RunHistoryModal>>,
    _qa_modal_sub: Option<Subscription>,
    _collection_manager_sub: Option<Subscription>,
    _obs_modal_sub: Option<Subscription>,
    _history_modal_subs: Vec<Subscription>,
    _conn_obs: Subscription,
    _qa_search_sub: Subscription,
    _health_bridge: Task<()>,
    _quick_action_bridge: Option<Task<()>>,
    _bridges: Vec<Task<()>>,
}

const VIEWER_DELTA_WINDOW_MINUTES: u64 = 15;

pub struct ObsSignedOut;
pub struct VTubeSignedOut;

impl EventEmitter<NavRequested> for IntegrationDetail {}
impl EventEmitter<ObsConnected> for IntegrationDetail {}
impl EventEmitter<ObsSignedOut> for IntegrationDetail {}
impl EventEmitter<VTubeSignedOut> for IntegrationDetail {}

impl IntegrationDetail {
    #[expect(
        clippy::too_many_arguments,
        reason = "constructor receives each shared handle it wires"
    )]
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
        twitch_slot: Option<IntegrationSlot<BuiltinObject>>,
        kick_slot: Option<IntegrationSlot<BuiltinObject>>,
        youtube_slot: Option<IntegrationSlot<BuiltinObject>>,
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
            collections,
            option_source,
            obs_client: obs_source,
            vtube_client: _,
            follow: _,
            ban_list: _,
        } = object;
        let is_obs_detail = status.id().as_str() == "obs";
        let obs_password_rejected = is_obs_detail && connectivity.read(cx).obs_password_rejected();
        let conn_obs = cx.observe(&connectivity, move |this, connectivity, cx| {
            this.obs_password_rejected =
                this.is_obs && connectivity.read(cx).obs_password_rejected();
            this.reload(cx)
        });

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
        let version = status.version();
        let connection = status.connection();
        let capability_flags = status.capability_flags();
        let header_actions = status.header_actions();
        let health_metrics = health.metrics();
        let sections = content.sections();
        let quick_actions = reachable_quick_actions(&*quick, collections.as_deref());

        let health_bridge = Self::spawn_health_bridge(&health, cx);
        let quick_action_bridge = Self::spawn_quick_action_bridge(&*quick, cx);
        let mut bridges = Vec::new();

        if is_twitch {
            bridges.push(Self::spawn_eventsub_tally(&event_bus, cx));
        }
        if is_twitch || is_obs {
            bridges.push(Self::spawn_descriptor_reload_watch(&event_bus, cx));
        }
        if is_obs {
            bridges.push(Self::spawn_obs_content_watch(&event_bus, cx));
        }
        if is_twitch || matches!(status.id().as_str(), "youtube" | "kick") {
            bridges.push(Self::spawn_viewer_sampler(&live_viewers, cx));
            bridges.push(Self::spawn_detail_ticker(cx));
        }

        let mut this = Self {
            status,
            health,
            content,
            quick,
            control,
            collections,
            option_source,
            rt_handle,
            action_engine,
            obs_source,
            credentials,
            settings,
            history,
            trigger_registry,
            bus,
            twitch_slot,
            kick_slot,
            youtube_slot,
            obs_install_seed,
            vtube_install_seed,
            connect: None,
            donation_settings: None,
            twitch_bot: None,
            twitch_sign_in: None,
            youtube_sign_in: None,
            kick_sign_in: None,
            platform_credentials: PlatformCredentialManagers::default(),
            _twitch_bot_subs: Vec::new(),
            is_twitch,
            is_obs,
            is_vtube,
            twitch_reauth_required: false,
            obs_password_rejected,
            icon,
            display_name,
            version,
            connection,
            capability_flags,
            header_actions,
            health_metrics,
            sections,
            quick_actions,
            qa_search,
            eventsub_tally: HashMap::new(),
            row_steps_in_flight: InFlightSteps::default(),
            viewer_samples: VecDeque::new(),
            pending_disconnect: Confirm::default(),
            signing_out: false,
            quick_action_modal: None,
            collection_manager: None,
            obs_settings_modal: None,
            history_modal: None,
            _qa_modal_sub: None,
            _collection_manager_sub: None,
            _obs_modal_sub: None,
            _history_modal_subs: Vec::new(),
            _conn_obs: conn_obs,
            _qa_search_sub: qa_search_sub,
            _health_bridge: health_bridge,
            _quick_action_bridge: quick_action_bridge,
            _bridges: bridges,
        };

        if let Some(platform) = connect_platform {
            this.open_connect_flow(platform, cx);
        }
        this
    }
}

impl Render for IntegrationDetail {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let body = match &self.connect {
            Some(connect) => {
                let twitch_bot = self
                    .twitch_bot
                    .clone()
                    .filter(|section| section.read(cx).has_bot_account(cx));
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(scaled(px(14.0), density))
                    .child(connect.view.clone())
                    .children(twitch_bot)
                    .into_any_element()
            }
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
                let refresh: SectionRefresh =
                    Rc::new(cx.listener(|this, _: &ClickEvent, _, cx| this.resync_content(cx)));
                let run_step: SectionStepRun = Rc::new(
                    cx.listener(|this, click: &StepClick, _, cx| this.run_row_step(click, cx)),
                );
                let hooks = SectionHooks {
                    refresh: Some(refresh),
                    run_step: Some(run_step),
                };
                let content =
                    content_sections(&self.augmented_sections(), &hooks, &palette, density);
                let quick = self.quick_actions_card(&palette, density, cx);
                let donation_settings = self
                    .donation_settings
                    .as_ref()
                    .map(|link| link.view.clone());
                let twitch_bot = self.twitch_bot.clone();

                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(scaled(px(14.0), density))
                    .children(reauth_banner)
                    .children(state_banner)
                    .child(header_card)
                    .child(health)
                    .child(content)
                    .children(twitch_bot)
                    .children(donation_settings)
                    .child(quick)
                    .into_any_element()
            }
        };

        let scroll = div()
            .id("integration-detail-scroll")
            .flex_1()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(scaled(px(18.0), density))
                    .px(scaled(px(22.0), density))
                    .child(body),
            );

        let donation_remove_confirm = self
            .donation_settings
            .as_ref()
            .and_then(|link| remove_confirm(&link.view, &palette, cx));
        let disconnect_overlay = self
            .pending_disconnect
            .is_pending()
            .then(|| self.disconnect_overlay(&palette, cx));

        let ancestor_crumb = hub_crumb(
            declaration_of(self.status.id()).map(|declaration| declaration.category),
            cx,
        );

        let is_oauth_platform = self.is_oauth_platform();
        let header_right = match &self.connect {
            Some(connect) => Some(connect.view.read(cx).status_indicator(&palette, density)),
            None if is_oauth_platform && self.connection == ConnectionState::Connected => {
                Some(self.header_right_connected(&palette, density))
            }
            None if is_oauth_platform => None,
            None => Some(self.header_right_connection(&palette, density)),
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
            .children(donation_remove_confirm)
            .children(self.quick_action_modal.clone())
            .children(self.collection_manager.clone())
            .children(self.obs_settings_modal.clone())
            .children(self.history_modal.clone())
    }
}

fn platform_of(id: &str) -> Option<PlatformId> {
    match id {
        "twitch" => Some(PlatformId::Twitch),
        "youtube" => Some(PlatformId::YouTube),
        "kick" => Some(PlatformId::Kick),
        _ => None,
    }
}

fn connect_platform_for(id: &str, has_control: bool) -> Option<PlatformId> {
    if has_control {
        return None;
    }
    platform_of(id)
}
