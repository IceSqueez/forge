use forge_types::{IntegrationId, LiveSlot, MessageRef};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Delivery, EventPublisher, EventSubscription};
use forge_platform_core::{
    BanListSource, BuiltinCollections, BuiltinContent, BuiltinControl, BuiltinCurrentValues,
    BuiltinHealth, BuiltinOptionSource, BuiltinStatus, ChatPlatform, ControlOutcome, FollowLookup,
    PlatformEndpoints, QuickActions,
};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{ChatAvatars, ChatEmotes, DonationIngest, EventBus};
use forge_storage::{CredentialsRepo, DataProvider};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::donation_factories::{wire_donatello, wire_monobank};
use crate::donation_services::{DonationService, DonationServices, donatello_service};
use crate::integration_catalog::declarations;
use crate::integration_factories::{
    ChatImageBooks, wire_discord, wire_hotkey, wire_kick, wire_midi, wire_obs, wire_twitch,
    wire_vtube, wire_youtube,
};
use crate::integration_sign_out::{SignOutRoute, SignOutRoutes};
use crate::platform_credentials::PlatformCredentialManagers;
use crate::platform_identities::PlatformIdentities;
use forge_hotkey::HotkeyReconciler;
use forge_runtime::{IntegrationFactory, IntegrationObjects};

#[derive(Clone)]
pub struct BuiltinObject {
    pub status: Arc<dyn BuiltinStatus>,
    pub health: Arc<dyn BuiltinHealth>,
    pub content: Arc<dyn BuiltinContent>,
    pub quick: Arc<dyn QuickActions>,
    pub control: Option<Arc<dyn BuiltinControl>>,
    pub collections: Option<Arc<dyn BuiltinCollections>>,
    pub option_source: Option<Arc<dyn BuiltinOptionSource>>,
    pub current_values: Option<Arc<dyn BuiltinCurrentValues>>,
    pub follow: Option<Arc<dyn FollowLookup>>,
    pub ban_list: Option<Arc<dyn BanListSource>>,
}

impl BuiltinObject {
    pub fn of<T>(subject: Arc<T>) -> Self
    where
        T: BuiltinStatus + BuiltinHealth + BuiltinContent + QuickActions + 'static,
    {
        Self {
            status: subject.clone(),
            health: subject.clone(),
            content: subject.clone(),
            quick: subject,
            control: None,
            collections: None,
            option_source: None,
            current_values: None,
            follow: None,
            ban_list: None,
        }
    }

    pub fn with_control(mut self, control: Arc<dyn BuiltinControl>) -> Self {
        self.control = Some(control);
        self
    }

    pub fn with_collections(mut self, collections: Arc<dyn BuiltinCollections>) -> Self {
        self.collections = Some(collections);
        self
    }

    pub fn with_option_source(mut self, option_source: Arc<dyn BuiltinOptionSource>) -> Self {
        self.option_source = Some(option_source);
        self
    }

    pub fn with_current_values(mut self, current_values: Arc<dyn BuiltinCurrentValues>) -> Self {
        self.current_values = Some(current_values);
        self
    }

    pub fn with_follow(mut self, follow: Arc<dyn FollowLookup>) -> Self {
        self.follow = Some(follow);
        self
    }

    pub fn with_ban_list(mut self, ban_list: Arc<dyn BanListSource>) -> Self {
        self.ban_list = Some(ban_list);
        self
    }
}

type BuiltinEntries = HashMap<IntegrationId, BuiltinObject>;

#[derive(Clone)]
pub struct BuiltinRegistry {
    entries: Arc<watch::Sender<Arc<BuiltinEntries>>>,
}

#[derive(Clone)]
pub struct BuiltinRegistryChanges(watch::Receiver<Arc<BuiltinEntries>>);

impl BuiltinRegistryChanges {
    pub async fn changed(&mut self) -> bool {
        self.0.changed().await.is_ok()
    }
}

impl Default for BuiltinRegistry {
    fn default() -> Self {
        Self {
            entries: Arc::new(watch::Sender::new(Arc::new(BuiltinEntries::new()))),
        }
    }
}

impl BuiltinRegistry {
    fn current(&self) -> Arc<BuiltinEntries> {
        Arc::clone(&self.entries.borrow())
    }

    pub fn get(&self, id: &IntegrationId) -> Option<BuiltinObject> {
        self.current().get(id).cloned()
    }

    pub fn snapshot(&self) -> Vec<BuiltinObject> {
        self.current().values().cloned().collect()
    }

    pub fn changes(&self) -> BuiltinRegistryChanges {
        BuiltinRegistryChanges(self.entries.subscribe())
    }

    pub fn install(&self, object: BuiltinObject) {
        let id = object.status.id().clone();
        self.entries.send_modify(|entries| {
            Arc::make_mut(entries).insert(id, object);
        });
    }

    pub fn remove(&self, id: &IntegrationId) {
        self.entries.send_modify(|entries| {
            Arc::make_mut(entries).remove(id);
        });
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

pub struct Integrations {
    pub builtins: BuiltinRegistry,
    pub factories: Vec<Arc<dyn IntegrationFactory<BuiltinObject>>>,
    pub obs_install_seed: ObsInstallSeed,
    pub vtube_install_seed: VTubeInstallSeed,
    pub discord_client: Option<Arc<forge_discord::DiscordClient>>,
    pub midi_sink: Arc<forge_midi::SwitchableMidiSink>,
    pub hotkey_client: Option<Arc<forge_hotkey::HotkeyClient>>,
    pub hotkey_reconciler: Arc<HotkeyReconciler>,
    pub donation_services: DonationServices,
    pub twitch_bot: Option<Arc<forge_platform_twitch::TwitchBotAccount>>,
    pub twitch_sign_in: Option<forge_platform_twitch::TwitchSignIn>,
    pub youtube_sign_in: Option<forge_platform_youtube::YouTubeSignIn>,
    pub kick_sign_in: Option<forge_platform_kick::KickSignIn>,
    pub sign_out_routes: SignOutRoutes,
}

pub trait SeededClient: BuiltinStatus + BuiltinControl + Send + Sync + Sized + 'static {
    type Sink: Send + Sync + 'static;

    fn integration_id() -> IntegrationId;
    fn install_on(sink: &Self::Sink, client: Arc<Self>);
    fn forget_on(_sink: &Self::Sink) {}
    fn builtin_object(self: Arc<Self>) -> BuiltinObject;
}

#[async_trait::async_trait]
pub trait LiveConnection: Send + Sync {
    fn connect_failure(&self) -> Option<MessageRef>;
    async fn disconnect(&self) -> ControlOutcome;
}

#[async_trait::async_trait]
impl<C: SeededClient> LiveConnection for C {
    fn connect_failure(&self) -> Option<MessageRef> {
        BuiltinStatus::connect_failure(self)
    }

    async fn disconnect(&self) -> ControlOutcome {
        BuiltinControl::disconnect(self).await
    }
}

struct LiveClient<C> {
    client: Option<Arc<C>>,
    connection: Arc<dyn LiveConnection>,
}

pub struct InstallSeed<C: SeededClient> {
    sink: Arc<C::Sink>,
    live: LiveSlot<LiveClient<C>>,
    builtins: BuiltinRegistry,
}

pub type ObsInstallSeed = InstallSeed<forge_obs::ObsClient>;
pub type VTubeInstallSeed = InstallSeed<forge_vtube::VTubeClient>;

impl<C: SeededClient> Clone for InstallSeed<C> {
    fn clone(&self) -> Self {
        Self {
            sink: Arc::clone(&self.sink),
            live: self.live.clone(),
            builtins: self.builtins.clone(),
        }
    }
}

impl<C: SeededClient> InstallSeed<C> {
    pub fn new(sink: Arc<C::Sink>, builtins: BuiltinRegistry) -> Self {
        Self {
            sink,
            live: LiveSlot::new(),
            builtins,
        }
    }

    fn forget_object(&self) {
        C::forget_on(&self.sink);
        self.builtins.remove(&C::integration_id());
    }

    pub fn install(&self, client: Arc<C>) {
        C::install_on(&self.sink, Arc::clone(&client));
        self.builtins.install(Arc::clone(&client).builtin_object());
        let connection: Arc<dyn LiveConnection> = client.clone();
        self.live.install(Arc::new(LiveClient {
            client: Some(client),
            connection,
        }));
    }

    pub fn install_connection(&self, connection: Arc<dyn LiveConnection>) {
        self.live.install(Arc::new(LiveClient {
            client: None,
            connection,
        }));
    }

    pub fn clear(&self) {
        self.forget_object();
        self.live.take();
    }

    pub async fn disconnect_live(&self) {
        self.forget_object();
        if let Some(live) = self.live.take()
            && let Err(err) = live.connection.disconnect().await
        {
            tracing::warn!(error = %err, "disconnecting the live client failed");
        }
    }

    pub fn live(&self) -> Option<Arc<C>> {
        self.live.current().and_then(|live| live.client.clone())
    }

    pub fn connect_failure(&self) -> Option<MessageRef> {
        self.live.current()?.connection.connect_failure()
    }
}

impl ObsInstallSeed {
    pub fn stream_output(&self) -> forge_obs::StreamOutputActive {
        self.sink.stream_output()
    }

    pub fn stream_stats(&self) -> forge_obs::ObsStatsWatch {
        self.sink.stream_stats()
    }
}

impl SeededClient for forge_obs::ObsClient {
    type Sink = forge_obs::SwitchableObsSink;

    fn integration_id() -> IntegrationId {
        forge_obs::OBS_INTEGRATION.id
    }

    fn install_on(sink: &Self::Sink, client: Arc<Self>) {
        sink.install(client);
    }

    fn builtin_object(self: Arc<Self>) -> BuiltinObject {
        BuiltinObject::of(self.clone())
            .with_control(self.clone())
            .with_option_source(self)
    }
}

impl SeededClient for forge_vtube::VTubeClient {
    type Sink = forge_vtube::SwitchableVTubeSink;

    fn integration_id() -> IntegrationId {
        forge_vtube::VTUBE_INTEGRATION.id
    }

    fn install_on(sink: &Self::Sink, client: Arc<Self>) {
        sink.install(client);
    }

    fn forget_on(sink: &Self::Sink) {
        sink.clear();
    }

    fn builtin_object(self: Arc<Self>) -> BuiltinObject {
        BuiltinObject::of(self.clone())
            .with_control(self.clone())
            .with_option_source(self)
    }
}

pub fn twitch_builtin_object(
    bundle: Arc<forge_platform_twitch::TwitchIntegrationBundle>,
) -> BuiltinObject {
    BuiltinObject::of(bundle.clone())
        .with_control(bundle.clone())
        .with_collections(bundle.clone())
        .with_option_source(bundle.clone())
        .with_current_values(bundle.clone())
        .with_follow(bundle.clone())
        .with_ban_list(bundle)
}

pub fn youtube_builtin_object(
    bundle: Arc<forge_platform_youtube::YouTubeIntegrationBundle>,
) -> BuiltinObject {
    BuiltinObject::of(bundle.clone())
        .with_control(bundle.clone())
        .with_current_values(bundle.clone())
        .with_follow(bundle.clone())
        .with_ban_list(bundle)
}

pub fn kick_builtin_object(
    bundle: Arc<forge_platform_kick::KickIntegrationBundle>,
) -> BuiltinObject {
    BuiltinObject::of(bundle.clone())
        .with_control(bundle.clone())
        .with_ban_list(bundle)
}

pub struct IntegrationInputs {
    pub endpoints: PlatformEndpoints,
    pub hotkey_main_thread: forge_hotkey::MainThreadLink,
    pub platform_identities: PlatformIdentities,
    pub chat_avatars: ChatAvatars,
    pub chat_emotes: ChatEmotes,
}

pub async fn build_integrations(
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
        chat_avatars,
        chat_emotes,
    } = inputs;
    let endpoints = &endpoints;
    register_platform_triggers(triggers);
    let midi_kinds = kinds_owned_by(triggers, &forge_midi::MIDI_INTEGRATION.id);

    let mut factories: Vec<Arc<dyn IntegrationFactory<BuiltinObject>>> = Vec::new();
    let mut twitch_bot = None;
    let mut twitch_sign_in = None;
    let mut platform_credentials = PlatformCredentialManagers::default();
    if let Some(twitch) = platform_identities.twitch_client_id.and_then(|client_id| {
        wire_twitch(
            sub_actions,
            backend,
            bus,
            endpoints,
            client_id,
            ChatImageBooks {
                avatars: &chat_avatars,
                emotes: &chat_emotes,
            },
        )
    }) {
        twitch_bot = Some(twitch.bot());
        twitch_sign_in = Some(twitch.sign_in());
        platform_credentials.twitch = Some(twitch.credentials_manager());
        factories.push(Arc::new(twitch));
    }
    let builtins = BuiltinRegistry::default();
    let obs = wire_obs(sub_actions, backend, bus, builtins.clone());
    let obs_install_seed = obs.seed();
    let mut sign_out_routes = SignOutRoutes::default();
    sign_out_routes.register(forge_obs::OBS_INTEGRATION.id, obs.sign_out_route());
    factories.push(Arc::new(obs));
    let vtube = wire_vtube(sub_actions, backend, bus, builtins.clone());
    let vtube_install_seed = vtube.seed();
    sign_out_routes.register(forge_vtube::VTUBE_INTEGRATION.id, vtube.sign_out_route());
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
    if let Some(youtube) = platform_identities.youtube.and_then(|identity| {
        wire_youtube(
            sub_actions,
            backend,
            bus,
            endpoints,
            identity,
            &chat_avatars,
        )
    }) {
        youtube_sign_in = Some(youtube.sign_in());
        platform_credentials.youtube = Some(youtube.credentials_manager());
        factories.push(Arc::new(youtube));
    }
    let mut kick_sign_in = None;
    if let Some(kick) = platform_identities.kick.and_then(|identity| {
        wire_kick(
            sub_actions,
            backend,
            bus,
            endpoints,
            identity,
            &chat_avatars,
        )
    }) {
        kick_sign_in = Some(kick.sign_in());
        platform_credentials.kick = Some(kick.credentials_manager());
        factories.push(Arc::new(kick));
    }
    for declaration in declarations() {
        if let Some(platform) = declaration.chat_platform() {
            sign_out_routes.register(
                declaration.id,
                SignOutRoute::platform(platform, platform_credentials.clone(), creds_of(backend)),
            );
        }
    }
    let mut donation_services = DonationServices::default();
    if let Some(donatello) = wire_donatello(backend, endpoints, donations, bus) {
        let service = donatello_service(donatello.provider());
        service
            .restore_poll_interval(backend.settings_repo().as_ref())
            .await;
        donation_services.register(service);
        factories.push(Arc::new(donatello));
    }
    if let Some(monobank) = wire_monobank(backend, endpoints, donations, bus) {
        donation_services.register(DonationService::of(monobank.provider()));
        factories.push(Arc::new(monobank));
    }

    Integrations {
        builtins,
        factories,
        obs_install_seed,
        vtube_install_seed,
        discord_client,
        midi_sink,
        hotkey_client: Some(hotkey_client),
        hotkey_reconciler,
        donation_services,
        twitch_bot,
        twitch_sign_in,
        youtube_sign_in,
        kick_sign_in,
        sign_out_routes,
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

pub(crate) fn register_platform_triggers(triggers: &mut TriggerRegistry) {
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
    if let Err(e) = forge_monobank::register_monobank_triggers(triggers) {
        tracing::error!(error = %e, "monobank trigger registration failed");
    }
}

pub(crate) fn publisher(bus: &Arc<EventBus>) -> Arc<dyn EventPublisher> {
    Arc::clone(bus) as Arc<dyn EventPublisher>
}

pub(crate) fn creds_of(backend: &Arc<dyn DataProvider>) -> Arc<dyn CredentialsRepo> {
    backend.credentials_repo()
}

pub(crate) struct EventRelay {
    task: JoinHandle<()>,
    retired: CancellationToken,
}

impl EventRelay {
    pub(crate) fn spawn(
        bus: Arc<dyn EventPublisher>,
        events: EventSubscription,
        label: &'static str,
    ) -> Self {
        let retired = CancellationToken::new();
        Self {
            task: tokio::spawn(relay_events(bus, events, label, retired.clone())),
            retired,
        }
    }

    pub(crate) async fn drain(mut self, bound: Duration) {
        self.retired.cancel();
        if tokio::time::timeout(bound, &mut self.task).await.is_err() {
            tracing::warn!(?bound, "event relay did not drain in time; aborting it");
        }
    }

    #[cfg(test)]
    pub(crate) fn stalled(held: Arc<()>) -> Self {
        Self {
            task: tokio::spawn(async move {
                let _held = held;
                std::future::pending::<()>().await;
            }),
            retired: CancellationToken::new(),
        }
    }
}

impl Drop for EventRelay {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn relay_events(
    bus: Arc<dyn EventPublisher>,
    mut events: EventSubscription,
    label: &'static str,
    retired: CancellationToken,
) {
    loop {
        let delivery = tokio::select! {
            biased;
            delivery = events.next() => delivery,
            () = retired.cancelled() => break,
        };
        match delivery {
            Delivery::Event(event) => bus.publish(Arc::unwrap_or_clone(event)),
            Delivery::Closed => break,
            Delivery::Skipped(skipped) => {
                tracing::warn!(bridge = label, skipped, "event bridge dropped events");
            }
        }
    }
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
mod tests {
    use super::*;
    use crate::integration_factories::TWITCH_EVENT_RELAY;
    use crate::test_support::{every_registry_entry, yield_until};
    use forge_events::{Event, EventChannel, EventSource};
    use forge_types::MessageCode;

    use forge_platform_core::ConnectionState;
    use forge_runtime::NullEventLogRepo;

    fn test_bus() -> Arc<EventBus> {
        EventBus::new(Arc::new(NullEventLogRepo))
    }

    async fn listening_but_silent_port() -> (tokio::net::TcpListener, u16) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    async fn silent_client(port: u16) -> Arc<forge_obs::ObsClient> {
        let client = forge_obs::ObsClient::connect(
            &forge_obs::ObsEndpoint::parse(&format!("127.0.0.1:{port}"), "").unwrap(),
            Arc::new(forge_events::test_support::NullPublisher),
        )
        .await
        .unwrap();
        Arc::new(client)
    }

    async fn installable_object(port: u16) -> BuiltinObject {
        silent_client(port).await.builtin_object()
    }

    fn wakes_now(changes: &mut BuiltinRegistryChanges) -> Option<bool> {
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        match std::pin::pin!(changes.changed()).poll(&mut context) {
            std::task::Poll::Ready(open) => Some(open),
            std::task::Poll::Pending => None,
        }
    }

    #[tokio::test]
    async fn a_change_follower_wakes_on_install_and_removal_and_coalesces_a_burst() {
        let (_listener, port) = listening_but_silent_port().await;
        let object = installable_object(port).await;
        let id = object.status.id().clone();
        let registry = BuiltinRegistry::default();
        let mut changes = registry.changes();

        let before_any_change = wakes_now(&mut changes);
        registry.install(object.clone());
        registry.install(object);
        let after_burst = (wakes_now(&mut changes), wakes_now(&mut changes));
        registry.remove(&id);
        let after_removal = wakes_now(&mut changes);

        assert_eq!(
            (before_any_change, after_burst, after_removal),
            (None, (Some(true), None), Some(true))
        );
    }

    #[test]
    fn a_change_follower_learns_when_the_registry_is_gone() {
        let registry = BuiltinRegistry::default();
        let mut changes = registry.changes();

        drop(registry);

        assert_eq!(wakes_now(&mut changes), Some(false));
    }

    #[tokio::test]
    async fn a_live_obs_client_is_a_registry_object_until_the_seed_lets_go_of_it() {
        let (_listener, port) = listening_but_silent_port().await;
        let registry = BuiltinRegistry::default();
        let seed = ObsInstallSeed::new(forge_obs::SwitchableObsSink::new(), registry.clone());
        let id = forge_obs::OBS_INTEGRATION.id;
        let serves_options = |registry: &BuiltinRegistry| {
            registry
                .get(&id)
                .is_some_and(|object| object.option_source.is_some())
        };

        let before = serves_options(&registry);
        seed.install(silent_client(port).await);
        let installed = serves_options(&registry);
        seed.clear();
        let cleared = serves_options(&registry);
        seed.install(silent_client(port).await);
        seed.disconnect_live().await;

        assert_eq!(
            (before, installed, cleared, serves_options(&registry)),
            (false, true, false, false)
        );
    }

    #[derive(Default)]
    struct RecordedConnection {
        disconnects: std::sync::atomic::AtomicUsize,
    }

    #[async_trait::async_trait]
    impl LiveConnection for RecordedConnection {
        fn connect_failure(&self) -> Option<MessageRef> {
            Some(MessageRef::new(MessageCode::from_static("obs", "refused")))
        }

        async fn disconnect(&self) -> ControlOutcome {
            self.disconnects
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn retiring_an_installed_connection_disconnects_it_once_and_forgets_its_failure() {
        let seed = ObsInstallSeed::new(
            forge_obs::SwitchableObsSink::new(),
            BuiltinRegistry::default(),
        );
        let connection = Arc::new(RecordedConnection::default());
        seed.install_connection(connection.clone());
        let before = seed.connect_failure().is_some();

        seed.disconnect_live().await;
        seed.disconnect_live().await;

        assert_eq!(
            (
                before,
                seed.connect_failure().is_some(),
                connection
                    .disconnects
                    .load(std::sync::atomic::Ordering::SeqCst)
            ),
            (true, false, 1)
        );
    }

    #[tokio::test]
    async fn retiring_the_live_client_empties_the_slot_and_leaves_it_disconnected() {
        let (_listener, port) = listening_but_silent_port().await;
        let seed = ObsInstallSeed::new(
            forge_obs::SwitchableObsSink::new(),
            BuiltinRegistry::default(),
        );
        let client = Arc::new(
            forge_obs::ObsClient::connect(
                &forge_obs::ObsEndpoint::parse(&format!("127.0.0.1:{port}"), "").unwrap(),
                Arc::new(forge_events::test_support::NullPublisher),
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
        let seed = ObsInstallSeed::new(
            forge_obs::SwitchableObsSink::new(),
            BuiltinRegistry::default(),
        );
        assert!(seed.connect_failure().is_none());
        let port = closed_port().await;
        let client = Arc::new(
            forge_obs::ObsClient::connect(
                &forge_obs::ObsEndpoint::parse(&format!("127.0.0.1:{port}"), "").unwrap(),
                Arc::new(forge_events::test_support::NullPublisher),
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
        let seed = VTubeInstallSeed::new(
            forge_vtube::SwitchableVTubeSink::new(),
            BuiltinRegistry::default(),
        );
        assert!(seed.connect_failure().is_none());
        let port = closed_port().await;
        let (backend, _writes) = crate::test_support::test_backend();
        let client = Arc::new(forge_vtube::VTubeClient::connect(
            forge_vtube::VTubeConfig {
                endpoint: format!("ws://127.0.0.1:{port}/"),
            },
            Arc::new(forge_events::test_support::NullPublisher),
            backend.credentials_repo(),
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
            forge_monobank::MONOBANK_INTEGRATION.id,
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
            .filter(|id| !matches!(id.as_str(), "hotkey" | "monobank"))
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
        let bridge = tokio::spawn(relay_events(
            publisher(&bus),
            platform.subscribe(),
            "twitch",
            CancellationToken::new(),
        ));

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
            tokio::spawn(relay_events(
                publisher(&bus),
                events,
                "twitch",
                CancellationToken::new(),
            )),
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

    struct AnsweringTitle;

    #[async_trait::async_trait]
    impl BuiltinCurrentValues for AnsweringTitle {
        async fn read_current(
            &self,
            _field_keys: &[String],
        ) -> std::collections::BTreeMap<String, forge_platform_core::QuickActionFieldValue>
        {
            std::collections::BTreeMap::from([(
                "title".to_owned(),
                forge_platform_core::QuickActionFieldValue::Text("Live now".to_owned()),
            )])
        }
    }

    #[tokio::test]
    async fn an_object_reads_current_values_only_through_the_reader_it_was_given() {
        let (_listener, port) = listening_but_silent_port().await;
        let object = installable_object(port).await;
        let unset = object.current_values.is_none();

        let reader = object
            .with_current_values(Arc::new(AnsweringTitle))
            .current_values
            .expect("the reader fills the slot");
        let answer = reader.read_current(&["title".to_owned()]).await;

        assert_eq!(
            (unset, answer.get("title")),
            (
                true,
                Some(&forge_platform_core::QuickActionFieldValue::Text(
                    "Live now".to_owned()
                ))
            )
        );
    }
}
