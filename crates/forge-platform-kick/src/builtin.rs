use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use forge_platform_core::{
    BannerLevel, BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus, CapabilityFlags,
    ChatPlatform, ConnectionState, DetailSection, HeaderAction, HealthDelta, HealthMetric,
    HealthStream, HealthValue, HeroBadge, HeroBadgeTone, QuickAction, QuickActionAccent,
    QuickActionField, QuickActionFieldKind, QuickActionFieldValue, QuickActions, RateLimiter,
    SectionIcon, ViewerReport,
};
use forge_registry::{RegistryError, TriggerRegistry};
use forge_types::{SubActionStep, Variant};
use tokio::sync::{broadcast, watch};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::capabilities::KICK_COMMUNITY_NOTE;
use crate::chat_platform::KickPlatform;
use crate::credentials_manager::KickCredentialsManager;
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

pub fn register_kick_triggers(registry: &mut TriggerRegistry) -> Result<(), RegistryError> {
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
    id: BuiltinId,
    slug: String,
    user_id: u64,
    token_expires_at: RwLock<Option<SystemTime>>,
    health_tx: broadcast::Sender<HealthDelta>,
    platform: Arc<KickPlatform>,
    credentials_manager: Arc<KickCredentialsManager>,
    rate_limiter: Arc<dyn RateLimiter>,
    viewer_report_rx: watch::Receiver<ViewerReport>,
    state_rx: watch::Receiver<ConnectionState>,
}

impl KickIntegrationBundle {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        slug: String,
        user_id: u64,
        platform: Arc<KickPlatform>,
        credentials_manager: Arc<KickCredentialsManager>,
        rate_limiter: Arc<dyn RateLimiter>,
        viewer_report_rx: watch::Receiver<ViewerReport>,
    ) -> (Arc<Self>, broadcast::Sender<HealthDelta>) {
        let (health_tx, _) = broadcast::channel(16);
        let state_rx = platform.state_receiver();
        let bundle = Arc::new(Self {
            id: BuiltinId::new("kick"),
            slug,
            user_id,
            token_expires_at: RwLock::new(None),
            health_tx: health_tx.clone(),
            platform,
            credentials_manager,
            rate_limiter,
            viewer_report_rx,
            state_rx,
        });
        Self::spawn_health_bridge(&bundle);
        Self::spawn_viewer_health_bridge(&bundle);
        Self::spawn_identity_refresh(&bundle);
        (bundle, health_tx)
    }

    fn current_state(&self) -> ConnectionState {
        self.platform.connection_state()
    }

    fn ws_health_value(&self) -> HealthValue {
        let state = self.current_state();
        HealthValue::Status {
            label: state.label().to_owned(),
            active: state.is_connected(),
            detail: Some(format!("chatrooms.{}.v2", self.slug)),
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
                    .map(|d| format!("resets in {}s", d.as_secs())),
            },
            _ => HealthValue::Text {
                primary: "-".to_owned(),
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
            while state_rx.changed().await.is_ok() {
                let current = *state_rx.borrow();

                let ws_delta = HealthDelta {
                    index: 0,
                    new_value: bundle.ws_health_value(),
                };
                let _ = bundle.health_tx.send(ws_delta);

                let api_calls_delta = HealthDelta {
                    index: 3,
                    new_value: bundle.api_calls_health_value(),
                };
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
            while reports_rx.changed().await.is_ok() {
                let viewers_delta = HealthDelta {
                    index: 2,
                    new_value: viewers_health_value(*reports_rx.borrow()),
                };
                let _ = bundle.health_tx.send(viewers_delta);

                let api_calls_delta = HealthDelta {
                    index: 3,
                    new_value: bundle.api_calls_health_value(),
                };
                let _ = bundle.health_tx.send(api_calls_delta);
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

    pub(crate) fn credentials_manager(&self) -> &Arc<KickCredentialsManager> {
        &self.credentials_manager
    }

    pub(crate) fn platform(&self) -> &Arc<KickPlatform> {
        &self.platform
    }
}

fn events_health_value() -> HealthValue {
    HealthValue::Status {
        label: "Active".to_owned(),
        active: true,
        detail: Some("channels 30s / redemptions 12s".to_owned()),
    }
}

fn viewers_health_value(report: ViewerReport) -> HealthValue {
    match report {
        ViewerReport::Live { count } => HealthValue::Text {
            primary: count.to_string(),
            secondary: Some("live".to_owned()),
        },
        ViewerReport::Absent => HealthValue::Text {
            primary: "0".to_owned(),
            secondary: None,
        },
    }
}

impl BuiltinStatus for KickIntegrationBundle {
    fn id(&self) -> &BuiltinId {
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

    /// The bundle is only ever constructed after credentials resolve (see build_kick), so
    /// the slug is always populated by the time a hero name is rendered.
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
                label: "Live Chat".to_owned(),
                value: self.ws_health_value(),
            },
            HealthMetric {
                label: "Events".to_owned(),
                value: events_health_value(),
            },
            HealthMetric {
                label: "Viewers".to_owned(),
                value: viewers_health_value(*self.viewer_report_rx.borrow()),
            },
            HealthMetric {
                label: "API Calls".to_owned(),
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

fn blank() -> Variant {
    Variant::String(String::new())
}

fn config(pairs: impl IntoIterator<Item = (&'static str, Variant)>) -> BTreeMap<String, Variant> {
    pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

fn group_badge(group: &str) -> (SectionIcon, QuickActionAccent) {
    match group {
        "Stream info" => (SectionIcon::new("edit"), QuickActionAccent::Info),
        "Chat" => (SectionIcon::new("message-2"), QuickActionAccent::Brand),
        "Moderation" => (SectionIcon::new("shield"), QuickActionAccent::Danger),
        "Rewards" => (SectionIcon::new("gift"), QuickActionAccent::Bits),
        "Lookups" => (SectionIcon::new("search"), QuickActionAccent::Info),
        _ => (SectionIcon::new("dot"), QuickActionAccent::Brand),
    }
}

fn text_field(key: &str, label: &str, default: &str) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind: QuickActionFieldKind::Text,
        default: Some(QuickActionFieldValue::Text(default.to_owned())),
        placeholder: None,
        hint: None,
        required: false,
    }
}

fn text_field_placeholder(
    key: &str,
    label: &str,
    default: &str,
    placeholder: &str,
) -> QuickActionField {
    QuickActionField {
        placeholder: Some(placeholder.to_owned()),
        ..text_field(key, label, default)
    }
}

fn text_field_hint(key: &str, label: &str, default: &str, hint: &str) -> QuickActionField {
    QuickActionField {
        hint: Some(hint.to_owned()),
        ..text_field(key, label, default)
    }
}

fn multiline_field(key: &str, label: &str, default: &str) -> QuickActionField {
    QuickActionField {
        kind: QuickActionFieldKind::Multiline,
        ..text_field(key, label, default)
    }
}

fn toggle_field(key: &str, label: &str, default: bool) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind: QuickActionFieldKind::Toggle,
        default: Some(QuickActionFieldValue::Toggle(default)),
        placeholder: None,
        hint: None,
        required: false,
    }
}

#[allow(clippy::too_many_arguments)]
fn quick_action(
    label: &str,
    icon: &str,
    accent: QuickActionAccent,
    enabled: bool,
    group: &str,
    destructive: bool,
    kind_id: &str,
    config: BTreeMap<String, Variant>,
    fields: Vec<QuickActionField>,
) -> QuickAction {
    let (group_icon, group_accent) = group_badge(group);
    QuickAction {
        label: label.to_owned(),
        icon: SectionIcon::new(icon),
        enabled,
        locked_reason: None,
        group: Some(group.to_owned()),
        group_icon: Some(group_icon),
        group_accent: Some(group_accent),
        destructive,
        accent,
        subaction_template: SubActionStep {
            kind_id: kind_id.to_owned(),
            config,
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        },
        picker: None,
        fields,
    }
}

impl QuickActions for KickIntegrationBundle {
    fn actions(&self) -> Vec<QuickAction> {
        let connected = self.current_state().is_connected();

        vec![
            quick_action(
                "Update title",
                "edit",
                QuickActionAccent::Info,
                connected,
                "Stream info",
                false,
                "kick.channel.update_info",
                config([("title", blank())]),
                vec![
                    text_field_placeholder(
                        "title",
                        "Stream Title",
                        "",
                        "e.g. Modded Minecraft, day 14",
                    )
                    .required(),
                ],
            ),
            quick_action(
                "Set category",
                "category",
                QuickActionAccent::Success,
                connected,
                "Stream info",
                false,
                "kick.channel.update_info",
                config([("category_id", blank())]),
                vec![
                    text_field_hint(
                        "category_id",
                        "Category ID",
                        "",
                        "Use Find category (Lookups) to get the numeric id",
                    )
                    .required(),
                ],
            ),
            quick_action(
                "Set custom tags",
                "hash",
                QuickActionAccent::Brand,
                connected,
                "Stream info",
                false,
                "kick.channel.update_info",
                config([("tags", blank())]),
                vec![
                    text_field_placeholder(
                        "tags",
                        "Tags (comma-separated, max 10)",
                        "",
                        "gaming, chill, speedrun",
                    )
                    .required(),
                ],
            ),
            quick_action(
                "Send message",
                "send",
                QuickActionAccent::Brand,
                connected,
                "Chat",
                false,
                "kick.chat.send_message",
                config([("message", blank()), ("as_bot", Variant::Bool(false))]),
                vec![
                    multiline_field("message", "Message", "Hey chat!").required(),
                    toggle_field("as_bot", "Send as bot", false),
                ],
            ),
            quick_action(
                "Delete message",
                "trash",
                QuickActionAccent::Danger,
                connected,
                "Chat",
                true,
                "kick.chat.delete_message",
                config([("message_id", blank())]),
                vec![
                    text_field_placeholder("message_id", "Message ID", "", "%message_id%")
                        .required(),
                ],
            ),
            quick_action(
                "Timeout user",
                "clock-pause",
                QuickActionAccent::Warning,
                connected,
                "Moderation",
                false,
                "kick.moderation.timeout",
                config([("user_id", blank()), ("duration_minutes", Variant::Int(10))]),
                vec![
                    text_field_placeholder("user_id", "Target User ID", "", "%user_id%").required(),
                ],
            ),
            quick_action(
                "Ban user",
                "ban",
                QuickActionAccent::Danger,
                connected,
                "Moderation",
                true,
                "kick.moderation.ban",
                config([("user_id", blank())]),
                vec![
                    text_field_placeholder("user_id", "Target User ID", "", "%user_id%").required(),
                ],
            ),
            quick_action(
                "Unban user",
                "unban",
                QuickActionAccent::Success,
                connected,
                "Moderation",
                false,
                "kick.moderation.unban",
                config([("user_id", blank())]),
                vec![
                    text_field_placeholder("user_id", "Target User ID", "", "%user_id%").required(),
                ],
            ),
            quick_action(
                "Create reward",
                "gift",
                QuickActionAccent::Success,
                connected,
                "Rewards",
                false,
                "kick.reward.create",
                config([("title", blank()), ("cost", blank())]),
                vec![
                    text_field_placeholder("title", "Reward Title", "", "e.g. Hydrate").required(),
                    text_field_placeholder("cost", "Cost (channel points)", "", "e.g. 500")
                        .required(),
                ],
            ),
            quick_action(
                "Update reward",
                "edit",
                QuickActionAccent::Info,
                connected,
                "Rewards",
                false,
                "kick.reward.update",
                config([
                    ("reward_id", blank()),
                    ("title", blank()),
                    ("cost", blank()),
                ]),
                vec![
                    text_field_placeholder("reward_id", "Reward ID", "", "%reward_id%").required(),
                    text_field_placeholder(
                        "title",
                        "New Title (optional)",
                        "",
                        "Leave empty to keep current",
                    ),
                    text_field_placeholder(
                        "cost",
                        "New Cost (optional)",
                        "",
                        "Leave empty to keep current",
                    ),
                ],
            ),
            quick_action(
                "Delete reward",
                "trash",
                QuickActionAccent::Danger,
                connected,
                "Rewards",
                true,
                "kick.reward.delete",
                config([("reward_id", blank())]),
                vec![
                    text_field_placeholder("reward_id", "Reward ID", "", "%reward_id%").required(),
                ],
            ),
            quick_action(
                "Accept redemption",
                "check",
                QuickActionAccent::Success,
                connected,
                "Rewards",
                false,
                "kick.reward.redemption_accept",
                config([(
                    "redemption_ids",
                    Variant::String("%redemption_id%".to_owned()),
                )]),
                vec![
                    text_field_placeholder(
                        "redemption_ids",
                        "Redemption ID(s)",
                        "%redemption_id%",
                        "%redemption_id% or id1,id2,id3 (max 25)",
                    )
                    .required(),
                ],
            ),
            quick_action(
                "Reject redemption",
                "x-circle",
                QuickActionAccent::Warning,
                connected,
                "Rewards",
                false,
                "kick.reward.redemption_reject",
                config([(
                    "redemption_ids",
                    Variant::String("%redemption_id%".to_owned()),
                )]),
                vec![
                    text_field_placeholder(
                        "redemption_ids",
                        "Redemption ID(s)",
                        "%redemption_id%",
                        "%redemption_id% or id1,id2,id3 (max 25)",
                    )
                    .required(),
                ],
            ),
            quick_action(
                "Lookup user",
                "user",
                QuickActionAccent::Success,
                connected,
                "Lookups",
                false,
                "kick.lookup.user",
                config([("slug", blank())]),
                vec![text_field_placeholder("slug", "Channel Slug", "", "channel-slug").required()],
            ),
            quick_action(
                "Stream stats",
                "chart-line",
                QuickActionAccent::Info,
                connected,
                "Lookups",
                false,
                "kick.lookup.stream_stats",
                BTreeMap::new(),
                Vec::new(),
            ),
            quick_action(
                "Find category",
                "search",
                QuickActionAccent::Brand,
                connected,
                "Lookups",
                false,
                "kick.lookup.category",
                config([("query", blank())]),
                vec![
                    text_field_placeholder("query", "Search Query", "", "just chatting").required(),
                ],
            ),
        ]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use forge_events::{Event, EventSource};
    use forge_registry::KindPlatformContract;
    use forge_types::{PlatformId, VariantKind};

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

    // Why: the analyzer offers variable completions from output_schema while the action engine
    // interpolates whatever build_arg_stack actually produced. A declared kind that the builder
    // never emits (or a name it never sets) is invisible until a user's action reads a wrong-typed
    // variable at runtime, so the two surfaces are pinned against each other here.
    #[test]
    fn declared_output_schema_kinds_match_the_arg_stack_built_from_an_empty_payload() {
        let mut reg = TriggerRegistry::new();
        register_kick_triggers(&mut reg).unwrap();
        for descriptor in reg.all() {
            let Some(schema) = descriptor.output_schema() else {
                continue;
            };
            let event = Event::new(EventSource::Kick, descriptor.id(), serde_json::json!({}));
            let stack = descriptor.build_arg_stack(&event).snapshot();
            for declared in schema.variables {
                let actual = stack.get(&declared.name).unwrap_or_else(|| {
                    panic!(
                        "'{}' declares '{}' but never sets it",
                        descriptor.id(),
                        declared.name
                    )
                });
                assert_eq!(
                    VariantKind::from_variant(actual),
                    declared.kind,
                    "'{}' declares '{}' as {:?} but emits {actual:?}",
                    descriptor.id(),
                    declared.name,
                    declared.kind,
                );
            }
        }
    }

    #[test]
    fn numeric_trigger_fields_fall_back_to_zero_on_non_numeric_wire_values() {
        let mut reg = TriggerRegistry::new();
        register_kick_triggers(&mut reg).unwrap();
        for (kind_id, field) in [
            ("kick.moderation.banned", "duration_secs"),
            ("kick.channel.hosted", "viewer_count"),
            ("kick.channel.subscribed", "months"),
            ("kick.channel.subscription.gifts", "count"),
        ] {
            let descriptor = reg.get(kind_id).unwrap();
            for wire in [
                serde_json::Value::Null,
                serde_json::json!("300"),
                serde_json::json!(true),
                serde_json::json!([1]),
            ] {
                let event = Event::new(
                    EventSource::Kick,
                    kind_id,
                    serde_json::json!({ field: wire.clone() }),
                );
                assert_eq!(
                    descriptor.build_arg_stack(&event).get(field),
                    Some(&Variant::Int(0)),
                    "'{kind_id}' field '{field}' with wire value {wire}",
                );
            }
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
        use forge_platform_core::{
            BuiltinControl, ControlFailure, PlatformError, RateLimitOutcome, RateLimiter,
        };
        use forge_storage::{CredentialId, CredentialsRepo, StorageError};
        use time::OffsetDateTime;

        use super::super::*;
        use crate::chat_platform::KickPlatform;

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
            async fn acquire(&self, _: u32) -> Result<RateLimitOutcome, PlatformError> {
                Ok(RateLimitOutcome::Granted)
            }
            fn remaining(&self) -> u32 {
                60
            }
            async fn observe_remote_throttle(&self, _: StdDuration) {}
        }

        pub(super) fn bundle_with_viewer_channel()
        -> (Arc<KickIntegrationBundle>, watch::Sender<ViewerReport>) {
            let manager = Arc::new(KickCredentialsManager::new(
                Arc::new(EmptyRepo),
                "test_cid".to_owned(),
                "test_secret".to_owned(),
            ));
            let platform = Arc::new(KickPlatform::new(manager.clone(), Arc::new(GrantLimiter)));
            let (viewer_tx, viewer_rx) = watch::channel(ViewerReport::Absent);
            let (bundle, _) = KickIntegrationBundle::new(
                "test_channel".to_owned(),
                777,
                platform,
                manager,
                Arc::new(GrantLimiter),
                viewer_rx,
            );
            (bundle, viewer_tx)
        }

        pub(super) fn disconnected_bundle() -> Arc<KickIntegrationBundle> {
            bundle_with_viewer_channel().0
        }

        #[test]
        fn live_chat_health_metric_reports_disconnected_with_slug_topic_while_offline() {
            let bundle = disconnected_bundle();
            let ws = &bundle.metrics()[0];
            assert_eq!(ws.label, "Live Chat");
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
        fn send_message_quick_action_is_disabled_while_disconnected() {
            let bundle = disconnected_bundle();
            let action = bundle
                .actions()
                .into_iter()
                .find(|a| a.label == "Send message")
                .unwrap();
            assert!(!action.enabled);
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
        async fn a_viewer_report_updates_the_metric_slot_labelled_viewers() {
            let (bundle, viewer_tx) = bundle_with_viewer_channel();
            let mut health = bundle.stream();

            viewer_tx.send(ViewerReport::Live { count: 5 }).unwrap();

            let delta = tokio::time::timeout(StdDuration::from_secs(5), health.next())
                .await
                .expect("the viewer bridge must publish a health delta")
                .expect("the health stream must stay open");

            assert_eq!(
                bundle.metrics()[usize::from(delta.index)].label,
                "Viewers",
                "the viewer bridge index must address the Viewers metric",
            );
            assert_eq!(
                delta.new_value,
                HealthValue::Text {
                    primary: "5".to_owned(),
                    secondary: Some("live".to_owned()),
                }
            );
        }
    }

    mod roster {
        use std::collections::BTreeSet;

        use forge_platform_core::PlatformError;
        use forge_registry::SubActionRegistry;
        use futures::future::BoxFuture;

        use super::super::*;
        use super::lifecycle::{GrantLimiter, disconnected_bundle};
        use crate::categories::KickCategories;
        use crate::channel::KickChannel;
        use crate::moderation::KickModeration;
        use crate::rewards::KickRewards;
        use crate::send::KickSendChat;
        use crate::sub_actions::{KickSubActionDeps, register_kick_sub_actions};

        fn runner_registry() -> SubActionRegistry {
            let limiter: Arc<dyn RateLimiter> = Arc::new(GrantLimiter);
            let mut registry = SubActionRegistry::new();
            register_kick_sub_actions(
                &mut registry,
                KickSubActionDeps {
                    client: Arc::new(KickSendChat::new(Arc::clone(&limiter))),
                    token_source: Arc::new(|| {
                        Box::pin(async { Ok("tok".to_owned()) })
                            as BoxFuture<'static, Result<String, PlatformError>>
                    }),
                    broadcaster_id_source: Arc::new(|| {
                        Box::pin(async { Ok(1_u64) })
                            as BoxFuture<'static, Result<u64, PlatformError>>
                    }),
                    moderation: Arc::new(KickModeration::new(Arc::clone(&limiter))),
                    channel: Arc::new(KickChannel::new(Arc::clone(&limiter))),
                    rewards: Arc::new(KickRewards::new(Arc::clone(&limiter))),
                    categories: Arc::new(KickCategories::new(limiter)),
                },
            )
            .unwrap();
            registry
        }

        /// A quick action that names an unregistered runner, or presets a key the runner never
        /// reads, silently does nothing when the user clicks it.
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
