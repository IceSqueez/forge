use super::*;

pub(super) struct SubscriptionPassContext {
    pub(super) manager: Arc<TwitchCredentialsManager>,
    pub(super) helix: Gateway,
    pub(super) client_id: String,
    pub(super) session_id: String,
    pub(super) broadcaster_id: String,
    pub(super) user_id: String,
    pub(super) bus: Arc<dyn EventPublisher>,
    pub(super) tracker: SubscriptionTracker,
    pub(super) retries_tx: oneshot::Sender<TopicRetries>,
}

impl SubscriptionPassContext {
    pub(super) async fn run(self) -> PassVerdict {
        let token = match self.manager.get_valid_access_token().await {
            Ok(token) => token,
            Err(PlatformError::ReauthRequired { .. }) => {
                warn!("chat token refresh rejected: reauth required");
                return PassVerdict::ReauthRequired;
            }
            Err(e) => {
                warn_token_failure!("chat token fetch failed; treating as disconnect", &e);
                return PassVerdict::TokenUnavailable;
            }
        };
        let result = match self.subscribe(&token).await {
            Err(TwitchError::TokenRejected) => {
                debug!("eventsub subscription rejected the access token; refreshing once");
                match self.manager.refresh(&token).await {
                    Ok(renewed) => self.subscribe(&renewed.access_token).await,
                    Err(PlatformError::ReauthRequired { .. }) => {
                        warn!("chat token refresh rejected: reauth required");
                        return PassVerdict::ReauthRequired;
                    }
                    Err(e) => {
                        warn_token_failure!("chat token fetch failed; treating as disconnect", &e);
                        return PassVerdict::TokenUnavailable;
                    }
                }
            }
            first => first,
        };
        match result {
            Ok(pass) if pass.any_live() => {
                if pass.has_scope_gap() {
                    warn!(
                        "some eventsub topics were rejected for a missing scope; chat stays live"
                    );
                    self.manager.credential_state().status_signal().bump();
                    PlatformReauthRequired {
                        platform_id: vocabulary::PLATFORM,
                    }
                    .publish(self.bus.as_ref(), EventSource::Twitch, None);
                }
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the session may stop waiting for the retry pass"
                )]
                let _ = self.retries_tx.send(pass.into_retries());
                PassVerdict::Live
            }
            Ok(_) => PassVerdict::NothingLive,
            Err(TwitchError::ScopeMissing) => {
                warn!("chat subscription rejected: scope missing (reauth required)");
                PassVerdict::ReauthRequired
            }
            Err(_) => {
                warn!("chat subscription rejected the renewed token (reauth required)");
                PassVerdict::ReauthRequired
            }
        }
    }

    pub(super) async fn subscribe(&self, token: &OAuthToken) -> Result<SubscribePass, TwitchError> {
        let helix: Arc<dyn HelixTransport> = Arc::new(HelixHttpTransport::new(
            self.helix.clone(),
            self.client_id.clone(),
            Arc::new(FixedTokenSource::new(token.clone())),
        ));
        subscribe_all(
            &helix,
            &self.session_id,
            &self.broadcaster_id,
            &self.user_id,
            &self.bus,
            &self.tracker,
        )
        .await
    }
}
