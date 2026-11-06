use std::sync::Arc;

use forge_events::EventPublisher;
use forge_net::{RateLimiter, TokenBucketRateLimiter};
use forge_platform_core::PlatformFuture;
use forge_platform_core::{PlatformEndpoints, PlatformError, ViewerAvatarSource};
use forge_registry::SubActionRegistry;
use forge_storage::{BanLedgerRepo, CredentialsRepo};
use forge_types::Secret;

use crate::account::auth::YouTubeSignIn;
use crate::account::oauth_client::{
    GOOGLE_OAUTH_BUDGET_CAPACITY, GOOGLE_OAUTH_BUDGET_WINDOW, google_oauth_gateway,
};
use crate::broadcast::broadcast_lookup::BroadcastLookup;
use crate::chat::avatar_book::AvatarBook;
use crate::chat::chat_platform::token_source;
use crate::{
    ActiveBroadcastIdHandle, GoogleOAuthClient, LiveChatIdHandle, SharedQuota, YouTubeAdBreak,
    YouTubeChannelLookup, YouTubeCredentialsManager, YouTubeDataApi, YouTubeIntegrationBundle,
    YouTubeModeration, YouTubePlatform, YouTubeSendChat, YouTubeStreamMetadata, YouTubeStreamStats,
    YouTubeThumbnail, register_youtube_sub_actions,
};

pub struct YouTubeAssembly {
    api: YouTubeDataApi,
    manager: Arc<YouTubeCredentialsManager>,
    live_chat_id: LiveChatIdHandle,
    active_broadcast: ActiveBroadcastIdHandle,
    ban_ledger: Arc<dyn BanLedgerRepo>,
    sign_in: YouTubeSignIn,
    avatars: AvatarBook,
}

pub struct YouTubeSession {
    pub platform: Arc<YouTubePlatform>,
    pub bundle: Arc<YouTubeIntegrationBundle>,
}

impl YouTubeAssembly {
    pub fn new(
        endpoints: &PlatformEndpoints,
        events: Arc<dyn EventPublisher>,
        creds: Arc<dyn CredentialsRepo>,
        ban_ledger: Arc<dyn BanLedgerRepo>,
        (client_id, client_secret): (String, Secret),
    ) -> Option<Self> {
        let limiter: Arc<dyn RateLimiter> = Arc::new(TokenBucketRateLimiter::new(
            GOOGLE_OAUTH_BUDGET_CAPACITY,
            GOOGLE_OAUTH_BUDGET_WINDOW,
        ));
        let oauth_gateway = google_oauth_gateway(endpoints, limiter)
            .inspect_err(|e| tracing::error!(error = %e, "youtube oauth gateway build failed"))
            .ok()?;
        let oauth = GoogleOAuthClient::new(oauth_gateway, client_id, client_secret);
        let api = YouTubeDataApi::new(endpoints, SharedQuota::default(), events)
            .inspect_err(|e| tracing::error!(error = %e, "youtube data api gateway build failed"))
            .ok()?;
        let manager = Arc::new(YouTubeCredentialsManager::new(creds, oauth, api.clone()));
        let sign_in = YouTubeSignIn::new(Arc::clone(&manager));
        let lookup = BroadcastLookup::new(api.clone(), token_source(Arc::clone(&manager)).source());
        let live_chat_id = LiveChatIdHandle::new();
        live_chat_id.fall_back_to(lookup.clone());
        let active_broadcast = ActiveBroadcastIdHandle::new();
        active_broadcast.fall_back_to(lookup);
        Some(Self {
            api,
            manager,
            live_chat_id,
            active_broadcast,
            ban_ledger,
            sign_in,
            avatars: AvatarBook::default(),
        })
    }

    pub fn viewer_avatars(&self) -> Arc<dyn ViewerAvatarSource> {
        Arc::new(self.avatars.clone())
    }

    pub fn sign_in(&self) -> YouTubeSignIn {
        self.sign_in.clone()
    }

    pub fn credentials_manager(&self) -> Arc<YouTubeCredentialsManager> {
        Arc::clone(&self.manager)
    }

    pub fn register_sub_actions(&self, registry: &mut SubActionRegistry) {
        let send = Arc::new(YouTubeSendChat::new(
            self.api.clone(),
            token_source(Arc::clone(&self.manager)).source(),
            self.live_chat_id.clone(),
        ));
        let broadcaster_source = {
            let manager = Arc::clone(&self.manager);
            Arc::new(move || {
                let manager = Arc::clone(&manager);
                Box::pin(async move { manager.broadcaster().await }) as PlatformFuture<_>
            })
        };
        let moderation = Arc::new(YouTubeModeration::new(
            self.api.clone(),
            token_source(Arc::clone(&self.manager)).source(),
            broadcaster_source,
            self.live_chat_id.clone(),
            Arc::clone(&self.ban_ledger),
        ));
        let metadata = Arc::new(YouTubeStreamMetadata::new(
            self.api.clone(),
            token_source(Arc::clone(&self.manager)).source(),
            self.active_broadcast.clone(),
        ));
        let stream_stats = Arc::new(YouTubeStreamStats::new(
            self.api.clone(),
            token_source(Arc::clone(&self.manager)).source(),
            self.active_broadcast.clone(),
        ));
        let ad_break = Arc::new(YouTubeAdBreak::new(
            self.api.clone(),
            token_source(Arc::clone(&self.manager)).source(),
            self.active_broadcast.clone(),
        ));
        let thumbnail = Arc::new(YouTubeThumbnail::new(
            self.api.clone(),
            token_source(Arc::clone(&self.manager)).source(),
            self.active_broadcast.clone(),
        ));
        let channel_lookup = Arc::new(YouTubeChannelLookup::new(
            self.api.clone(),
            token_source(Arc::clone(&self.manager)).source(),
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

    pub async fn start(&self) -> Result<Option<YouTubeSession>, PlatformError> {
        let Some(stored) = self.manager.load().await? else {
            return Ok(None);
        };
        let platform = Arc::new(
            YouTubePlatform::new(
                self.api.clone(),
                stored.channel_id.clone(),
                Arc::clone(&self.manager),
                self.live_chat_id.clone(),
                self.active_broadcast.clone(),
                Arc::clone(&self.ban_ledger),
            )
            .with_avatar_book(self.avatars.clone()),
        );
        let bundle =
            YouTubeIntegrationBundle::new(Arc::clone(&platform), Arc::clone(&self.manager));
        YouTubeIntegrationBundle::start_background_tasks(&bundle);
        Ok(Some(YouTubeSession { platform, bundle }))
    }
}
