use std::collections::HashMap;
use std::sync::Arc;

use forge_events::{Delivery, EventPublisher, EventSubscription};
use forge_platform_core::{
    BanListSource, BuiltinCollections, BuiltinContent, BuiltinControl, BuiltinHealth,
    BuiltinOptionSource, BuiltinStatus, ChatPlatform, FollowLookup, PlatformEndpoints,
    QuickActions, SectionIcon,
};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{DonationIngest, EventBus};
use forge_storage::{CredentialsRepo, DataProvider};
use forge_types::{IntegrationId, MessageRef};
use tokio::task::JoinHandle;

use crate::donation_factories::{wire_donatello, wire_monobank};
use crate::donation_services::{DonationServices, restore_donatello_poll_interval};
use crate::integration_factories::{
    wire_discord, wire_hotkey, wire_kick, wire_midi, wire_obs, wire_twitch, wire_vtube,
    wire_youtube,
};
use crate::platform_credentials::PlatformCredentialManagers;
use crate::platform_identities::PlatformIdentities;
use forge_hotkey::HotkeyReconciler;
use forge_runtime::{IntegrationFactory, IntegrationObjects};

#[derive(Clone)]
pub(crate) struct BuiltinObject {
    pub icon: SectionIcon,
    pub status: Arc<dyn BuiltinStatus>,
    pub health: Arc<dyn BuiltinHealth>,
    pub content: Arc<dyn BuiltinContent>,
    pub quick: Arc<dyn QuickActions>,
    pub control: Option<Arc<dyn BuiltinControl>>,
    pub collections: Option<Arc<dyn BuiltinCollections>>,
    pub option_source: Option<Arc<dyn BuiltinOptionSource>>,
    pub obs_client: Option<Arc<forge_obs::ObsClient>>,
    pub vtube_client: Option<Arc<forge_vtube::VTubeClient>>,
    pub follow: Option<Arc<dyn FollowLookup>>,
    pub ban_list: Option<Arc<dyn BanListSource>>,
}

#[derive(Clone, Default)]
pub(crate) struct BuiltinRegistry {
    entries: Arc<std::sync::RwLock<HashMap<IntegrationId, BuiltinObject>>>,
}

impl BuiltinRegistry {
    pub(crate) fn get(&self, id: &IntegrationId) -> Option<BuiltinObject> {
        let guard = self.entries.read().unwrap_or_else(|e| e.into_inner());
        guard.get(id).cloned()
    }

    pub(crate) fn snapshot(&self) -> Vec<BuiltinObject> {
        let guard = self.entries.read().unwrap_or_else(|e| e.into_inner());
        guard.values().cloned().collect()
    }

    pub(crate) fn install(&self, object: BuiltinObject) {
        let id = object.status.id().clone();
        let mut guard = self.entries.write().unwrap_or_else(|e| e.into_inner());
        guard.insert(id, object);
    }

    pub(crate) fn remove(&self, id: &IntegrationId) {
        let mut guard = self.entries.write().unwrap_or_else(|e| e.into_inner());
        guard.remove(id);
    }
}

impl IntegrationObjects<BuiltinObject> for BuiltinRegistry {
    fn install(&self, object: BuiltinObject) {
        BuiltinRegistry::install(self, object);
    }

    fn remove(&self, id: &IntegrationId) {
        BuiltinRegistry::remove(self, id);
    }

    fn get(&self, id: &IntegrationId) -> Option<BuiltinObject> {
        BuiltinRegistry::get(self, id)
    }
}

pub(crate) struct Integrations {
    pub builtins: BuiltinRegistry,
    pub factories: Vec<Arc<dyn IntegrationFactory<BuiltinObject>>>,
    pub obs_install_seed: ObsInstallSeed,
    pub vtube_install_seed: VTubeInstallSeed,
    pub discord_client: Option<Arc<forge_discord::DiscordClient>>,
    pub midi_sink: Arc<forge_midi::SwitchableMidiSink>,
    pub hotkey_client: Option<Arc<forge_hotkey::HotkeyClient>>,
    pub hotkey_reconciler: Option<Arc<HotkeyReconciler>>,
    pub donation_services: DonationServices,
    pub twitch_bot: Option<Arc<forge_platform_twitch::TwitchBotAccount>>,
    pub twitch_sign_in: Option<forge_platform_twitch::TwitchSignIn>,
    pub youtube_sign_in: Option<forge_platform_youtube::YoutubeSignIn>,
    pub kick_sign_in: Option<forge_platform_kick::KickSignIn>,
    pub platform_credentials: PlatformCredentialManagers,
}

#[derive(Clone)]
pub(crate) struct ObsInstallSeed {
    sink: Arc<forge_obs::SwitchableObsSink>,
    live: Arc<std::sync::RwLock<Option<Arc<forge_obs::ObsClient>>>>,
}

impl ObsInstallSeed {
    pub(crate) fn new(sink: Arc<forge_obs::SwitchableObsSink>) -> Self {
        Self {
            sink,
            live: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    pub(crate) fn stream_output(&self) -> forge_obs::StreamOutputActive {
        self.sink.stream_output()
    }

    pub(crate) fn stream_stats(&self) -> forge_obs::ObsStatsWatch {
        self.sink.stream_stats()
    }

    pub(crate) fn install(&self, client: Arc<forge_obs::ObsClient>) {
        self.sink.install(Arc::clone(&client));
        let mut guard = self.live.write().unwrap_or_else(|e| e.into_inner());
        *guard = Some(client);
    }

    pub(crate) fn clear(&self) {
        let mut guard = self.live.write().unwrap_or_else(|e| e.into_inner());
        *guard = None;
    }

    pub(crate) async fn disconnect_live(&self) {
        let previous = {
            let mut guard = self.live.write().unwrap_or_else(|e| e.into_inner());
            guard.take()
        };
        if let Some(client) = previous
            && let Err(err) = client.disconnect().await
        {
            tracing::warn!(error = %err, "disconnecting the live client failed");
        }
    }

    pub(crate) fn live(&self) -> Option<Arc<forge_obs::ObsClient>> {
        let guard = self.live.read().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    }

    pub(crate) fn connect_failure(&self) -> Option<MessageRef> {
        self.live()?.connect_failure()
    }
}

#[derive(Clone)]
pub(crate) struct VTubeInstallSeed {
    sink: Arc<forge_vtube::SwitchableVTubeSink>,
    live: Arc<std::sync::RwLock<Option<Arc<forge_vtube::VTubeClient>>>>,
}

impl VTubeInstallSeed {
    pub(crate) fn new(sink: Arc<forge_vtube::SwitchableVTubeSink>) -> Self {
        Self {
            sink,
            live: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    pub(crate) fn install(&self, client: Arc<forge_vtube::VTubeClient>) {
        self.sink.install(Arc::clone(&client));
        let mut guard = self.live.write().unwrap_or_else(|e| e.into_inner());
        *guard = Some(client);
    }

    pub(crate) fn clear(&self) {
        let mut guard = self.live.write().unwrap_or_else(|e| e.into_inner());
        *guard = None;
    }

    pub(crate) async fn disconnect_live(&self) {
        let previous = {
            let mut guard = self.live.write().unwrap_or_else(|e| e.into_inner());
            guard.take()
        };
        if let Some(client) = previous
            && let Err(err) = client.disconnect().await
        {
            tracing::warn!(error = %err, "disconnecting the live client failed");
        }
    }

    pub(crate) fn live(&self) -> Option<Arc<forge_vtube::VTubeClient>> {
        let guard = self.live.read().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    }

    pub(crate) fn connect_failure(&self) -> Option<MessageRef> {
        self.live()?.connect_failure()
    }
}

pub(crate) fn vtube_builtin_object(client: Arc<forge_vtube::VTubeClient>) -> BuiltinObject {
    BuiltinObject {
        icon: SectionIcon::new("mood-smile"),
        status: client.clone(),
        health: client.clone(),
        content: client.clone(),
        quick: client.clone(),
        control: Some(Arc::clone(&client) as Arc<dyn BuiltinControl>),
        collections: None,
        option_source: Some(Arc::clone(&client) as Arc<dyn BuiltinOptionSource>),
        obs_client: None,
        vtube_client: Some(client),
        follow: None,
        ban_list: None,
    }
}

pub(crate) fn obs_builtin_object(client: Arc<forge_obs::ObsClient>) -> BuiltinObject {
    BuiltinObject {
        icon: SectionIcon::new("broadcast"),
        status: client.clone(),
        health: client.clone(),
        content: client.clone(),
        quick: client.clone(),
        control: Some(Arc::clone(&client) as Arc<dyn BuiltinControl>),
        collections: None,
        option_source: Some(Arc::clone(&client) as Arc<dyn BuiltinOptionSource>),
        obs_client: Some(client),
        vtube_client: None,
        follow: None,
        ban_list: None,
    }
}

pub(crate) fn twitch_builtin_object(
    bundle: Arc<forge_platform_twitch::TwitchIntegrationBundle>,
) -> BuiltinObject {
    BuiltinObject {
        icon: SectionIcon::new("brand-twitch"),
        status: bundle.clone(),
        health: bundle.clone(),
        content: bundle.clone(),
        quick: bundle.clone(),
        control: Some(Arc::clone(&bundle) as Arc<dyn BuiltinControl>),
        collections: Some(Arc::clone(&bundle) as Arc<dyn BuiltinCollections>),
        option_source: Some(Arc::clone(&bundle) as Arc<dyn BuiltinOptionSource>),
        obs_client: None,
        vtube_client: None,
        follow: Some(Arc::clone(&bundle) as Arc<dyn FollowLookup>),
        ban_list: Some(bundle as Arc<dyn BanListSource>),
    }
}

pub(crate) fn youtube_builtin_object(
    bundle: Arc<forge_platform_youtube::YoutubeIntegrationBundle>,
) -> BuiltinObject {
    BuiltinObject {
        icon: SectionIcon::new("brand-youtube"),
        status: bundle.clone(),
        health: bundle.clone(),
        content: bundle.clone(),
        quick: bundle.clone(),
        control: Some(Arc::clone(&bundle) as Arc<dyn BuiltinControl>),
        collections: None,
        option_source: None,
        obs_client: None,
        vtube_client: None,
        follow: Some(Arc::clone(&bundle) as Arc<dyn FollowLookup>),
        ban_list: Some(bundle as Arc<dyn BanListSource>),
    }
}

pub(crate) fn kick_builtin_object(
    bundle: Arc<forge_platform_kick::KickIntegrationBundle>,
) -> BuiltinObject {
    BuiltinObject {
        icon: SectionIcon::new("brand-kick"),
        status: bundle.clone(),
        health: bundle.clone(),
        content: bundle.clone(),
        quick: bundle.clone(),
        control: Some(Arc::clone(&bundle) as Arc<dyn BuiltinControl>),
        collections: None,
        option_source: None,
        obs_client: None,
        vtube_client: None,
        follow: None,
        ban_list: Some(bundle as Arc<dyn BanListSource>),
    }
}

pub(crate) struct IntegrationInputs {
    pub endpoints: PlatformEndpoints,
    pub hotkey_main_thread: forge_hotkey::MainThreadLink,
    pub platform_identities: PlatformIdentities,
}

pub(crate) async fn build_integrations(
    sub_actions: &mut SubActionRegistry,
    triggers: &mut TriggerRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    donations: &Arc<DonationIngest>,
    inputs: IntegrationInputs,
) -> Integrations {
    let IntegrationInputs {
        endpoints,
        hotkey_main_thread,
        platform_identities,
    } = inputs;
    let endpoints = &endpoints;
    register_platform_triggers(triggers);
    let midi_kinds = kinds_owned_by(triggers, &forge_midi::MIDI_INTEGRATION.id);

    let mut factories: Vec<Arc<dyn IntegrationFactory<BuiltinObject>>> = Vec::new();
    let mut twitch_bot = None;
    let mut twitch_sign_in = None;
    let mut platform_credentials = PlatformCredentialManagers::default();
    if let Some(twitch) = platform_identities
        .twitch_client_id
        .and_then(|client_id| wire_twitch(sub_actions, backend, bus, endpoints, client_id))
    {
        twitch_bot = Some(twitch.bot());
        twitch_sign_in = Some(twitch.sign_in());
        platform_credentials.twitch = Some(twitch.credentials_manager());
        factories.push(Arc::new(twitch));
    }
    let obs = wire_obs(sub_actions, backend, bus);
    let obs_install_seed = obs.seed();
    factories.push(Arc::new(obs));
    let vtube = wire_vtube(sub_actions, backend, bus);
    let vtube_install_seed = vtube.seed();
    factories.push(Arc::new(vtube));
    let mut discord_client = None;
    if let Some(discord) = wire_discord(sub_actions, backend, bus, endpoints) {
        discord_client = Some(discord.client());
        factories.push(Arc::new(discord));
    }
    let midi = wire_midi(sub_actions, backend, bus, midi_kinds);
    let midi_sink = midi.sink();
    factories.push(Arc::new(midi));
    let (hotkey, hotkey_reconciler) = wire_hotkey(backend, bus, hotkey_main_thread).await;
    let hotkey_client = hotkey.client();
    factories.push(Arc::new(hotkey));
    let mut youtube_sign_in = None;
    if let Some(youtube) = platform_identities
        .youtube
        .and_then(|identity| wire_youtube(sub_actions, backend, bus, endpoints, identity))
    {
        youtube_sign_in = Some(youtube.sign_in());
        platform_credentials.youtube = Some(youtube.credentials_manager());
        factories.push(Arc::new(youtube));
    }
    let mut kick_sign_in = None;
    if let Some(kick) = platform_identities
        .kick
        .and_then(|identity| wire_kick(sub_actions, backend, bus, endpoints, identity))
    {
        kick_sign_in = Some(kick.sign_in());
        platform_credentials.kick = Some(kick.credentials_manager());
        factories.push(Arc::new(kick));
    }
    let mut donation_services = DonationServices::default();
    if let Some(donatello) = wire_donatello(backend, endpoints, donations, bus) {
        let provider = donatello.provider();
        restore_donatello_poll_interval(&provider, backend.as_ref()).await;
        donation_services.donatello = Some(provider);
        factories.push(Arc::new(donatello));
    }
    if let Some(monobank) = wire_monobank(backend, endpoints, donations, bus) {
        donation_services.monobank = Some(monobank.provider());
        factories.push(Arc::new(monobank));
    }

    Integrations {
        builtins: BuiltinRegistry::default(),
        factories,
        obs_install_seed,
        vtube_install_seed,
        discord_client,
        midi_sink,
        hotkey_client: Some(hotkey_client),
        hotkey_reconciler: Some(hotkey_reconciler),
        donation_services,
        twitch_bot,
        twitch_sign_in,
        youtube_sign_in,
        kick_sign_in,
        platform_credentials,
    }
}

fn kinds_owned_by(triggers: &TriggerRegistry, owner: &IntegrationId) -> Vec<String> {
    triggers
        .all()
        .map(|descriptor| descriptor.id())
        .filter(|kind| triggers.owning_integration(kind) == Some(owner))
        .map(ToOwned::to_owned)
        .collect()
}

fn register_platform_triggers(triggers: &mut TriggerRegistry) {
    if let Err(e) = forge_platform_twitch::register_twitch_triggers(triggers) {
        tracing::error!(error = %e, "twitch trigger registration failed");
    }
    if let Err(e) = forge_obs::register_obs_triggers(triggers) {
        tracing::error!(error = %e, "obs trigger registration failed");
    }
    if let Err(e) = forge_vtube::register_vtube_triggers(triggers) {
        tracing::error!(error = %e, "vtube trigger registration failed");
    }
    if let Err(e) = forge_midi::register_midi_triggers(triggers) {
        tracing::error!(error = %e, "midi trigger registration failed");
    }
    if let Err(e) = forge_hotkey::register_hotkey_triggers(triggers) {
        tracing::error!(error = %e, "hotkey trigger registration failed");
    }
    if let Err(e) = forge_platform_youtube::register_youtube_triggers(triggers) {
        tracing::error!(error = %e, "youtube trigger registration failed");
    }
    if let Err(e) = forge_platform_kick::register_kick_triggers(triggers) {
        tracing::error!(error = %e, "kick trigger registration failed");
    }
}

pub(crate) fn publisher(bus: &Arc<EventBus>) -> Arc<dyn EventPublisher> {
    Arc::clone(bus) as Arc<dyn EventPublisher>
}

pub(crate) fn creds_of(backend: &Arc<dyn DataProvider>) -> Arc<dyn CredentialsRepo> {
    Arc::clone(backend) as Arc<dyn CredentialsRepo>
}

pub(crate) fn spawn_event_bridge(
    bus: Arc<dyn EventPublisher>,
    mut events: EventSubscription,
    label: &'static str,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match events.next().await {
                Delivery::Event(event) => bus.publish(Arc::unwrap_or_clone(event)),
                Delivery::Closed => break,
                Delivery::Skipped(skipped) => {
                    tracing::warn!(bridge = label, skipped, "event bridge dropped events");
                }
            }
        }
    })
}

pub(crate) fn spawn_connect(
    platform: Arc<dyn ChatPlatform>,
    label: &'static str,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = platform.connect().await {
            tracing::error!(platform = label, error = %e, "chat platform connect failed");
        }
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::event_loss::TWITCH_EVENT_RELAY;
    use crate::test_support::yield_until;
    use forge_events::{Event, EventChannel, EventSource};
    use forge_net::{RateLimitError, RateLimitOutcome, RateLimiter};
    use forge_platform_core::{ConnectionState, PlatformError};
    use forge_runtime::NullEventLogRepo;
    use std::time::Duration;

    struct NoopRateLimiter;

    #[async_trait::async_trait]
    impl RateLimiter for NoopRateLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, RateLimitError> {
            Ok(RateLimitOutcome::Granted)
        }

        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    fn test_bus() -> Arc<EventBus> {
        EventBus::new(Arc::new(NullEventLogRepo))
    }

    async fn listening_but_silent_port() -> (tokio::net::TcpListener, u16) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    #[tokio::test]
    async fn retiring_the_live_client_empties_the_slot_and_leaves_it_disconnected() {
        let (_listener, port) = listening_but_silent_port().await;
        let seed = ObsInstallSeed::new(forge_obs::SwitchableObsSink::new());
        let client = Arc::new(
            forge_obs::ObsClient::connect(
                &format!("127.0.0.1:{port}"),
                None,
                Arc::new(crate::test_support::SilentPublisher),
            )
            .await
            .unwrap(),
        );
        seed.install(Arc::clone(&client));

        seed.disconnect_live().await;

        assert!(
            seed.live().is_none(),
            "a reconnect whose new connect fails must leave no live client behind"
        );
        assert_eq!(
            client.connection_state(),
            ConnectionState::Disconnected,
            "the retired client was released before its supervisor joined"
        );
    }

    async fn closed_port() -> u16 {
        let (listener, port) = listening_but_silent_port().await;
        drop(listener);
        port
    }

    #[tokio::test]
    async fn an_obs_seed_reports_the_live_clients_connect_failure_until_cleared() {
        let seed = ObsInstallSeed::new(forge_obs::SwitchableObsSink::new());
        assert!(seed.connect_failure().is_none());
        let port = closed_port().await;
        let client = Arc::new(
            forge_obs::ObsClient::connect(
                &format!("127.0.0.1:{port}"),
                None,
                Arc::new(crate::test_support::SilentPublisher),
            )
            .await
            .unwrap(),
        );
        client.set_auto_reconnect(false);
        seed.install(Arc::clone(&client));

        yield_until(|| seed.connect_failure().is_some()).await;
        let code = seed
            .connect_failure()
            .map(|failure| failure.code().name().to_owned());
        seed.clear();

        assert_eq!(code.as_deref(), Some("connect_refused"));
        assert!(seed.connect_failure().is_none());
    }

    #[tokio::test]
    async fn a_vtube_seed_reports_the_live_clients_connect_failure_until_cleared() {
        let seed = VTubeInstallSeed::new(forge_vtube::SwitchableVTubeSink::new());
        assert!(seed.connect_failure().is_none());
        let port = closed_port().await;
        let (backend, _writes) = crate::test_support::test_backend();
        let client = Arc::new(forge_vtube::VTubeClient::connect(
            forge_vtube::VTubeConfig {
                endpoint: format!("ws://127.0.0.1:{port}/"),
            },
            Arc::new(crate::test_support::SilentPublisher),
            backend as Arc<dyn forge_storage::CredentialsRepo>,
        ));
        client.set_auto_reconnect(false);
        seed.install(Arc::clone(&client));

        yield_until(|| seed.connect_failure().is_some()).await;
        let code = seed
            .connect_failure()
            .map(|failure| failure.code().name().to_owned());
        seed.clear();

        assert_eq!(code.as_deref(), Some("connect_refused"));
        assert!(seed.connect_failure().is_none());
    }

    mod detail_lifetime {
        use std::sync::Mutex;

        use forge_components::{Density, ThemeId};
        use forge_platform_core::{
            BuiltinContent, BuiltinHealth, BuiltinStatus, CapabilityFlags, ConnectionState,
            DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthMetricId, HealthStream,
            HealthValue, QuickAction, QuickActions, SectionIcon,
        };
        use forge_runtime::{
            ActionCancelRegistry, spawn_action_engine, spawn_live_viewer_aggregator,
        };
        use forge_types::{IntegrationId, MessageCode};
        use gpui::{AppContext as _, TestAppContext};

        use super::super::{BuiltinObject, ObsInstallSeed, VTubeInstallSeed};
        use crate::integration_detail::IntegrationDetail;
        use crate::platforms::PlatformConnectivity;
        use crate::test_support::{StubActions, StubEventLog, StubHistory, runtime, test_backend};
        use forge_components::Presentation;
        use forge_registry::{SubActionRegistry, TriggerRegistry};
        use forge_runtime::EventBus;
        use forge_storage::{CredentialsRepo, SettingsRepo};
        use std::sync::Arc;
        use std::time::Duration;
        use tokio::sync::mpsc;

        struct HeldStreamBuiltin {
            id: IntegrationId,
            deltas: Mutex<Option<mpsc::UnboundedReceiver<HealthDelta>>>,
        }

        impl BuiltinStatus for HeldStreamBuiltin {
            fn id(&self) -> &IntegrationId {
                &self.id
            }
            fn display_name(&self) -> &str {
                "OBS"
            }
            fn version(&self) -> Option<String> {
                None
            }
            fn connection(&self) -> ConnectionState {
                ConnectionState::Connected
            }
            fn uptime(&self) -> Option<Duration> {
                None
            }
            fn endpoint(&self) -> Option<forge_types::UserText> {
                None
            }
            fn capability_flags(&self) -> CapabilityFlags {
                CapabilityFlags {
                    limited: false,
                    label: None,
                }
            }
            fn header_actions(&self) -> Vec<HeaderAction> {
                Vec::new()
            }
        }

        const STUB_METRIC_IDS: [HealthMetricId; 4] = [
            HealthMetricId::new(MessageCode::from_static("stub", "metric0")),
            HealthMetricId::new(MessageCode::from_static("stub", "metric1")),
            HealthMetricId::new(MessageCode::from_static("stub", "metric2")),
            HealthMetricId::new(MessageCode::from_static("stub", "metric3")),
        ];

        impl BuiltinHealth for HeldStreamBuiltin {
            fn metrics(&self) -> [HealthMetric; 4] {
                std::array::from_fn(|i| HealthMetric {
                    id: STUB_METRIC_IDS[i].clone(),
                    value: HealthValue::Text {
                        primary: forge_types::VerbatimText::new("").into(),
                        secondary: None,
                    },
                })
            }
            fn stream(&self) -> HealthStream {
                let rx = self.deltas.lock().unwrap().take().unwrap();
                Box::pin(futures_util::stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|delta| (delta, rx))
                }))
            }
        }

        impl BuiltinContent for HeldStreamBuiltin {
            fn sections(&self) -> Vec<DetailSection> {
                Vec::new()
            }
        }

        impl QuickActions for HeldStreamBuiltin {
            fn actions(&self) -> Vec<QuickAction> {
                Vec::new()
            }
        }

        #[gpui::test]
        fn closing_an_integration_screen_ends_the_task_holding_its_health_stream(
            cx: &mut TestAppContext,
        ) {
            cx.update(|cx| {
                cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            });
            let rt = runtime();
            let _enter = rt.enter();
            let (deltas_tx, deltas_rx) = mpsc::unbounded_channel();
            let probe = Arc::new(HeldStreamBuiltin {
                id: IntegrationId::new("obs"),
                deltas: Mutex::new(Some(deltas_rx)),
            });
            let object = BuiltinObject {
                icon: SectionIcon::new("bug"),
                status: probe.clone(),
                health: probe.clone(),
                content: probe.clone(),
                quick: probe,
                control: None,
                collections: None,
                option_source: None,
                obs_client: None,
                vtube_client: None,
                follow: None,
                ban_list: None,
            };
            let bus = EventBus::new(Arc::new(StubEventLog));
            let engine = spawn_action_engine(
                Arc::clone(&bus),
                crate::test_support::stub_catalog(),
                Arc::new(StubActions),
                Arc::new(StubHistory),
                Arc::new(SubActionRegistry::new()),
                Arc::new(ActionCancelRegistry::new()),
            );
            let (backend, _writes) = test_backend();
            let connectivity = cx.new(|_| PlatformConnectivity::new());
            let view = cx.new(|cx| {
                IntegrationDetail::new(
                    object,
                    rt.handle().clone(),
                    engine,
                    Arc::clone(&backend) as Arc<dyn CredentialsRepo>,
                    backend as Arc<dyn SettingsRepo>,
                    Arc::new(StubHistory),
                    Arc::new(TriggerRegistry::new()),
                    Arc::clone(&bus) as Arc<dyn forge_events::EventPublisher>,
                    Arc::clone(&bus),
                    spawn_live_viewer_aggregator(),
                    None,
                    None,
                    None,
                    ObsInstallSeed::new(forge_obs::SwitchableObsSink::new()),
                    VTubeInstallSeed::new(forge_vtube::SwitchableVTubeSink::new()),
                    connectivity,
                    cx,
                )
            });
            cx.run_until_parked();
            assert!(
                !deltas_tx.is_closed(),
                "the open screen must hold its health stream"
            );

            drop(view);
            cx.update(|_| {});
            cx.run_until_parked();

            assert!(
                deltas_tx.is_closed(),
                "the closed screen's health task must end without waiting for a delta"
            );
        }
    }

    pub(crate) struct SilentSoundboard;

    #[async_trait::async_trait]
    impl forge_soundboard::SoundboardControl for SilentSoundboard {
        async fn play(
            &self,
            _clip_id: forge_types::ClipId,
            _output_device_override: Option<forge_types::OutputDevice>,
        ) -> Result<(), forge_soundboard::SoundboardError> {
            Ok(())
        }

        async fn stop(
            &self,
            _clip_id: forge_types::ClipId,
        ) -> Result<(), forge_soundboard::SoundboardError> {
            Ok(())
        }

        async fn stop_all(&self) -> Result<(), forge_soundboard::SoundboardError> {
            Ok(())
        }

        async fn set_master_volume(
            &self,
            _gain: f32,
        ) -> Result<(), forge_soundboard::SoundboardError> {
            Ok(())
        }
    }

    struct SilentSpeaker;

    #[async_trait::async_trait]
    impl forge_runtime::SpeakDispatcher for SilentSpeaker {
        async fn speak(
            &self,
            _text: String,
            _voice: Option<String>,
            _origin: forge_runtime::SpeechOrigin,
        ) -> Result<(), forge_runtime::SpeakDispatchError> {
            Ok(())
        }
    }

    struct NoLatestValues;

    impl forge_types::LatestValueReader for NoLatestValues {
        fn latest(
            &self,
            _slot: &str,
            _scope: forge_types::LatestScope<'_>,
        ) -> Option<forge_types::LatestValue> {
            None
        }
    }

    pub(crate) async fn every_registry_entry() -> (Arc<TriggerRegistry>, Arc<SubActionRegistry>) {
        let media_root = tempfile::tempdir().unwrap();
        let backend: Arc<dyn DataProvider> = Arc::new(
            forge_storage_sqlite::SqliteBackend::open_for_test(
                "sqlite::memory:",
                [0x11; 32],
                media_root.path().to_owned(),
                None,
            )
            .await
            .unwrap(),
        );
        let bus = test_bus();
        let latest_values = forge_runtime::LatestValues::load(
            backend.latest_value_repo(),
            publisher(&bus),
            forge_types::Shared::new(String::new()),
        )
        .await;
        let hub = forge_server::DeliveryHub::new(
            Arc::clone(&bus) as Arc<dyn forge_events::EventObserver>
        );
        let mut wiring = forge_runtime::RuntimeWiring::new(forge_runtime::RuntimeWiringParts {
            bus: Arc::clone(&bus),
            provider: Arc::clone(&backend),
            latest_values,
            script_speech: None,
            overlay_speech: None,
            overlay_kinds: Arc::new(forge_overlay::OverlayKindRegistry::new()),
            overlay_frames: Arc::new(hub) as Arc<dyn forge_runtime::OverlayFrameSink>,
            clock: Arc::new(forge_types::SystemWallClock),
        });
        wiring.register_core_sub_actions().unwrap();
        wiring.register_scheduled_run_sub_actions().unwrap();
        forge_runtime::register_core_triggers(wiring.triggers_mut()).unwrap();
        let (sub_actions, triggers) = wiring.registries_mut();
        register_platform_triggers(triggers);
        wire_obs(&mut *sub_actions, &backend, &bus);
        wire_vtube(&mut *sub_actions, &backend, &bus);
        wire_discord(
            &mut *sub_actions,
            &backend,
            &bus,
            &PlatformEndpoints::default(),
        );
        wire_midi(&mut *sub_actions, &backend, &bus, Vec::new());
        register_twitch_runners(&mut *sub_actions, &backend, &bus);
        register_youtube_runners(&mut *sub_actions, &bus);
        register_kick_runners(&mut *sub_actions, &bus);
        forge_soundboard::register_soundboard_sub_actions(
            &mut *sub_actions,
            Arc::new(SilentSoundboard),
        )
        .unwrap();
        forge_runtime::register_speech_sub_actions(
            &mut *sub_actions,
            Arc::new(SilentSpeaker),
            forge_runtime::TwitchEmoteLexicon::default(),
        )
        .unwrap();
        forge_runtime::register_donation_sub_actions(
            &mut *sub_actions,
            Arc::new(DonationIngest::new(
                backend.donation_repo(),
                publisher(&bus),
                forge_types::Shared::new(String::new()),
            )),
        )
        .unwrap();
        forge_runtime::register_latest_sub_actions(&mut *sub_actions, Arc::new(NoLatestValues))
            .unwrap();
        let frozen = wiring.freeze().await;
        (frozen.trigger_registry, frozen.sub_action_registry)
    }

    fn register_twitch_runners(
        sub_actions: &mut SubActionRegistry,
        backend: &Arc<dyn DataProvider>,
        bus: &Arc<EventBus>,
    ) {
        let creds = creds_of(backend);
        let manager = Arc::new(forge_platform_twitch::TwitchCredentialsManager::new(
            crate::test_support::default_twitch_oauth_client(),
            Arc::clone(&creds),
        ));
        let transport: Arc<dyn forge_platform_twitch::HelixTransport> =
            Arc::new(forge_platform_twitch::HelixHttpTransport::new(
                crate::test_support::default_twitch_helix_gateway(publisher(bus)),
                "test-client".to_owned(),
                manager as Arc<dyn forge_platform_twitch::HelixTokenSource>,
            ));
        forge_platform_twitch::register_twitch_sub_actions(
            sub_actions,
            transport,
            creds,
            forge_platform_twitch::TwitchLifecycle::new(),
            None,
        )
        .unwrap();
    }

    fn youtube_token_source() -> Arc<
        dyn Fn() -> futures_util::future::BoxFuture<'static, Result<String, PlatformError>>
            + Send
            + Sync,
    > {
        Arc::new(|| Box::pin(async { Ok("token".to_owned()) }))
    }

    fn register_youtube_runners(sub_actions: &mut SubActionRegistry, bus: &Arc<EventBus>) {
        use forge_platform_youtube as yt;
        let api = yt::YoutubeDataApi::new(
            &forge_platform_core::PlatformEndpoints::default(),
            yt::SharedQuota::default(),
            publisher(bus),
        )
        .unwrap();
        let chat = yt::LiveChatIdHandle::new();
        let broadcast = yt::ActiveBroadcastIdHandle::new();
        yt::register_youtube_sub_actions(
            sub_actions,
            Arc::new(yt::YoutubeSendChat::new(
                api.clone(),
                youtube_token_source(),
                chat.clone(),
            )),
            Arc::new(yt::YoutubeModeration::new(
                api.clone(),
                youtube_token_source(),
                Arc::new(|| {
                    Box::pin(async {
                        Ok(yt::YoutubeBroadcaster {
                            channel_id: "UCtest".to_owned(),
                            channel_title: "Test".to_owned(),
                        })
                    })
                }),
                chat,
                Arc::new(forge_storage::MockBanLedgerRepo::new()),
            )),
            Arc::new(yt::YoutubeStreamMetadata::new(
                api.clone(),
                youtube_token_source(),
                broadcast.clone(),
            )),
            Arc::new(yt::YoutubeStreamStats::new(
                api.clone(),
                youtube_token_source(),
                broadcast.clone(),
            )),
            Arc::new(yt::YoutubeAdBreak::new(
                api.clone(),
                youtube_token_source(),
                broadcast.clone(),
            )),
            Arc::new(yt::YoutubeThumbnail::new(
                api.clone(),
                youtube_token_source(),
                broadcast,
            )),
            Arc::new(yt::YoutubeChannelLookup::new(api, youtube_token_source())),
        )
        .unwrap();
    }

    fn register_kick_runners(sub_actions: &mut SubActionRegistry, bus: &Arc<EventBus>) {
        use forge_platform_kick as kick;
        let api = kick::kick_api_gateway(
            &forge_platform_core::PlatformEndpoints::default(),
            Arc::new(NoopRateLimiter),
            publisher(bus),
        )
        .unwrap();
        kick::register_kick_sub_actions(
            sub_actions,
            kick::KickSubActionDeps {
                client: Arc::new(kick::KickSendChat::new(api.clone())),
                token_source: Arc::new(|| Box::pin(async { Ok("token".to_owned()) })),
                broadcaster_id_source: Arc::new(|| Box::pin(async { Ok(1) })),
                moderation: Arc::new(kick::KickModeration::new(
                    api.clone(),
                    Arc::new(forge_storage::MockBanLedgerRepo::new()),
                )),
                channel: Arc::new(kick::KickChannel::new(api.clone())),
                rewards: Arc::new(kick::KickRewards::new(api.clone())),
                categories: Arc::new(kick::KickCategories::new(api)),
            },
        )
        .unwrap();
    }

    fn declared_integrations() -> Vec<IntegrationId> {
        vec![
            forge_platform_twitch::TWITCH_INTEGRATION.id,
            forge_platform_youtube::YOUTUBE_INTEGRATION.id,
            forge_platform_kick::KICK_INTEGRATION.id,
            forge_obs::OBS_INTEGRATION.id,
            forge_vtube::VTUBE_INTEGRATION.id,
            forge_discord::DISCORD_INTEGRATION.id,
            forge_midi::MIDI_INTEGRATION.id,
            forge_hotkey::HOTKEY_INTEGRATION.id,
        ]
    }

    const TARGET_ROUTED_CORE_RUNNERS: &[&str] = &["twitch.chat.send_message"];

    fn ownership_violations<'a>(
        kinds: impl Iterator<Item = &'a str>,
        owner_of: impl Fn(&str) -> Option<IntegrationId>,
    ) -> Vec<String> {
        let declared = declared_integrations();
        kinds
            .filter(|kind| !TARGET_ROUTED_CORE_RUNNERS.contains(kind))
            .filter_map(|kind| {
                let namespace = kind.split('.').next().unwrap_or(kind);
                let expected = declared
                    .iter()
                    .find(|integration| integration.as_str() == namespace)
                    .cloned();
                let actual = owner_of(kind);
                (actual != expected)
                    .then(|| format!("{kind}: owned by {actual:?}, expected {expected:?}"))
            })
            .collect()
    }

    #[tokio::test]
    async fn every_trigger_belongs_to_the_integration_named_by_its_namespace_or_to_core() {
        let (triggers, _) = every_registry_entry().await;

        let violations = ownership_violations(triggers.all().map(|d| d.id()), |kind| {
            triggers.owning_integration(kind).cloned()
        });

        assert!(violations.is_empty(), "{violations:#?}");
        for integration in [
            forge_platform_twitch::TWITCH_INTEGRATION.id,
            forge_midi::MIDI_INTEGRATION.id,
            forge_hotkey::HOTKEY_INTEGRATION.id,
        ] {
            assert!(
                triggers
                    .all()
                    .any(|d| triggers.owning_integration(d.id()) == Some(&integration)),
                "{integration} owns no trigger"
            );
        }
    }

    #[tokio::test]
    async fn every_sub_action_belongs_to_the_integration_named_by_its_namespace_or_to_core() {
        let (_, sub_actions) = every_registry_entry().await;

        let violations = ownership_violations(sub_actions.all().map(|r| r.id()), |kind| {
            sub_actions.owning_integration(kind).cloned()
        });

        assert!(violations.is_empty(), "{violations:#?}");
        for integration in declared_integrations()
            .into_iter()
            .filter(|id| id.as_str() != "hotkey")
        {
            assert!(
                sub_actions
                    .all()
                    .any(|r| sub_actions.owning_integration(r.id()) == Some(&integration)),
                "{integration} owns no sub-action"
            );
        }
    }

    #[tokio::test]
    async fn event_bridge_forwards_a_platform_s_final_events_then_exits_once_its_channel_closes() {
        let bus = test_bus();
        let mut observer = bus.subscribe();
        let platform = EventChannel::new(16);
        let bridge = spawn_event_bridge(publisher(&bus), platform.subscribe(), "twitch");

        for state in ["connected", "disconnected"] {
            platform.publish(Event::new(
                EventSource::Core,
                "platform.connection.changed",
                serde_json::json!({ "platform_id": "twitch", "state": state }),
            ));
        }
        drop(platform);

        tokio::time::timeout(Duration::from_secs(30), bridge)
            .await
            .expect("the bridge must exit once every platform sender is gone")
            .unwrap();
        for state in ["connected", "disconnected"] {
            let event = observer.recv().await.unwrap();
            assert_eq!(event.payload["state"], state);
        }
    }

    #[tokio::test]
    async fn a_lagging_event_bridge_writes_the_skipped_events_to_the_ledger_and_keeps_forwarding() {
        const CAPACITY: usize = 4;
        const SENT: usize = 10;
        let bus = test_bus();
        let mut observer = bus.subscribe();
        let platform = EventChannel::new(CAPACITY);
        let events = platform
            .subscribe()
            .recording_lag(bus.relay_lag(TWITCH_EVENT_RELAY));
        for index in 0..SENT {
            platform.publish(Event::new(
                EventSource::Core,
                "platform.connection.changed",
                serde_json::json!({ "index": index }),
            ));
        }
        drop(platform);

        tokio::time::timeout(
            Duration::from_secs(30),
            spawn_event_bridge(publisher(&bus), events, "twitch"),
        )
        .await
        .expect("the bridge must exit once every platform sender is gone")
        .unwrap();

        let skipped = bus
            .loss_report()
            .into_iter()
            .find(|entry| entry.consumer == TWITCH_EVENT_RELAY)
            .map(|entry| entry.loss.skipped);
        assert_eq!(skipped, Some((SENT - CAPACITY) as u64));
        for index in (SENT - CAPACITY)..SENT {
            let event = observer.recv().await.unwrap();
            assert_eq!(event.payload["index"], index);
        }
    }
}
