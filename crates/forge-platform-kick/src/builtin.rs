use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use forge_net::RateLimiter;
use forge_platform_core::{
    BannerLevel, BuiltinContent, BuiltinHealth, BuiltinStatus, CapabilityFlags, ChatPlatform,
    ConnectionState, DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthMetricId,
    HealthStream, HealthText, HealthValue, HeroBadge, HeroBadgeTone, ViewerReport,
};
use forge_registry::{RegistryError, TriggerRegistry};
use forge_types::IntegrationId;
use tokio::sync::{broadcast, watch};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tokio_util::sync::CancellationToken;

use crate::api::poller::PollerAuth;
use crate::auth::credentials_manager::KickCredentialsManager;
use crate::capabilities::KICK_COMMUNITY_NOTE;
use crate::platform::KickPlatform;
use crate::pusher::protocol::chatroom_channel;
use crate::triggers::ban::BanDescriptor;
use crate::triggers::chat::ChatDescriptor;
use crate::triggers::chat_command::ChatCommandDescriptor;
use crate::triggers::host::HostDescriptor;
use crate::triggers::livestream_metadata::LivestreamMetadataDescriptor;
use crate::triggers::livestream_status::LivestreamStatusDescriptor;
use crate::triggers::message_deleted::MessageDeletedDescriptor;
use crate::triggers::reward_redeemed::RewardRedeemedDescriptor;
use crate::triggers::sub::SubDescriptor;
use crate::triggers::sub_gift::SubGiftDescriptor;
use crate::vocabulary::messages;

const LIVE_CHAT_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_LIVE_CHAT);
const EVENTS_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_EVENTS);
const API_CALLS_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_API_CALLS);

pub fn register_kick_triggers(registry: &mut TriggerRegistry) -> Result<(), RegistryError> {
    let mut registry = registry.owned_by(crate::KICK_INTEGRATION.id);
    registry.register(Box::new(ChatDescriptor))?;
    registry.register(Box::new(ChatCommandDescriptor))?;
    registry.register(Box::new(SubDescriptor))?;
    registry.register(Box::new(SubGiftDescriptor))?;
    registry.register(Box::new(BanDescriptor))?;
    registry.register(Box::new(MessageDeletedDescriptor))?;
    registry.register(Box::new(HostDescriptor))?;
    registry.register(Box::new(LivestreamStatusDescriptor))?;
    registry.register(Box::new(LivestreamMetadataDescriptor))?;
    registry.register(Box::new(RewardRedeemedDescriptor))?;
    Ok(())
}

pub struct KickIntegrationBundle {
    id: IntegrationId,
    slug: String,
    user_id: u64,
    token_expires_at: RwLock<Option<SystemTime>>,
    health_tx: broadcast::Sender<HealthDelta>,
    platform: Arc<KickPlatform>,
    credentials_manager: Arc<KickCredentialsManager>,
    rate_limiter: Arc<dyn RateLimiter>,
    viewer_report_rx: watch::Receiver<ViewerReport>,
    poller_auth_rx: watch::Receiver<PollerAuth>,
    state_rx: watch::Receiver<ConnectionState>,
    retired: CancellationToken,
}

impl KickIntegrationBundle {
    pub fn new(
        slug: String,
        user_id: u64,
        platform: Arc<KickPlatform>,
        credentials_manager: Arc<KickCredentialsManager>,
        rate_limiter: Arc<dyn RateLimiter>,
        viewer_report_rx: watch::Receiver<ViewerReport>,
        poller_auth_rx: watch::Receiver<PollerAuth>,
    ) -> (Arc<Self>, broadcast::Sender<HealthDelta>) {
        let (health_tx, _) = broadcast::channel(forge_platform_core::HEALTH_CHANNEL_CAPACITY);
        let state_rx = platform.state_receiver();
        let bundle = Arc::new(Self {
            id: crate::KICK_INTEGRATION.id,
            slug,
            user_id,
            token_expires_at: RwLock::new(None),
            health_tx: health_tx.clone(),
            platform,
            credentials_manager,
            rate_limiter,
            viewer_report_rx,
            poller_auth_rx,
            state_rx,
            retired: CancellationToken::new(),
        });
        Self::spawn_health_bridge(&bundle);
        Self::spawn_viewer_health_bridge(&bundle);
        Self::spawn_events_health_bridge(&bundle);
        Self::spawn_identity_refresh(&bundle);
        (bundle, health_tx)
    }

    fn current_state(&self) -> ConnectionState {
        self.platform.connection_state()
    }

    fn ws_health_value(&self) -> HealthValue {
        let state = self.current_state();
        HealthValue::Status {
            label: HealthText::Connection(state),
            active: state.is_connected(),
            detail: Some(HealthText::unlocalized(chatroom_channel(&self.slug))),
        }
    }

    fn api_calls_health_value(&self) -> HealthValue {
        let usage = self.rate_limiter.usage();
        match (usage.used, usage.capacity) {
            (Some(used), Some(capacity)) => HealthValue::Ratio {
                used: u64::from(used),
                total: u64::from(capacity),
                reset_hint: usage
                    .resets_in
                    .map(|d| format!("resets in {}s", d.as_secs()))
                    .map(HealthText::unlocalized),
            },
            _ => HealthValue::Text {
                primary: HealthText::unlocalized("-"),
                secondary: None,
            },
        }
    }

    fn spawn_health_bridge(bundle: &Arc<Self>) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let bundle = Arc::clone(bundle);
        let mut state_rx = bundle.state_rx.clone();
        handle.spawn(async move {
            let mut previous = *state_rx.borrow();
            loop {
                tokio::select! {
                    biased;
                    () = bundle.retired.cancelled() => break,
                    changed = state_rx.changed() => if changed.is_err() { break },
                }
                let current = *state_rx.borrow();

                let ws_delta = HealthDelta {
                    metric: LIVE_CHAT_METRIC,
                    new_value: bundle.ws_health_value(),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(ws_delta);

                let api_calls_delta = HealthDelta {
                    metric: API_CALLS_METRIC,
                    new_value: bundle.api_calls_health_value(),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(api_calls_delta);

                if current.is_connected() && !previous.is_connected() {
                    Self::spawn_identity_refresh(&bundle);
                }
                previous = current;
            }
        });
    }

    fn spawn_viewer_health_bridge(bundle: &Arc<Self>) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let bundle = Arc::clone(bundle);
        let mut reports_rx = bundle.viewer_report_rx.clone();
        handle.spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    () = bundle.retired.cancelled() => break,
                    changed = reports_rx.changed() => if changed.is_err() { break },
                }
                let viewers_delta = HealthDelta {
                    metric: HealthMetricId::VIEWERS,
                    new_value: viewers_health_value(*reports_rx.borrow()),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(viewers_delta);

                let api_calls_delta = HealthDelta {
                    metric: API_CALLS_METRIC,
                    new_value: bundle.api_calls_health_value(),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(api_calls_delta);
            }
        });
    }

    fn spawn_events_health_bridge(bundle: &Arc<Self>) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let bundle = Arc::clone(bundle);
        let mut auth_rx = bundle.poller_auth_rx.clone();
        handle.spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    () = bundle.retired.cancelled() => break,
                    changed = auth_rx.changed() => if changed.is_err() { break },
                }
                let events_delta = HealthDelta {
                    metric: EVENTS_METRIC,
                    new_value: events_health_value(*auth_rx.borrow()),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(events_delta);
            }
        });
    }

    fn spawn_identity_refresh(bundle: &Arc<Self>) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let bundle = Arc::clone(bundle);
        handle.spawn(async move {
            bundle.refresh_identity().await;
        });
    }

    pub(crate) async fn refresh_identity(&self) {
        let Ok(Some(stored)) = self.credentials_manager.load().await else {
            return;
        };
        if let Ok(mut guard) = self.token_expires_at.write() {
            *guard = Some(SystemTime::from(stored.expires_at));
        }
    }

    pub async fn shutdown(&self) {
        self.retired.cancel();
        if let Err(error) = self.platform.disconnect().await {
            tracing::warn!(%error, "kick chat disconnect failed at shutdown");
        }
    }

    pub(crate) fn credentials_manager(&self) -> &Arc<KickCredentialsManager> {
        &self.credentials_manager
    }

    pub(crate) fn platform(&self) -> &Arc<KickPlatform> {
        &self.platform
    }
}

fn events_health_value(auth: PollerAuth) -> HealthValue {
    match auth {
        PollerAuth::Authorized => HealthValue::Status {
            label: HealthText::unlocalized("Active"),
            active: true,
            detail: Some(HealthText::unlocalized("channels 30s / redemptions 12s")),
        },
        PollerAuth::AuthRequired => HealthValue::Status {
            label: HealthText::unlocalized("Auth required"),
            active: false,
            detail: Some(HealthText::unlocalized("sign in to Kick again")),
        },
    }
}

fn viewers_health_value(report: ViewerReport) -> HealthValue {
    match report {
        ViewerReport::Live { count } => HealthValue::Text {
            primary: HealthText::unlocalized(count.to_string()),
            secondary: Some(HealthText::unlocalized("live")),
        },
        ViewerReport::Absent => HealthValue::Text {
            primary: HealthText::unlocalized("0"),
            secondary: None,
        },
    }
}

impl BuiltinStatus for KickIntegrationBundle {
    fn id(&self) -> &IntegrationId {
        &self.id
    }

    fn display_name(&self) -> &str {
        "Kick"
    }

    fn version(&self) -> Option<&str> {
        None
    }

    fn connection(&self) -> ConnectionState {
        self.current_state()
    }

    fn uptime(&self) -> Option<Duration> {
        None
    }

    fn endpoint(&self) -> Option<&str> {
        Some("Pusher WS (read) + OAuth API (write)")
    }

    fn capability_flags(&self) -> CapabilityFlags {
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }

    fn header_actions(&self) -> Vec<HeaderAction> {
        vec![HeaderAction::RefreshToken, HeaderAction::Disconnect]
    }

    fn token_expiry(&self) -> Option<SystemTime> {
        *self
            .token_expires_at
            .read()
            .unwrap_or_else(|p| p.into_inner())
    }

    fn hero_name(&self) -> Option<&str> {
        Some(&self.slug)
    }

    fn name_badges(&self) -> Vec<HeroBadge> {
        vec![HeroBadge {
            label: format!("user_id {}", self.user_id),
            tone: HeroBadgeTone::Neutral,
            monospace: true,
        }]
    }
}

impl BuiltinHealth for KickIntegrationBundle {
    fn metrics(&self) -> [HealthMetric; 4] {
        [
            HealthMetric {
                id: LIVE_CHAT_METRIC,
                value: self.ws_health_value(),
            },
            HealthMetric {
                id: EVENTS_METRIC,
                value: events_health_value(*self.poller_auth_rx.borrow()),
            },
            HealthMetric {
                id: HealthMetricId::VIEWERS,
                value: viewers_health_value(*self.viewer_report_rx.borrow()),
            },
            HealthMetric {
                id: API_CALLS_METRIC,
                value: self.api_calls_health_value(),
            },
        ]
    }

    fn stream(&self) -> HealthStream {
        let rx = self.health_tx.subscribe();
        Box::pin(BroadcastStream::new(rx).filter_map(|r| r.ok()))
    }
}

impl BuiltinContent for KickIntegrationBundle {
    fn sections(&self) -> Vec<DetailSection> {
        vec![DetailSection::WarningBanner {
            level: BannerLevel::Warning,
            title: "Hybrid chat transport".to_owned(),
            body: KICK_COMMUNITY_NOTE.to_owned(),
            cta: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use forge_registry::KindPlatformContract;
    use forge_types::PlatformId;

    use super::*;

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = TriggerRegistry::new();
        register_kick_triggers(&mut reg).unwrap();
        let result = register_kick_triggers(&mut reg);
        assert!(result.is_err());
    }

    #[test]
    fn all_kind_ids_are_reachable() {
        let mut reg = TriggerRegistry::new();
        register_kick_triggers(&mut reg).unwrap();
        for id in [
            "kick.chat.message.sent",
            "kick.chat.command",
            "kick.channel.subscribed",
            "kick.channel.subscription.gifts",
            "kick.moderation.banned",
            "kick.chat.message.deleted",
            "kick.channel.hosted",
            "kick.livestream.status.updated",
            "kick.livestream.metadata.updated",
            "kick.channel.reward.redemption.updated",
        ] {
            assert!(reg.get(id).is_some(), "missing kind id: {id}");
        }
    }

    #[test]
    fn all_descriptors_are_platform_specific_kick() {
        let mut reg = TriggerRegistry::new();
        register_kick_triggers(&mut reg).unwrap();
        for descriptor in reg.all() {
            assert_eq!(
                descriptor.platform_contract(),
                KindPlatformContract::PlatformSpecific(PlatformId::Kick),
                "descriptor '{}' must be platform-specific Kick",
                descriptor.id()
            );
        }
    }

    mod lifecycle {
        use std::time::Duration as StdDuration;

        use async_trait::async_trait;
        use forge_net::{RateLimitError, RateLimitOutcome, RateLimiter};
        use forge_platform_core::{BuiltinControl, ControlFailure, QuickActions};
        use forge_storage::{CredentialId, CredentialsRepo, StorageError};
        use time::OffsetDateTime;

        use super::super::*;
        use crate::platform::KickPlatform;

        struct EmptyRepo;
        #[async_trait]
        impl CredentialsRepo for EmptyRepo {
            async fn store(&self, _: &CredentialId, _: &str) -> Result<(), StorageError> {
                Ok(())
            }
            async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
                Ok(None)
            }
            async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
                Ok(false)
            }
            async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
                Ok(Vec::new())
            }
            async fn last_refresh(
                &self,
                _: &CredentialId,
            ) -> Result<Option<OffsetDateTime>, StorageError> {
                Ok(None)
            }
            async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
                Ok(())
            }
        }

        pub(super) struct GrantLimiter;
        #[async_trait]
        impl RateLimiter for GrantLimiter {
            async fn acquire(&self, _: u32) -> Result<RateLimitOutcome, RateLimitError> {
                Ok(RateLimitOutcome::Granted)
            }
            async fn observe_remote_throttle(&self, _: StdDuration) {}
        }

        fn empty_manager() -> Arc<KickCredentialsManager> {
            Arc::new(crate::endpoints_test_support::offline_manager(Arc::new(
                EmptyRepo,
            )))
        }

        pub(super) fn bundle_with_feeds() -> (
            Arc<KickIntegrationBundle>,
            watch::Sender<ViewerReport>,
            watch::Sender<PollerAuth>,
        ) {
            let manager = empty_manager();
            let platform = Arc::new(KickPlatform::new(
                &crate::endpoints_test_support::unreachable_endpoints(),
                manager.clone(),
                Arc::new(crate::api::chat_messages::KickSendChat::new(
                    crate::endpoints_test_support::unreachable_api(),
                )),
                crate::endpoints_test_support::unreachable_channel_info(),
                crate::ban_ledger_test_support::MemoryBanLedger::shared(),
            ));
            bundle_on(platform, manager)
        }

        fn bundle_on(
            platform: Arc<KickPlatform>,
            manager: Arc<KickCredentialsManager>,
        ) -> (
            Arc<KickIntegrationBundle>,
            watch::Sender<ViewerReport>,
            watch::Sender<PollerAuth>,
        ) {
            let (viewer_tx, viewer_rx) = watch::channel(ViewerReport::Absent);
            let (auth_tx, auth_rx) = watch::channel(PollerAuth::Authorized);
            let (bundle, _) = KickIntegrationBundle::new(
                "test_channel".to_owned(),
                777,
                platform,
                manager,
                Arc::new(GrantLimiter),
                viewer_rx,
                auth_rx,
            );
            (bundle, viewer_tx, auth_tx)
        }

        pub(super) fn disconnected_bundle() -> Arc<KickIntegrationBundle> {
            bundle_with_feeds().0
        }

        #[test]
        fn live_chat_health_metric_reports_disconnected_with_slug_topic_while_offline() {
            let bundle = disconnected_bundle();
            let metrics = bundle.metrics();
            let ws = metrics.iter().find(|m| m.id == LIVE_CHAT_METRIC).unwrap();
            assert_eq!(
                ws.value,
                HealthValue::Status {
                    label: "Disconnected".to_owned(),
                    active: false,
                    detail: Some("chatrooms.test_channel.v2".to_owned()),
                }
            );
        }

        #[test]
        fn metrics_declare_unique_ids() {
            let ids: std::collections::HashSet<_> = disconnected_bundle()
                .metrics()
                .into_iter()
                .map(|metric| metric.id)
                .collect();

            assert_eq!(ids.len(), 4);
        }

        fn labels_with_enabled(bundle: &KickIntegrationBundle, enabled: bool) -> Vec<String> {
            bundle
                .actions()
                .into_iter()
                .filter(|a| a.enabled == enabled)
                .map(|a| a.label)
                .collect()
        }

        async fn connected_bundle() -> (
            Arc<KickIntegrationBundle>,
            Arc<KickPlatform>,
            wiremock::MockServer,
        ) {
            let (platform, server) = crate::platform::tests::platform_on_chat(
                "{}".to_owned(),
                crate::ban_ledger_test_support::MemoryBanLedger::shared(),
            )
            .await;
            let platform = Arc::new(platform);
            let mut state = platform.state_receiver();
            platform.connect().await.unwrap();
            tokio::time::timeout(
                StdDuration::from_secs(30),
                state.wait_for(ConnectionState::is_connected),
            )
            .await
            .expect("the chat session must report connected")
            .unwrap();
            let (bundle, _, _) = bundle_on(Arc::clone(&platform), empty_manager());
            (bundle, platform, server)
        }

        #[test]
        fn every_quick_action_is_disabled_while_disconnected() {
            let bundle = disconnected_bundle();
            assert_eq!(labels_with_enabled(&bundle, true), Vec::<String>::new());
        }

        #[tokio::test]
        async fn every_quick_action_is_enabled_once_chat_connects() {
            let (bundle, _platform, _server) = connected_bundle().await;
            assert_eq!(labels_with_enabled(&bundle, false), Vec::<String>::new());
        }

        #[tokio::test]
        async fn quick_actions_disable_again_after_chat_disconnects() {
            let (bundle, platform, _server) = connected_bundle().await;
            platform.disconnect().await.unwrap();
            assert_eq!(labels_with_enabled(&bundle, true), Vec::<String>::new());
        }

        #[test]
        fn content_surfaces_the_mandatory_kick_disclaimer_banner() {
            let bundle = disconnected_bundle();
            assert!(
                bundle
                    .sections()
                    .iter()
                    .any(|s| matches!(s, DetailSection::WarningBanner { .. }))
            );
        }

        #[tokio::test]
        async fn reconnect_without_stored_credentials_reports_not_connected() {
            let bundle = disconnected_bundle();
            let outcome = BuiltinControl::reconnect(bundle.as_ref()).await;
            assert_eq!(outcome, Err(ControlFailure::NotConnected));
        }

        #[tokio::test]
        async fn refresh_token_without_stored_credentials_reports_not_connected() {
            let bundle = disconnected_bundle();
            let outcome = BuiltinControl::refresh_token(bundle.as_ref()).await;
            assert_eq!(outcome, Err(ControlFailure::NotConnected));
        }

        #[tokio::test]
        async fn dyn_control_disconnect_while_disconnected_reports_not_connected() {
            let control: Arc<dyn BuiltinControl> = disconnected_bundle();
            let outcome = control.disconnect().await;
            assert_eq!(outcome, Err(ControlFailure::NotConnected));
        }

        #[tokio::test]
        async fn a_viewer_report_updates_the_shared_viewers_metric() {
            let (bundle, viewer_tx, _auth_tx) = bundle_with_feeds();
            let mut health = bundle.stream();

            viewer_tx.send(ViewerReport::Live { count: 5 }).unwrap();

            let delta = tokio::time::timeout(StdDuration::from_secs(30), health.next())
                .await
                .expect("the viewer bridge must publish a health delta")
                .expect("the health stream must stay open");

            assert_eq!(delta.metric, HealthMetricId::VIEWERS);
            assert_eq!(
                delta.new_value,
                HealthValue::Text {
                    primary: "5".to_owned(),
                    secondary: Some("live".to_owned()),
                }
            );
        }

        #[tokio::test]
        async fn a_poller_auth_change_updates_the_events_metric_slot() {
            let (bundle, _viewer_tx, auth_tx) = bundle_with_feeds();
            let mut health = bundle.stream();

            for (auth, active, label) in [
                (PollerAuth::AuthRequired, false, "Auth required"),
                (PollerAuth::Authorized, true, "Active"),
            ] {
                auth_tx.send(auth).unwrap();

                let delta = tokio::time::timeout(StdDuration::from_secs(30), health.next())
                    .await
                    .expect("the events bridge must publish a health delta")
                    .expect("the health stream must stay open");

                assert_eq!(delta.metric, EVENTS_METRIC, "{auth:?}");
                let metrics = bundle.metrics();
                let slot = metrics.iter().find(|m| m.id == EVENTS_METRIC).unwrap();
                assert_eq!(delta.new_value, slot.value, "{auth:?}");
                assert!(
                    matches!(
                        &slot.value,
                        HealthValue::Status { label: shown, active: lit, .. }
                            if shown == label && *lit == active
                    ),
                    "{auth:?}: {:?}",
                    slot.value
                );
            }
        }

        #[tokio::test]
        async fn shutting_down_twice_stops_every_bridge_that_holds_the_bundle() {
            let (bundle, _viewer_tx, _auth_tx) = bundle_with_feeds();

            bundle.shutdown().await;
            bundle.shutdown().await;

            let released = tokio::time::timeout(StdDuration::from_secs(30), async {
                while Arc::strong_count(&bundle) > 1 {
                    tokio::time::sleep(StdDuration::from_millis(5)).await;
                }
            })
            .await;
            assert!(
                released.is_ok(),
                "{} task(s) still hold the bundle after shutdown",
                Arc::strong_count(&bundle) - 1
            );
            assert!(!bundle.current_state().is_connected());
        }
    }

    mod roster {
        use std::collections::{BTreeMap, BTreeSet};

        use forge_platform_core::{
            PlatformError, QuickAction, QuickActionFieldKind, QuickActionFieldValue, QuickActions,
        };
        use forge_registry::SubActionRegistry;
        use futures::future::BoxFuture;

        use super::super::*;
        use super::lifecycle::{GrantLimiter, disconnected_bundle};
        use crate::api::categories::KickCategories;
        use crate::api::channel::KickChannel;
        use crate::api::chat_messages::KickSendChat;
        use crate::api::moderation::KickModeration;
        use crate::api::rewards::KickRewards;
        use crate::sub_actions::{KickSubActionDeps, register_kick_sub_actions};

        fn runner_registry() -> SubActionRegistry {
            let limiter: Arc<dyn RateLimiter> = Arc::new(GrantLimiter);
            let mut registry = SubActionRegistry::new();
            register_kick_sub_actions(
                &mut registry,
                KickSubActionDeps {
                    client: Arc::new(KickSendChat::new(
                        crate::endpoints_test_support::api_limited_by(
                            "http://127.0.0.1:0",
                            Arc::clone(&limiter),
                        ),
                    )),
                    token_source: Arc::new(|| {
                        Box::pin(async { Ok("tok".to_owned()) })
                            as BoxFuture<'static, Result<String, PlatformError>>
                    }),
                    broadcaster_id_source: Arc::new(|| {
                        Box::pin(async { Ok(1_u64) })
                            as BoxFuture<'static, Result<u64, PlatformError>>
                    }),
                    moderation: Arc::new(KickModeration::new(
                        crate::endpoints_test_support::api_limited_by(
                            "http://127.0.0.1:0",
                            Arc::clone(&limiter),
                        ),
                        crate::ban_ledger_test_support::MemoryBanLedger::shared(),
                    )),
                    channel: Arc::new(KickChannel::new(
                        crate::endpoints_test_support::api_limited_by(
                            "http://127.0.0.1:0",
                            Arc::clone(&limiter),
                        ),
                    )),
                    rewards: Arc::new(KickRewards::new(
                        crate::endpoints_test_support::api_limited_by(
                            "http://127.0.0.1:0",
                            Arc::clone(&limiter),
                        ),
                    )),
                    categories: Arc::new(KickCategories::new(
                        crate::endpoints_test_support::api_limited_by(
                            "http://127.0.0.1:0",
                            limiter,
                        ),
                    )),
                },
            )
            .unwrap();
            registry
        }

        #[test]
        fn every_quick_action_targets_a_registered_runner_that_reads_its_keys() {
            let registry = runner_registry();

            for action in disconnected_bundle().actions() {
                let kind_id = &action.subaction_template.kind_id;
                let runner = registry.get(kind_id).unwrap_or_else(|| {
                    panic!(
                        "quick action '{}' targets unknown runner '{kind_id}'",
                        action.label
                    )
                });
                let accepted: BTreeSet<String> = runner.default_config().into_keys().collect();

                let used = action
                    .subaction_template
                    .config
                    .keys()
                    .cloned()
                    .chain(action.fields.iter().map(|f| f.key.clone()));
                for key in used {
                    assert!(
                        accepted.contains(&key),
                        "quick action '{}' sets '{key}', which runner '{kind_id}' does not read",
                        action.label,
                    );
                }
            }
        }

        #[test]
        fn free_text_quick_action_fields_open_blank() {
            for action in disconnected_bundle().actions() {
                for field in &action.fields {
                    if !matches!(
                        field.kind,
                        QuickActionFieldKind::Text | QuickActionFieldKind::Multiline
                    ) {
                        continue;
                    }
                    let prefilled_default = match &field.default {
                        None => false,
                        Some(QuickActionFieldValue::Text(text)) => !text.is_empty(),
                        Some(_) => true,
                    };
                    let prefilled_config = matches!(
                        action.subaction_template.config.get(&field.key),
                        Some(value) if *value != forge_types::Variant::String(String::new())
                    );
                    assert!(
                        !prefilled_default && !prefilled_config,
                        "quick action {:?}: free-text field {:?} is prefilled (default {:?}, config {:?})",
                        action.label,
                        field.key,
                        field.default,
                        action.subaction_template.config.get(&field.key)
                    );
                }
            }
        }

        fn sample_value(key: &str) -> &'static str {
            match key {
                "user_id" => "777",
                "cost" => "500",
                "category_id" => "15",
                "tags" => "chill, speedrun",
                "redemption_ids" => "rd-1, rd-2",
                _ => "supplied",
            }
        }

        fn filled_values(action: &QuickAction) -> BTreeMap<String, QuickActionFieldValue> {
            action
                .fields
                .iter()
                .filter(|field| field.default.is_none())
                .map(|field| {
                    (
                        field.key.clone(),
                        QuickActionFieldValue::Text(sample_value(&field.key).to_owned()),
                    )
                })
                .collect()
        }

        #[test]
        fn every_partial_quick_action_template_merged_with_its_field_values_validates() {
            let registry = runner_registry();

            for action in disconnected_bundle().actions() {
                let step = action.merge_config(&filled_values(&action));
                let verdict = registry
                    .get(&step.kind_id)
                    .unwrap()
                    .validate_config(&step.config);

                assert!(
                    verdict.is_ok(),
                    "quick action '{}' merges a config its runner rejects: {verdict:?}",
                    action.label
                );
            }
        }

        #[test]
        fn quick_actions_are_marked_destructive_exactly_where_the_effect_is_irreversible() {
            let destructive: BTreeSet<String> = disconnected_bundle()
                .actions()
                .into_iter()
                .filter(|a| a.destructive)
                .map(|a| a.label)
                .collect();

            assert_eq!(
                destructive,
                BTreeSet::from([
                    "Ban user".to_owned(),
                    "Delete message".to_owned(),
                    "Delete reward".to_owned(),
                ])
            );
        }
    }
}
