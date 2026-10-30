use std::sync::Arc;

use forge_events::EventPublisher;
use forge_net::{Gateway, RateLimiter, TokenBucketRateLimiter};
use forge_platform_core::{ChatPlatform, PlatformEndpoints};
use forge_registry::SubActionRegistry;
use forge_storage::{CredentialsRepo, StorageError, has_credentials_for};
use forge_types::IntegrationId;

use crate::auth::TwitchSignIn;
use crate::bot_account::TwitchBotAccount;
use crate::builtin::TwitchIntegrationBundle;
use crate::chat_platform::TwitchPlatform;
use crate::config::{ChatSessionConfig, HELIX_BUDGET_CAPACITY, HELIX_BUDGET_WINDOW};
use crate::credentials;
use crate::credentials_manager::TwitchCredentialsManager;
use crate::helix::{HelixHttpTransport, HelixTokenRefresher, HelixTokenSource, HelixTransport};
use crate::helix_gateway::helix_gateway;
use crate::integration::TWITCH_INTEGRATION;
use crate::lifecycle::TwitchLifecycle;
use crate::oauth_client::{
    TWITCH_OAUTH_BUDGET_CAPACITY, TWITCH_OAUTH_BUDGET_WINDOW, TwitchOAuthClient,
    twitch_oauth_gateway,
};
use crate::sub_actions::register_twitch_sub_actions;
use crate::subscriptions::SubscriptionTracker;

pub struct TwitchAssembly {
    endpoints: PlatformEndpoints,
    client_id: String,
    creds: Arc<dyn CredentialsRepo>,
    manager: Arc<TwitchCredentialsManager>,
    lifecycle: TwitchLifecycle,
    helix: Gateway,
    bot: Arc<TwitchBotAccount>,
    sign_in: TwitchSignIn,
}

pub struct TwitchSession {
    platform: Arc<TwitchPlatform>,
    login: Option<Option<String>>,
}

impl TwitchAssembly {
    pub fn new(
        sub_actions: &mut SubActionRegistry,
        creds: Arc<dyn CredentialsRepo>,
        bus: &Arc<dyn EventPublisher>,
        endpoints: &PlatformEndpoints,
        client_id: String,
    ) -> Option<Self> {
        let lifecycle = TwitchLifecycle::new();
        let oauth = TwitchOAuthClient::new(oauth_gateway(endpoints)?, client_id.clone());
        let manager = Arc::new(TwitchCredentialsManager::new(
            oauth.clone(),
            Arc::clone(&creds),
        ));
        let helix = budgeted_helix_gateway(endpoints, bus)?;
        let transport: Arc<dyn HelixTransport> = Arc::new(
            HelixHttpTransport::new(
                helix.clone(),
                client_id.clone(),
                Arc::clone(&manager) as Arc<dyn HelixTokenSource>,
            )
            .with_refresher(Arc::clone(&manager) as Arc<dyn HelixTokenRefresher>),
        );
        let bot = TwitchBotAccount::new(
            Arc::clone(&creds),
            oauth.clone(),
            Arc::clone(bus),
            budgeted_helix_gateway(endpoints, bus)?,
            Arc::clone(&transport),
        );
        let sign_in = TwitchSignIn::new(oauth, helix.clone());
        if let Err(e) = register_twitch_sub_actions(
            sub_actions,
            transport,
            Arc::clone(&creds),
            lifecycle.clone(),
            Some(Arc::clone(&bot)),
        ) {
            tracing::error!(error = %e, "twitch sub-action registration failed");
        }
        Some(Self {
            endpoints: endpoints.clone(),
            client_id,
            creds,
            manager,
            lifecycle,
            helix,
            bot,
            sign_in,
        })
    }

    pub fn id(&self) -> IntegrationId {
        TWITCH_INTEGRATION.id
    }

    pub fn bot(&self) -> Arc<TwitchBotAccount> {
        Arc::clone(&self.bot)
    }

    pub fn sign_in(&self) -> TwitchSignIn {
        self.sign_in.clone()
    }

    pub fn credentials_manager(&self) -> Arc<TwitchCredentialsManager> {
        Arc::clone(&self.manager)
    }

    pub async fn is_configured(&self) -> Result<bool, StorageError> {
        has_credentials_for(self.creds.as_ref(), &self.id()).await
    }

    pub async fn open_session(&self) -> TwitchSession {
        let stored = credentials::load(self.creds.as_ref())
            .await
            .inspect_err(|error| tracing::warn!(%error, "twitch credentials could not be read"))
            .ok()
            .flatten();
        let user_id = stored
            .as_ref()
            .map(|stored| stored.user_id.clone())
            .unwrap_or_default();
        let platform = Arc::new(
            TwitchPlatform::new(
                ChatSessionConfig {
                    client_id: self.client_id.clone(),
                    broadcaster_id: user_id.clone(),
                    user_id,
                    endpoints: self.endpoints.clone(),
                },
                Arc::clone(&self.creds),
                Arc::clone(&self.manager),
                SubscriptionTracker::default(),
                self.helix.clone(),
                self.lifecycle.clone(),
            )
            .with_bot(Arc::clone(&self.bot)),
        );
        let login = stored.map(|stored| (!stored.login.is_empty()).then_some(stored.login));
        TwitchSession { platform, login }
    }
}

impl TwitchSession {
    pub fn chat_platform(&self) -> Arc<dyn ChatPlatform> {
        Arc::clone(&self.platform) as _
    }

    pub fn into_bundle(self) -> Option<Arc<TwitchIntegrationBundle>> {
        self.login
            .map(|login| TwitchIntegrationBundle::new(login, self.platform))
    }
}

fn oauth_gateway(endpoints: &PlatformEndpoints) -> Option<Gateway> {
    let limiter: Arc<dyn RateLimiter> = Arc::new(TokenBucketRateLimiter::new(
        TWITCH_OAUTH_BUDGET_CAPACITY,
        TWITCH_OAUTH_BUDGET_WINDOW,
    ));
    twitch_oauth_gateway(endpoints, limiter)
        .inspect_err(|e| tracing::error!(error = %e, "twitch oauth gateway build failed"))
        .ok()
}

fn budgeted_helix_gateway(
    endpoints: &PlatformEndpoints,
    bus: &Arc<dyn EventPublisher>,
) -> Option<Gateway> {
    let limiter: Arc<dyn RateLimiter> = Arc::new(TokenBucketRateLimiter::new(
        HELIX_BUDGET_CAPACITY,
        HELIX_BUDGET_WINDOW,
    ));
    helix_gateway(endpoints, limiter, Arc::clone(bus))
        .inspect_err(|e| tracing::error!(error = %e, "twitch helix gateway build failed"))
        .ok()
}
