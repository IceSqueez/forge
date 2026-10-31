use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_events::Event;
use forge_net::{Gateway, RateLimiter, TokenBucketRateLimiter};
use forge_platform_core::{ChatPlatform, EndpointSurface, PlatformEndpoints, SectionIcon};
use forge_registry::SubActionRegistry;
use forge_runtime::{
    DEFAULT_TEARDOWN_GRACE, EventBus, IntegrationFactory, IntegrationStartError,
    RunningIntegration, TaskGroup, spawn_chat_send_router,
};
use forge_storage::{
    CredentialsRepo, DataProvider, Flag, Setting, SettingsRepo, SoundboardClipsRepo, StorageError,
    TriggerInstanceRepo, has_credentials_for, read_setting, read_setting_or_default,
};
use forge_types::{IntegrationId, PlatformId};
use tokio::sync::mpsc;

use crate::event_loss::{KICK_EVENT_RELAY, TWITCH_EVENT_RELAY, YOUTUBE_EVENT_RELAY};
use crate::integration_start_errors::{
    hotkey_start_error, kick_start_error, midi_start_error, twitch_start_error, youtube_start_error,
};
use crate::integrations::{
    BuiltinObject, ObsInstallSeed, VTubeInstallSeed, creds_of, kick_builtin_object, publisher,
    spawn_connect, spawn_event_bridge, twitch_builtin_object, youtube_builtin_object,
};
use crate::obs_credentials_form::{OBS_AUTO_RECONNECT, OBS_CONNECT_ON_LAUNCH};
use crate::platform_identities::OAuthClientIdentity;
use crate::vtube_connect_form::{VTUBE_AUTO_RECONNECT, VTUBE_CONNECT_ON_LAUNCH};
use forge_hotkey::HotkeyReconciler;
use forge_hotkey::bound_hotkey_combos;
use forge_hotkey::clip_bindings_of;

const CONNECT_GUARD: Duration = Duration::from_secs(5);
const KICK_POLLER_QUEUE_CAPACITY: usize = 256;

fn idle_with_tasks(tasks: TaskGroup) -> RunningIntegration<BuiltinObject> {
    RunningIntegration::idle().with_teardown(Box::pin(async move { tasks.abort_all() }))
}

pub(crate) struct TwitchFactory {
    bus: Arc<EventBus>,
    assembly: forge_platform_twitch::TwitchAssembly,
}

pub(crate) fn wire_twitch(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    endpoints: &PlatformEndpoints,
    client_id: String,
) -> Option<TwitchFactory> {
    let assembly = forge_platform_twitch::TwitchAssembly::new(
        sub_actions,
        creds_of(backend),
        &publisher(bus),
        endpoints,
        client_id,
    )?;
    Some(TwitchFactory {
        bus: Arc::clone(bus),
        assembly,
    })
}

impl TwitchFactory {
    pub(crate) fn bot(&self) -> Arc<forge_platform_twitch::TwitchBotAccount> {
        self.assembly.bot()
    }

    pub(crate) fn sign_in(&self) -> forge_platform_twitch::TwitchSignIn {
        self.assembly.sign_in()
    }

    pub(crate) fn credentials_manager(
        &self,
    ) -> Arc<forge_platform_twitch::TwitchCredentialsManager> {
        self.assembly.credentials_manager()
    }
}

#[async_trait]
impl IntegrationFactory<BuiltinObject> for TwitchFactory {
    fn id(&self) -> IntegrationId {
        self.assembly.id()
    }

    fn teardown_grace(&self) -> Duration {
        forge_platform_twitch::SHUTDOWN_GRACE
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        self.assembly.is_configured().await
    }

    async fn start(&self) -> Result<RunningIntegration<BuiltinObject>, IntegrationStartError> {
        let session = self
            .assembly
            .open_session()
            .await
            .map_err(|e| twitch_start_error(&e))?;
        let chat_platform = session.chat_platform();
        let event_bridge = spawn_event_bridge(
            publisher(&self.bus),
            chat_platform
                .events()
                .recording_lag(self.bus.relay_lag(TWITCH_EVENT_RELAY)),
            "twitch",
        );
        let mut tasks = spawn_chat_send_router(
            Arc::clone(&self.bus),
            Arc::clone(&chat_platform),
            PlatformId::Twitch,
        );
        tasks.track(event_bridge);
        let bot = self.assembly.bot();
        tasks.track(tokio::spawn(async move { bot.sync_status().await }));
        let Some(bundle) = session.into_bundle() else {
            return Ok(idle_with_tasks(tasks));
        };
        tasks.track(spawn_connect(chat_platform, "twitch"));
        let viewers = bundle.viewer_source();
        let object = twitch_builtin_object(Arc::clone(&bundle));
        let teardown = Box::pin(async move {
            tasks.abort_all();
            bundle.shutdown().await;
        });
        Ok(RunningIntegration::idle()
            .with_object(object)
            .with_viewers(PlatformId::Twitch, viewers)
            .with_teardown(teardown))
    }
}

pub(crate) struct YoutubeFactory {
    assembly: forge_platform_youtube::YoutubeAssembly,
    bus: Arc<EventBus>,
    creds: Arc<dyn CredentialsRepo>,
}

pub(crate) fn wire_youtube(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    endpoints: &PlatformEndpoints,
    identity: OAuthClientIdentity,
) -> Option<YoutubeFactory> {
    let creds = creds_of(backend);
    let assembly = forge_platform_youtube::YoutubeAssembly::new(
        endpoints,
        publisher(bus),
        Arc::clone(&creds),
        backend.ban_ledger_repo(),
        (identity.client_id, identity.client_secret),
    )?;
    assembly.register_sub_actions(sub_actions);
    Some(YoutubeFactory {
        assembly,
        bus: Arc::clone(bus),
        creds,
    })
}

impl YoutubeFactory {
    pub(crate) fn sign_in(&self) -> forge_platform_youtube::YoutubeSignIn {
        self.assembly.sign_in()
    }

    pub(crate) fn credentials_manager(
        &self,
    ) -> Arc<forge_platform_youtube::YoutubeCredentialsManager> {
        self.assembly.credentials_manager()
    }
}

#[async_trait]
impl IntegrationFactory<BuiltinObject> for YoutubeFactory {
    fn id(&self) -> IntegrationId {
        forge_platform_youtube::YOUTUBE_INTEGRATION.id
    }

    fn teardown_grace(&self) -> Duration {
        DEFAULT_TEARDOWN_GRACE
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        has_credentials_for(self.creds.as_ref(), &self.id()).await
    }

    async fn start(&self) -> Result<RunningIntegration<BuiltinObject>, IntegrationStartError> {
        let Some(session) = self
            .assembly
            .start()
            .await
            .map_err(|e| youtube_start_error(&e))?
        else {
            return Ok(RunningIntegration::idle());
        };
        let bundle = session.bundle;
        let chat_platform: Arc<dyn ChatPlatform> = session.platform;

        let mut tasks = TaskGroup::default();
        tasks.track(spawn_event_bridge(
            publisher(&self.bus),
            chat_platform
                .events()
                .recording_lag(self.bus.relay_lag(YOUTUBE_EVENT_RELAY)),
            "youtube",
        ));
        tasks.track(spawn_connect(Arc::clone(&chat_platform), "youtube"));
        tasks.absorb(spawn_chat_send_router(
            Arc::clone(&self.bus),
            chat_platform,
            PlatformId::YouTube,
        ));

        let viewers = bundle.viewer_source();
        let object = youtube_builtin_object(Arc::clone(&bundle));
        let teardown = Box::pin(async move {
            tasks.abort_all();
            bundle.shutdown().await;
        });
        Ok(RunningIntegration::idle()
            .with_object(object)
            .with_viewers(PlatformId::YouTube, viewers)
            .with_teardown(teardown))
    }
}

pub(crate) struct KickFactory {
    bus: Arc<EventBus>,
    creds: Arc<dyn CredentialsRepo>,
    manager: Arc<forge_platform_kick::KickCredentialsManager>,
    rate_limiter: Arc<dyn RateLimiter>,
    platform: Arc<forge_platform_kick::KickPlatform>,
    channel: Arc<forge_platform_kick::KickChannel>,
    rewards: Arc<forge_platform_kick::KickRewards>,
    sign_in: forge_platform_kick::KickSignIn,
}

pub(crate) fn wire_kick(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    endpoints: &PlatformEndpoints,
    identity: OAuthClientIdentity,
) -> Option<KickFactory> {
    let oauth = forge_platform_kick::KickOAuthClient::new(
        endpoints,
        kick_oauth_gateway(endpoints)?,
        identity.client_id,
        identity.client_secret,
    );
    let rate_limiter: Arc<dyn RateLimiter> = Arc::new(TokenBucketRateLimiter::new(
        forge_platform_kick::KICK_API_BUDGET_CAPACITY,
        forge_platform_kick::KICK_API_BUDGET_WINDOW,
    ));
    let api =
        forge_platform_kick::kick_api_gateway(endpoints, Arc::clone(&rate_limiter), publisher(bus))
            .inspect_err(|e| tracing::error!(error = %e, "kick api gateway build failed"))
            .ok()?;
    let creds = creds_of(backend);
    let manager = Arc::new(forge_platform_kick::KickCredentialsManager::new(
        Arc::clone(&creds),
        oauth,
        api.clone(),
    ));
    let sign_in = forge_platform_kick::KickSignIn::new(Arc::clone(&manager));
    let channel_info = Arc::new(forge_platform_kick::ChannelInfoFetcher::new(
        kick_channel_info_gateway(endpoints, bus)?,
    ));
    let ban_ledger = backend.ban_ledger_repo();
    let sender = Arc::new(forge_platform_kick::KickSendChat::new(api.clone()));
    let platform = Arc::new(forge_platform_kick::KickPlatform::new(
        endpoints,
        Arc::clone(&manager),
        Arc::clone(&sender),
        channel_info,
        Arc::clone(&ban_ledger),
    ));
    let moderation = Arc::new(forge_platform_kick::KickModeration::new(
        api.clone(),
        ban_ledger,
    ));
    let channel = Arc::new(forge_platform_kick::KickChannel::new(api.clone()));
    let rewards = Arc::new(forge_platform_kick::KickRewards::new(api.clone()));
    let categories = Arc::new(forge_platform_kick::KickCategories::new(api));

    let manager_for_sub_actions = Arc::clone(&manager);
    let manager_for_broadcaster = Arc::clone(&manager);
    if let Err(e) = forge_platform_kick::register_kick_sub_actions(
        sub_actions,
        forge_platform_kick::KickSubActionDeps {
            client: sender,
            token_source: Arc::new(move || {
                let manager = Arc::clone(&manager_for_sub_actions);
                Box::pin(async move { manager.get_valid_access_token().await })
            }),
            broadcaster_id_source: Arc::new(move || {
                let manager = Arc::clone(&manager_for_broadcaster);
                Box::pin(async move { manager.user_id().await })
            }),
            moderation,
            channel: Arc::clone(&channel),
            rewards: Arc::clone(&rewards),
            categories,
        },
    ) {
        tracing::error!(error = %e, "kick sub-action registration failed");
    }
    Some(KickFactory {
        bus: Arc::clone(bus),
        creds,
        manager,
        rate_limiter,
        platform,
        channel,
        rewards,
        sign_in,
    })
}

fn kick_oauth_gateway(endpoints: &PlatformEndpoints) -> Option<Gateway> {
    let limiter: Arc<dyn RateLimiter> = Arc::new(TokenBucketRateLimiter::new(
        forge_platform_kick::KICK_OAUTH_BUDGET_CAPACITY,
        forge_platform_kick::KICK_OAUTH_BUDGET_WINDOW,
    ));
    forge_platform_kick::kick_oauth_gateway(endpoints, limiter)
        .inspect_err(|e| tracing::error!(error = %e, "kick oauth gateway build failed"))
        .ok()
}

impl KickFactory {
    pub(crate) fn sign_in(&self) -> forge_platform_kick::KickSignIn {
        self.sign_in.clone()
    }

    pub(crate) fn credentials_manager(&self) -> Arc<forge_platform_kick::KickCredentialsManager> {
        Arc::clone(&self.manager)
    }
}

fn kick_channel_info_gateway(
    endpoints: &PlatformEndpoints,
    bus: &Arc<EventBus>,
) -> Option<Gateway> {
    let limiter: Arc<dyn RateLimiter> = Arc::new(TokenBucketRateLimiter::new(
        forge_platform_kick::KICK_CHANNEL_INFO_BUDGET_CAPACITY,
        forge_platform_kick::KICK_CHANNEL_INFO_BUDGET_WINDOW,
    ));
    forge_platform_kick::kick_channel_info_gateway(endpoints, limiter, publisher(bus))
        .inspect_err(|e| tracing::error!(error = %e, "kick channel info gateway build failed"))
        .ok()
}

#[async_trait]
impl IntegrationFactory<BuiltinObject> for KickFactory {
    fn id(&self) -> IntegrationId {
        forge_platform_kick::KICK_INTEGRATION.id
    }

    fn teardown_grace(&self) -> Duration {
        DEFAULT_TEARDOWN_GRACE
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        has_credentials_for(self.creds.as_ref(), &self.id()).await
    }

    async fn start(&self) -> Result<RunningIntegration<BuiltinObject>, IntegrationStartError> {
        let chat_platform: Arc<dyn ChatPlatform> = Arc::clone(&self.platform) as _;
        let mut tasks = spawn_chat_send_router(
            Arc::clone(&self.bus),
            Arc::clone(&chat_platform),
            PlatformId::Kick,
        );
        let stored = match self.manager.load().await {
            Ok(Some(stored)) => stored,
            Ok(None) => return Ok(idle_with_tasks(tasks)),
            Err(e) => {
                tasks.abort_all();
                return Err(kick_start_error(&e));
            }
        };

        let bus = publisher(&self.bus);
        tasks.track(spawn_event_bridge(
            Arc::clone(&bus),
            chat_platform
                .events()
                .recording_lag(self.bus.relay_lag(KICK_EVENT_RELAY)),
            "kick",
        ));
        tasks.track(spawn_connect(chat_platform, "kick"));

        let (poller_tx, mut poller_rx) = mpsc::channel::<Event>(KICK_POLLER_QUEUE_CAPACITY);
        tasks.track(tokio::spawn(async move {
            while let Some(event) = poller_rx.recv().await {
                bus.publish(event);
            }
        }));
        let poller_manager = Arc::clone(&self.manager);
        let (viewer_source, poller) = forge_platform_kick::spawn_kick_poller(
            Arc::clone(&self.channel),
            Arc::clone(&self.rewards),
            Arc::new(move || {
                let manager = Arc::clone(&poller_manager);
                Box::pin(async move { manager.get_valid_access_token().await })
            }),
            poller_tx,
        );
        let (bundle, _health_tx) = forge_platform_kick::KickIntegrationBundle::new(
            stored.username,
            stored.user_id,
            Arc::clone(&self.platform),
            Arc::clone(&self.manager),
            Arc::clone(&self.rate_limiter),
            &viewer_source,
        );

        let object = kick_builtin_object(Arc::clone(&bundle));
        let teardown = Box::pin(async move {
            poller.stop();
            tasks.abort_all();
            bundle.shutdown().await;
        });
        Ok(RunningIntegration::idle()
            .with_object(object)
            .with_viewers(PlatformId::Kick, Box::new(viewer_source))
            .with_teardown(teardown))
    }
}

pub(crate) struct ObsFactory {
    bus: Arc<EventBus>,
    creds: Arc<dyn CredentialsRepo>,
    settings: Arc<dyn SettingsRepo>,
    seed: ObsInstallSeed,
}

pub(crate) fn wire_obs(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> ObsFactory {
    let sink = forge_obs::SwitchableObsSink::new();
    if let Err(e) = forge_obs::register_obs_sub_actions(
        sub_actions,
        Arc::clone(&sink) as Arc<dyn forge_obs::ObsSink>,
    ) {
        tracing::error!(error = %e, "obs sub-action registration failed");
    }
    ObsFactory {
        bus: Arc::clone(bus),
        creds: creds_of(backend),
        settings: Arc::clone(backend) as Arc<dyn SettingsRepo>,
        seed: ObsInstallSeed::new(sink),
    }
}

impl ObsFactory {
    pub(crate) fn seed(&self) -> ObsInstallSeed {
        self.seed.clone()
    }
}

async fn connects_on_launch(settings: &dyn SettingsRepo, setting: &Setting<Flag>) -> bool {
    read_setting(settings, setting)
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(
                key = setting.name(),
                %error,
                "connect on launch cannot be read; staying disconnected"
            );
            false
        })
}

#[async_trait]
impl IntegrationFactory<BuiltinObject> for ObsFactory {
    fn id(&self) -> IntegrationId {
        forge_obs::OBS_INTEGRATION.id
    }

    fn teardown_grace(&self) -> Duration {
        DEFAULT_TEARDOWN_GRACE
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        has_credentials_for(self.creds.as_ref(), &self.id()).await
    }

    async fn start(&self) -> Result<RunningIntegration<BuiltinObject>, IntegrationStartError> {
        let seed = self.seed.clone();
        let running = RunningIntegration::idle().with_teardown(Box::pin({
            let seed = seed.clone();
            async move { seed.disconnect_live().await }
        }));
        if seed.live().is_some()
            || !connects_on_launch(self.settings.as_ref(), &OBS_CONNECT_ON_LAUNCH).await
        {
            return Ok(running);
        }
        let connect =
            forge_obs::credentials::load_and_connect(self.creds.as_ref(), publisher(&self.bus));
        match tokio::time::timeout(CONNECT_GUARD, connect).await {
            Ok(Ok(client)) => {
                client.set_auto_reconnect(
                    read_setting_or_default(self.settings.as_ref(), &OBS_AUTO_RECONNECT).await,
                );
                seed.install(client);
            }
            Ok(Err(error)) => tracing::warn!(%error, "obs connect was not started"),
            Err(_) => tracing::warn!(timeout = ?CONNECT_GUARD, "obs connect timed out"),
        }
        Ok(running)
    }
}

pub(crate) struct VTubeFactory {
    bus: Arc<EventBus>,
    creds: Arc<dyn CredentialsRepo>,
    settings: Arc<dyn SettingsRepo>,
    seed: VTubeInstallSeed,
}

pub(crate) fn wire_vtube(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) -> VTubeFactory {
    let sink = forge_vtube::SwitchableVTubeSink::new();
    if let Err(e) = forge_vtube::register_vtube_sub_actions(
        sub_actions,
        Arc::clone(&sink) as Arc<dyn forge_vtube::VTubeSink>,
    ) {
        tracing::error!(error = %e, "vtube sub-action registration failed");
    }
    VTubeFactory {
        bus: Arc::clone(bus),
        creds: creds_of(backend),
        settings: Arc::clone(backend) as Arc<dyn SettingsRepo>,
        seed: VTubeInstallSeed::new(sink),
    }
}

impl VTubeFactory {
    pub(crate) fn seed(&self) -> VTubeInstallSeed {
        self.seed.clone()
    }
}

#[async_trait]
impl IntegrationFactory<BuiltinObject> for VTubeFactory {
    fn id(&self) -> IntegrationId {
        forge_vtube::VTUBE_INTEGRATION.id
    }

    fn teardown_grace(&self) -> Duration {
        DEFAULT_TEARDOWN_GRACE
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        has_credentials_for(self.creds.as_ref(), &self.id()).await
    }

    async fn start(&self) -> Result<RunningIntegration<BuiltinObject>, IntegrationStartError> {
        let seed = self.seed.clone();
        let running = RunningIntegration::idle().with_teardown(Box::pin({
            let seed = seed.clone();
            async move { seed.disconnect_live().await }
        }));
        if seed.live().is_some()
            || !connects_on_launch(self.settings.as_ref(), &VTUBE_CONNECT_ON_LAUNCH).await
        {
            return Ok(running);
        }
        let connect = forge_vtube::credentials::load_and_connect(
            self.creds.as_ref(),
            publisher(&self.bus),
            Arc::clone(&self.creds),
        );
        match tokio::time::timeout(CONNECT_GUARD, connect).await {
            Ok(Ok(client)) => {
                client.set_auto_reconnect(
                    read_setting_or_default(self.settings.as_ref(), &VTUBE_AUTO_RECONNECT).await,
                );
                seed.install(client);
            }
            Ok(Err(error)) => tracing::warn!(%error, "vtube connect was not started"),
            Err(_) => tracing::warn!(timeout = ?CONNECT_GUARD, "vtube connect timed out"),
        }
        Ok(running)
    }
}

pub(crate) struct DiscordFactory {
    creds: Arc<dyn CredentialsRepo>,
    client: Arc<forge_discord::DiscordClient>,
}

pub(crate) fn wire_discord(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    endpoints: &PlatformEndpoints,
) -> Option<DiscordFactory> {
    let client = forge_discord::DiscordClient::new(
        endpoints.base_url(EndpointSurface::DiscordApi),
        publisher(bus),
        creds_of(backend),
    )
    .inspect_err(|e| tracing::error!(error = %e, "discord gateway build failed"))
    .ok()?;
    if let Err(e) = forge_discord::register_discord_sub_actions(sub_actions, Arc::clone(&client)) {
        tracing::error!(error = %e, "discord sub-action registration failed");
    }
    Some(DiscordFactory {
        creds: creds_of(backend),
        client,
    })
}

impl DiscordFactory {
    pub(crate) fn client(&self) -> Arc<forge_discord::DiscordClient> {
        Arc::clone(&self.client)
    }
}

#[async_trait]
impl IntegrationFactory<BuiltinObject> for DiscordFactory {
    fn id(&self) -> IntegrationId {
        forge_discord::DISCORD_INTEGRATION.id
    }

    fn teardown_grace(&self) -> Duration {
        DEFAULT_TEARDOWN_GRACE
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        has_credentials_for(self.creds.as_ref(), &self.id()).await
    }

    async fn start(&self) -> Result<RunningIntegration<BuiltinObject>, IntegrationStartError> {
        let client = Arc::clone(&self.client);
        Ok(RunningIntegration::idle().with_object(BuiltinObject {
            icon: SectionIcon::new("brand-discord"),
            status: client.clone(),
            health: client.clone(),
            content: client.clone(),
            quick: client,
            control: None,
            collections: None,
            option_source: None,
            obs_client: None,
            vtube_client: None,
            follow: None,
            ban_list: None,
        }))
    }
}

pub(crate) struct MidiFactory {
    bus: Arc<EventBus>,
    triggers: Arc<dyn TriggerInstanceRepo>,
    owned_kinds: Vec<String>,
    sink: Arc<forge_midi::SwitchableMidiSink>,
}

pub(crate) fn wire_midi(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    owned_kinds: Vec<String>,
) -> MidiFactory {
    let sink = forge_midi::SwitchableMidiSink::new();
    if let Err(e) = forge_midi::register_midi_sub_actions(
        sub_actions,
        Arc::clone(&sink) as Arc<dyn forge_midi::MidiSink>,
    ) {
        tracing::error!(error = %e, "midi sub-action registration failed");
    }
    MidiFactory {
        bus: Arc::clone(bus),
        triggers: backend.trigger_instance_repo(),
        owned_kinds,
        sink,
    }
}

impl MidiFactory {
    pub(crate) fn sink(&self) -> Arc<forge_midi::SwitchableMidiSink> {
        Arc::clone(&self.sink)
    }
}

#[async_trait]
impl IntegrationFactory<BuiltinObject> for MidiFactory {
    fn id(&self) -> IntegrationId {
        forge_midi::MIDI_INTEGRATION.id
    }

    fn teardown_grace(&self) -> Duration {
        DEFAULT_TEARDOWN_GRACE
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        Ok(self
            .triggers
            .list_user_defined()
            .await?
            .iter()
            .any(|instance| self.owned_kinds.contains(&instance.kind_id)))
    }

    async fn start(&self) -> Result<RunningIntegration<BuiltinObject>, IntegrationStartError> {
        let client = forge_midi::MidiClient::start_with_midir(
            forge_midi::MidiConfig::default(),
            publisher(&self.bus),
        )
        .map_err(|e| midi_start_error(&e))?;
        self.sink.install(Arc::clone(&client));
        let object = BuiltinObject {
            icon: SectionIcon::new("piano"),
            status: client.clone(),
            health: client.clone(),
            content: client.clone(),
            quick: client.clone(),
            control: Some(client),
            collections: None,
            option_source: None,
            obs_client: None,
            vtube_client: None,
            follow: None,
            ban_list: None,
        };
        let sink = Arc::clone(&self.sink);
        let teardown = Box::pin(async move {
            if let Some(client) = sink.take()
                && let Err(e) = client.shutdown().await
            {
                tracing::warn!(error = %e, "midi engine did not acknowledge shutdown");
            }
        });
        Ok(RunningIntegration::idle()
            .with_object(object)
            .with_teardown(teardown))
    }
}

pub(crate) struct HotkeyFactory {
    client: Arc<forge_hotkey::HotkeyClient>,
    triggers: Arc<dyn TriggerInstanceRepo>,
    clips: Arc<dyn SoundboardClipsRepo>,
}

pub(crate) async fn wire_hotkey(
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
    main_thread: forge_hotkey::MainThreadLink,
) -> (HotkeyFactory, Arc<HotkeyReconciler>) {
    let settings = Arc::clone(backend) as Arc<dyn SettingsRepo>;
    let config = forge_hotkey::HotkeyConfig {
        hold_ceiling_secs: forge_hotkey::load_hold_ceiling(settings.as_ref()).await,
        ..forge_hotkey::HotkeyConfig::default()
    };
    let client = forge_hotkey::HotkeyClient::new(config, publisher(bus), main_thread).await;
    if let Err(e) = client.disable().await {
        tracing::error!(error = %e, "hotkey engine could not be held idle at boot");
    }
    let reconciler = HotkeyReconciler::new(
        Arc::clone(&client),
        backend.trigger_instance_repo(),
        backend.soundboard_clips_repo(),
    );
    reconciler.reconcile().await;
    let factory = HotkeyFactory {
        client,
        triggers: backend.trigger_instance_repo(),
        clips: backend.soundboard_clips_repo(),
    };
    (factory, reconciler)
}

impl HotkeyFactory {
    pub(crate) fn client(&self) -> Arc<forge_hotkey::HotkeyClient> {
        Arc::clone(&self.client)
    }
}

#[async_trait]
impl IntegrationFactory<BuiltinObject> for HotkeyFactory {
    fn id(&self) -> IntegrationId {
        forge_hotkey::HOTKEY_INTEGRATION.id
    }

    fn teardown_grace(&self) -> Duration {
        DEFAULT_TEARDOWN_GRACE
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        let instances = self.triggers.list_all().await?;
        if !bound_hotkey_combos(&instances).is_empty() {
            return Ok(true);
        }
        Ok(!clip_bindings_of(&self.clips.list().await?).is_empty())
    }

    async fn start(&self) -> Result<RunningIntegration<BuiltinObject>, IntegrationStartError> {
        let failures = self
            .client
            .enable()
            .await
            .map_err(|e| hotkey_start_error(&e))?;
        if !failures.is_empty() {
            tracing::warn!(
                failed = failures.len(),
                "some hotkey bindings could not be claimed"
            );
        }
        let client = Arc::clone(&self.client);
        let object = BuiltinObject {
            icon: SectionIcon::new("keyboard"),
            status: client.clone(),
            health: client.clone(),
            content: client.clone(),
            quick: client.clone(),
            control: Some(client.clone()),
            collections: None,
            option_source: None,
            obs_client: None,
            vtube_client: None,
            follow: None,
            ban_list: None,
        };
        let teardown = Box::pin(async move {
            if let Err(e) = client.disable().await {
                tracing::warn!(error = %e, "hotkey engine did not release its bindings");
            }
        });
        Ok(RunningIntegration::idle()
            .with_object(object)
            .with_teardown(teardown))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_events::EventPublisher;
    use forge_hotkey::HotkeyConfig;
    use forge_hotkey::testing::test_client;
    use forge_hotkey::vocabulary::event_kinds;
    use forge_storage::MockSoundboardClipsRepo;
    use forge_storage::MockTriggerInstanceRepo;
    use forge_storage::test_support::MemorySettings;
    use forge_types::{PermissionRung, PlatformScope, TriggerInstance, TriggerInstanceId, Variant};

    use super::*;
    use crate::start_failure::StartRemedy;

    struct SilentPublisher;

    impl EventPublisher for SilentPublisher {
        fn publish(&self, _: Event) {}
    }

    fn hotkey_factory(instances: Vec<TriggerInstance>) -> HotkeyFactory {
        let mut triggers = MockTriggerInstanceRepo::new();
        triggers
            .expect_list_all()
            .returning(move || Ok(instances.clone()));
        let mut clips = MockSoundboardClipsRepo::new();
        clips.expect_list().returning(|| Ok(Vec::new()));
        let (client, _) = test_client(HotkeyConfig::default(), Arc::new(SilentPublisher));
        HotkeyFactory {
            client,
            triggers: Arc::new(triggers),
            clips: Arc::new(clips),
        }
    }

    fn disabled_binding() -> TriggerInstance {
        TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: event_kinds::GLOBAL_PRESSED.to_owned(),
            name: "F1".to_owned(),
            overrides: BTreeMap::from([(
                forge_hotkey::COMBO_FIELD.to_owned(),
                Variant::String("F1".to_owned()),
            )]),
            enabled: false,
            user_defined: true,
            platform_scope: PlatformScope::default(),
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        }
    }

    #[tokio::test]
    async fn hotkeys_stay_configured_when_every_binding_is_disabled() {
        let factory = hotkey_factory(vec![disabled_binding()]);

        assert!(factory.is_configured().await.unwrap());
    }

    #[tokio::test]
    async fn hotkeys_are_not_configured_without_any_binding() {
        let factory = hotkey_factory(Vec::new());

        assert!(!factory.is_configured().await.unwrap());
    }

    #[tokio::test]
    async fn an_unreadable_twitch_sign_in_fails_the_start_and_asks_to_sign_in_again() {
        let mut creds = forge_storage::credentials::MockCredentialsRepo::new();
        creds
            .expect_load()
            .returning(|_| Err(StorageError::Decryption));
        let bus = EventBus::new(Arc::new(crate::test_support::StubEventLog));
        let assembly = forge_platform_twitch::TwitchAssembly::new(
            &mut SubActionRegistry::new(),
            Arc::new(creds),
            &publisher(&bus),
            &PlatformEndpoints::default(),
            "factory-test-client".to_owned(),
        )
        .unwrap();
        let factory = TwitchFactory { bus, assembly };

        let Err(error) = factory.start().await else {
            panic!("an unreadable sign-in must not start as signed out");
        };

        assert_eq!(
            (error.message().code().clone(), StartRemedy::of(&error)),
            (
                forge_platform_twitch::vocabulary::messages::STORED_SIGN_IN_UNREADABLE,
                StartRemedy::SignIn
            )
        );
    }

    #[tokio::test]
    async fn connecting_on_launch_stays_off_when_its_switch_cannot_be_read() {
        for setting in [&OBS_CONNECT_ON_LAUNCH, &VTUBE_CONNECT_ON_LAUNCH] {
            let settings = MemorySettings::new();
            settings.seed(setting, &true).unwrap();
            let stored = connects_on_launch(&settings, setting).await;

            settings.fail_reads_for(setting.name());
            let unreadable = connects_on_launch(&settings, setting).await;

            assert_eq!((stored, unreadable), (true, false), "{}", setting.name());
        }
    }
}
