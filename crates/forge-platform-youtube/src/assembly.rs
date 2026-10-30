use std::sync::Arc;

use forge_events::EventPublisher;
use forge_net::{RateLimiter, TokenBucketRateLimiter};
use forge_platform_core::{PlatformEndpoints, PlatformError};
use forge_registry::SubActionRegistry;
use forge_storage::{BanLedgerRepo, CredentialsRepo};
use futures::future::BoxFuture;

use crate::auth::{YoutubeSignIn, client_credentials};
use crate::chat_platform::token_source;
use crate::oauth_client::{
    GOOGLE_OAUTH_BUDGET_CAPACITY, GOOGLE_OAUTH_BUDGET_WINDOW, google_oauth_gateway,
};
use crate::{
    ActiveBroadcastIdHandle, GoogleOAuthClient, LiveChatIdHandle, SharedQuota, YoutubeAdBreak,
    YoutubeChannelLookup, YoutubeCredentialsManager, YoutubeDataApi, YoutubeIntegrationBundle,
    YoutubeModeration, YoutubePlatform, YoutubeSendChat, YoutubeStreamMetadata, YoutubeStreamStats,
    YoutubeThumbnail, register_youtube_sub_actions,
};

pub struct YoutubeAssembly {
    api: YoutubeDataApi,
    manager: Arc<YoutubeCredentialsManager>,
    live_chat_id: LiveChatIdHandle,
    active_broadcast: ActiveBroadcastIdHandle,
    ban_ledger: Arc<dyn BanLedgerRepo>,
    sign_in: YoutubeSignIn,
}

pub struct YoutubeSession {
    pub platform: Arc<YoutubePlatform>,
    pub bundle: Arc<YoutubeIntegrationBundle>,
}

impl YoutubeAssembly {
    pub fn new(
        endpoints: &PlatformEndpoints,
        events: Arc<dyn EventPublisher>,
        creds: Arc<dyn CredentialsRepo>,
        ban_ledger: Arc<dyn BanLedgerRepo>,
    ) -> Option<Self> {
        let (client_id, client_secret) = client_credentials()?;
        let limiter: Arc<dyn RateLimiter> = Arc::new(TokenBucketRateLimiter::new(
            GOOGLE_OAUTH_BUDGET_CAPACITY,
            GOOGLE_OAUTH_BUDGET_WINDOW,
        ));
        let oauth_gateway = google_oauth_gateway(endpoints, limiter)
            .inspect_err(|e| tracing::error!(error = %e, "youtube oauth gateway build failed"))
            .ok()?;
        let oauth = GoogleOAuthClient::new(oauth_gateway, client_id, client_secret);
        let api = YoutubeDataApi::new(endpoints, SharedQuota::default(), events)
            .inspect_err(|e| tracing::error!(error = %e, "youtube data api gateway build failed"))
            .ok()?;
        let manager = Arc::new(YoutubeCredentialsManager::new(creds, oauth, api.clone()));
        let sign_in = YoutubeSignIn::new(Arc::clone(&manager));
        Some(Self {
            api,
            manager,
            live_chat_id: LiveChatIdHandle::new(),
            active_broadcast: ActiveBroadcastIdHandle::new(),
            ban_ledger,
            sign_in,
        })
    }

    pub fn sign_in(&self) -> YoutubeSignIn {
        self.sign_in.clone()
    }

    pub fn credentials_manager(&self) -> Arc<YoutubeCredentialsManager> {
        Arc::clone(&self.manager)
    }

    pub fn register_sub_actions(&self, registry: &mut SubActionRegistry) {
        let send = Arc::new(YoutubeSendChat::new(
            self.api.clone(),
            token_source(Arc::clone(&self.manager)),
            self.live_chat_id.clone(),
        ));
        let broadcaster_source = {
            let manager = Arc::clone(&self.manager);
            Arc::new(move || {
                let manager = Arc::clone(&manager);
                Box::pin(async move { manager.broadcaster().await }) as BoxFuture<'static, _>
            })
        };
        let moderation = Arc::new(YoutubeModeration::new(
            self.api.clone(),
            token_source(Arc::clone(&self.manager)),
            broadcaster_source,
            self.live_chat_id.clone(),
            Arc::clone(&self.ban_ledger),
        ));
        let metadata = Arc::new(YoutubeStreamMetadata::new(
            self.api.clone(),
            token_source(Arc::clone(&self.manager)),
            self.active_broadcast.clone(),
        ));
        let stream_stats = Arc::new(YoutubeStreamStats::new(
            self.api.clone(),
            token_source(Arc::clone(&self.manager)),
            self.active_broadcast.clone(),
        ));
        let ad_break = Arc::new(YoutubeAdBreak::new(
            self.api.clone(),
            token_source(Arc::clone(&self.manager)),
            self.active_broadcast.clone(),
        ));
        let thumbnail = Arc::new(YoutubeThumbnail::new(
            self.api.clone(),
            token_source(Arc::clone(&self.manager)),
            self.active_broadcast.clone(),
        ));
        let channel_lookup = Arc::new(YoutubeChannelLookup::new(
            self.api.clone(),
            token_source(Arc::clone(&self.manager)),
        ));
        if let Err(e) = register_youtube_sub_actions(
            registry,
            send,
            moderation,
            metadata,
            stream_stats,
            ad_break,
            thumbnail,
            channel_lookup,
        ) {
            tracing::error!(error = %e, "youtube sub-action registration failed");
        }
    }

    pub async fn start(&self) -> Result<Option<YoutubeSession>, PlatformError> {
        let Some(stored) = self.manager.load().await? else {
            return Ok(None);
        };
        let platform = Arc::new(YoutubePlatform::new(
            self.api.clone(),
            stored.channel_id.clone(),
            Arc::clone(&self.manager),
            self.live_chat_id.clone(),
            self.active_broadcast.clone(),
            Arc::clone(&self.ban_ledger),
        ));
        let bundle = YoutubeIntegrationBundle::new(
            stored.channel_id,
            Arc::clone(&platform),
            Arc::clone(&self.manager),
        );
        Ok(Some(YoutubeSession { platform, bundle }))
    }
}
