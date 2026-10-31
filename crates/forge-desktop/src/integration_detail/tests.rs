use super::quick_actions::reachable_quick_actions;
use super::*;
use forge_platform_core::CollectionId;
use forge_types::UserText;

use forge_types::VerbatimText;
#[test]
fn subscription_counts_are_tallied_by_topic_kind_never_by_the_shown_label() {
    fn row(kind: &str, error_label: Option<UserText>) -> forge_platform_core::SubscriptionRow {
        forge_platform_core::SubscriptionRow {
            kind: kind.to_owned(),
            name: forge_types::VerbatimText::new(format!("{kind} (shown label)")).into(),
            status: forge_platform_core::SubscriptionStatus::Active,
            version: None,
            event_count: None,
            error_label,
        }
    }
    let mut section = DetailSection::SubscriptionList {
        title: forge_types::VerbatimText::new("EventSub").into(),
        icon: SectionIcon::new("rss"),
        items: vec![
            row("channel.raid", None),
            row("channel.follow", None),
            row(
                "channel.ban",
                Some(forge_types::VerbatimText::new("failed").into()),
            ),
        ],
        footer: None,
        banner: None,
    };
    let tally = HashMap::from([
        ("channel.raid".to_owned(), 3),
        ("channel.ban".to_owned(), 5),
    ]);

    IntegrationDetail::fill_subscription_counts(&mut section, &tally);

    let DetailSection::SubscriptionList { items, .. } = section else {
        panic!("the section kind changed");
    };
    let counts: Vec<Option<u64>> = items.iter().map(|row| row.event_count).collect();
    assert_eq!(counts, vec![Some(3), Some(0), None]);
}

struct DeclaredActions(Vec<QuickAction>);

impl QuickActions for DeclaredActions {
    fn actions(&self) -> Vec<QuickAction> {
        self.0.clone()
    }
}

struct DeclaredCollections(Vec<&'static str>);

#[async_trait::async_trait]
impl BuiltinCollections for DeclaredCollections {
    fn collections(&self) -> Vec<forge_platform_core::CollectionMetadata> {
        self.0
            .iter()
            .map(|id| forge_platform_core::CollectionMetadata {
                id: CollectionId::new(*id),
                label: VerbatimText::new(*id).into(),
                icon: SectionIcon::new("diamond"),
                capacity: None,
                fields: Vec::new(),
                toggles: Vec::new(),
            })
            .collect()
    }

    fn revisions(&self) -> forge_platform_core::CollectionRevisions {
        forge_platform_core::CollectionRevisionSignal::new().subscribe()
    }

    async fn list(
        &self,
        _: &CollectionId,
    ) -> forge_platform_core::CollectionOutcome<Vec<forge_platform_core::CollectionItem>> {
        Err(forge_platform_core::CollectionFailure::Transport)
    }

    async fn create(
        &self,
        _: &CollectionId,
        _: &std::collections::BTreeMap<String, forge_platform_core::QuickActionFieldValue>,
    ) -> forge_platform_core::CollectionOutcome<forge_platform_core::CollectionItem> {
        Err(forge_platform_core::CollectionFailure::Transport)
    }

    async fn update(
        &self,
        _: &CollectionId,
        _: &forge_platform_core::CollectionItemId,
        _: &std::collections::BTreeMap<String, forge_platform_core::QuickActionFieldValue>,
    ) -> forge_platform_core::CollectionOutcome<forge_platform_core::CollectionItem> {
        Err(forge_platform_core::CollectionFailure::Transport)
    }

    async fn delete(
        &self,
        _: &CollectionId,
        _: &forge_platform_core::CollectionItemId,
    ) -> forge_platform_core::CollectionOutcome<()> {
        Err(forge_platform_core::CollectionFailure::Transport)
    }

    async fn set_toggle(
        &self,
        _: &CollectionId,
        _: &forge_platform_core::CollectionItemId,
        _: &str,
        _: bool,
    ) -> forge_platform_core::CollectionOutcome<forge_platform_core::CollectionItem> {
        Err(forge_platform_core::CollectionFailure::Transport)
    }
}

fn action(label: &str, collection: Option<&str>) -> QuickAction {
    QuickAction {
        label: VerbatimText::new(label).into(),
        icon: SectionIcon::new("play"),
        enabled: true,
        locked_reason: None,
        liveness: forge_platform_core::QuickActionLiveness::Unknown,
        group: None,
        destructive: false,
        accent: forge_platform_core::QuickActionAccent::Brand,
        subaction_template: forge_types::SubActionStep {
            kind_id: String::new(),
            config: std::collections::BTreeMap::new(),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        },
        fields: Vec::new(),
        collection: collection.map(CollectionId::new),
    }
}

fn reachable_labels(collections: Option<&DeclaredCollections>) -> Vec<String> {
    let quick = DeclaredActions(vec![
        action("Fulfill redemption", None),
        action("Manage rewards", Some("rewards")),
        action("Manage scenes", Some("scenes")),
    ]);
    reachable_quick_actions(&quick, collections.map(|c| c as &dyn BuiltinCollections))
        .into_iter()
        .map(|action| action.label.code_or_text())
        .collect()
}

#[test]
fn collection_actions_survive_only_when_the_integration_declares_their_collection() {
    let declared = DeclaredCollections(vec!["rewards"]);

    assert_eq!(
        reachable_labels(Some(&declared)),
        vec!["Fulfill redemption".to_owned(), "Manage rewards".to_owned()]
    );
}

#[test]
fn an_integration_without_collections_keeps_only_its_step_actions() {
    assert_eq!(
        reachable_labels(None),
        vec!["Fulfill redemption".to_owned()]
    );
}

mod quick_action_bridge {
    use std::sync::Mutex;

    use forge_components::{Density, ThemeId};
    use forge_platform_core::{CollectionRevisionSignal, CollectionRevisions, HealthStream};
    use forge_registry::SubActionRegistry;
    use forge_runtime::{ActionCancelRegistry, spawn_action_engine, spawn_live_viewer_aggregator};
    use forge_types::MessageCode;
    use gpui::TestAppContext;

    use super::super::*;
    use super::action;
    use crate::platforms::PlatformConnectivity;
    use crate::test_support::{StubActions, StubEventLog, StubHistory, runtime, test_backend};
    use crate::twitch_bot_presence::TwitchBotPresence;
    use crate::twitch_bot_section::StreamerSignInRequested;
    use forge_components::Presentation;
    use forge_platform_core::HealthDelta;
    use forge_platform_core::HealthMetricId;
    use forge_platform_core::HealthValue;
    use forge_types::IntegrationId;
    use forge_types::VerbatimText;
    use std::time::Duration;

    pub(super) struct ShiftingActions {
        id: IntegrationId,
        label: Mutex<&'static str>,
        signal: CollectionRevisionSignal,
    }

    impl ShiftingActions {
        fn labelled(label: &'static str) -> Arc<Self> {
            Self::for_integration("discord", label)
        }

        pub(super) fn for_integration(id: &'static str, label: &'static str) -> Arc<Self> {
            Arc::new(Self {
                id: IntegrationId::new(id),
                label: Mutex::new(label),
                signal: CollectionRevisionSignal::new(),
            })
        }

        fn relabel(&self, label: &'static str) {
            *self.label.lock().unwrap_or_else(|p| p.into_inner()) = label;
            self.signal.bump();
        }

        pub(super) fn object(self: &Arc<Self>) -> BuiltinObject {
            BuiltinObject {
                icon: SectionIcon::new("bug"),
                status: self.clone(),
                health: self.clone(),
                content: self.clone(),
                quick: self.clone(),
                control: None,
                collections: None,
                option_source: None,
                obs_client: None,
                vtube_client: None,
                follow: None,
                ban_list: None,
            }
        }
    }

    pub(super) struct LiveControl;

    #[async_trait::async_trait]
    impl BuiltinControl for LiveControl {
        async fn reconnect(&self) -> forge_platform_core::ControlOutcome {
            Ok(())
        }
        async fn disconnect(&self) -> forge_platform_core::ControlOutcome {
            Ok(())
        }
        async fn refresh_token(&self) -> forge_platform_core::ControlOutcome {
            Ok(())
        }
    }

    impl BuiltinStatus for ShiftingActions {
        fn id(&self) -> &IntegrationId {
            &self.id
        }
        fn display_name(&self) -> &str {
            "Discord"
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

    impl BuiltinHealth for ShiftingActions {
        fn metrics(&self) -> [HealthMetric; 4] {
            std::array::from_fn(|i| HealthMetric {
                id: STUB_METRIC_IDS[i].clone(),
                value: HealthValue::Text {
                    primary: VerbatimText::new("").into(),
                    secondary: None,
                },
            })
        }
        fn stream(&self) -> HealthStream {
            Box::pin(futures_util::stream::pending())
        }
    }

    impl BuiltinContent for ShiftingActions {
        fn sections(&self) -> Vec<DetailSection> {
            Vec::new()
        }
    }

    impl QuickActions for ShiftingActions {
        fn actions(&self) -> Vec<QuickAction> {
            vec![action(
                *self.label.lock().unwrap_or_else(|p| p.into_inner()),
                None,
            )]
        }

        fn revisions(&self) -> Option<CollectionRevisions> {
            Some(self.signal.subscribe())
        }
    }

    fn open_detail(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        object: BuiltinObject,
    ) -> Entity<IntegrationDetail> {
        let (backend, _writes) = test_backend();
        open_detail_with(cx, rt, object, backend)
    }

    pub(super) fn open_detail_with(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        object: BuiltinObject,
        credentials: Arc<dyn CredentialsRepo>,
    ) -> Entity<IntegrationDetail> {
        open_detail_over(cx, rt, object, credentials, None)
    }

    pub(super) fn open_detail_over(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        object: BuiltinObject,
        credentials: Arc<dyn CredentialsRepo>,
        kick_slot: Option<IntegrationSlot<BuiltinObject>>,
    ) -> Entity<IntegrationDetail> {
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
        });
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
                credentials,
                backend as Arc<dyn SettingsRepo>,
                Arc::new(StubHistory),
                Arc::new(TriggerRegistry::new()),
                Arc::clone(&bus) as Arc<dyn EventPublisher>,
                Arc::clone(&bus),
                spawn_live_viewer_aggregator(),
                None,
                kick_slot,
                None,
                ObsInstallSeed::new(forge_obs::SwitchableObsSink::new()),
                VTubeInstallSeed::new(forge_vtube::SwitchableVTubeSink::new()),
                connectivity,
                cx,
            )
        });
        cx.run_until_parked();
        view
    }

    fn shown_labels(cx: &mut TestAppContext, view: &Entity<IntegrationDetail>) -> Vec<String> {
        view.read_with(cx, |detail, _| {
            detail
                .quick_actions
                .iter()
                .map(|action| action.label.code_or_text())
                .collect()
        })
    }

    #[gpui::test]
    fn a_quick_action_revision_bump_reloads_the_shown_quick_actions(cx: &mut TestAppContext) {
        let rt = runtime();
        let _enter = rt.enter();
        let builtin = ShiftingActions::labelled("Start Shield mode");
        let view = open_detail(cx, &rt, builtin.object());

        builtin.relabel("End Shield mode");
        cx.run_until_parked();

        assert_eq!(shown_labels(cx, &view), vec!["End Shield mode".to_owned()]);
    }

    fn shown_metric_values(
        cx: &mut TestAppContext,
        view: &Entity<IntegrationDetail>,
    ) -> Vec<HealthValue> {
        view.read_with(cx, |detail, _| {
            detail
                .health_metrics
                .iter()
                .map(|metric| metric.value.clone())
                .collect()
        })
    }

    fn live_value() -> HealthValue {
        HealthValue::Text {
            primary: VerbatimText::new("42").into(),
            secondary: None,
        }
    }

    #[gpui::test]
    fn a_health_delta_replaces_only_the_metric_it_names(cx: &mut TestAppContext) {
        let rt = runtime();
        let _enter = rt.enter();
        let view = open_detail(cx, &rt, ShiftingActions::labelled("idle").object());
        let mut expected = shown_metric_values(cx, &view);
        expected[2] = live_value();

        view.update(cx, |detail, cx| {
            detail.apply_health_delta(
                HealthDelta {
                    metric: STUB_METRIC_IDS[2].clone(),
                    new_value: live_value(),
                },
                cx,
            );
        });

        assert_eq!(shown_metric_values(cx, &view), expected);
    }

    #[gpui::test]
    fn a_health_delta_for_an_undeclared_metric_leaves_every_metric_unchanged(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let _enter = rt.enter();
        let view = open_detail(cx, &rt, ShiftingActions::labelled("idle").object());
        let before = shown_metric_values(cx, &view);

        view.update(cx, |detail, cx| {
            detail.apply_health_delta(
                HealthDelta {
                    metric: HealthMetricId::new(MessageCode::from_static("stub", "not_declared")),
                    new_value: live_value(),
                },
                cx,
            );
        });

        assert_eq!(shown_metric_values(cx, &view), before);
    }

    #[gpui::test]
    fn the_viewer_trend_lands_on_the_metric_declared_as_viewers(cx: &mut TestAppContext) {
        let rt = runtime();
        let _enter = rt.enter();
        let view = open_detail(cx, &rt, ShiftingActions::labelled("idle").object());

        let augmented = view.update(cx, |detail, _| {
            detail.health_metrics[1].id = HealthMetricId::VIEWERS;
            detail.augmented_health()
        });

        let trends: Vec<bool> = augmented
            .iter()
            .map(|metric| {
                matches!(
                    &metric.value,
                    HealthValue::Text {
                        secondary: Some(_),
                        ..
                    }
                )
            })
            .collect();
        assert_eq!(trends, [false, true, false, false]);
    }

    #[gpui::test]
    fn a_reconnected_integration_reloads_on_its_own_quick_action_revisions(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let _enter = rt.enter();
        let first = ShiftingActions::labelled("first session");
        let view = open_detail(cx, &rt, first.object());
        let second = ShiftingActions::labelled("second session");
        view.update(cx, |detail, cx| detail.adopt_builtin(second.object(), cx));

        second.relabel("second session, shield on");
        cx.run_until_parked();

        assert_eq!(
            shown_labels(cx, &view),
            vec!["second session, shield on".to_owned()]
        );
    }

    #[gpui::test]
    fn a_streamer_sign_in_request_from_the_bot_card_restarts_the_twitch_connect_flow(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let _enter = rt.enter();
        let mut object = ShiftingActions::for_integration("twitch", "x").object();
        object.control = Some(Arc::new(LiveControl));
        let view = open_detail(cx, &rt, object);
        let (backend, _writes) = test_backend();
        let bus = EventBus::new(Arc::new(StubEventLog));
        let bot = crate::test_support::default_twitch_bot(
            backend as Arc<dyn CredentialsRepo>,
            bus as Arc<dyn EventPublisher>,
        );
        let presence = cx.new(|_| TwitchBotPresence::new());
        view.update(cx, |detail, cx| detail.attach_twitch_bot(bot, presence, cx));
        let section = view.read_with(cx, |detail, _| detail.twitch_bot.clone());
        let connect_flow = |cx: &mut TestAppContext| {
            view.read_with(cx, |detail, _| {
                detail
                    .connect
                    .as_ref()
                    .map(|connect| connect.view.entity_id())
            })
        };
        let before = connect_flow(cx);

        if let Some(section) = section {
            section.update(cx, |_, cx| cx.emit(StreamerSignInRequested));
        }
        cx.run_until_parked();
        crate::test_support::pump(&rt);
        cx.run_until_parked();

        let after = connect_flow(cx);
        assert!(
            after.is_some() && after != before,
            "{before:?} -> {after:?}"
        );
    }
}

mod sign_out {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use forge_storage::{CredentialId, StorageError};
    use gpui::TestAppContext;
    use time::OffsetDateTime;

    use super::super::*;
    use super::quick_action_bridge::{
        LiveControl, ShiftingActions, open_detail_over, open_detail_with,
    };
    use crate::test_support::{error_toasts, pump, runtime};
    use forge_components::ToastKind;
    use forge_components::Toasts;
    use forge_types::IntegrationId;
    use std::time::Duration;

    struct ScriptedCredentials {
        delete_fails: bool,
        release: Option<Arc<tokio::sync::Notify>>,
        deletes: AtomicUsize,
    }

    impl ScriptedCredentials {
        fn new(delete_fails: bool, release: Option<Arc<tokio::sync::Notify>>) -> Arc<Self> {
            Arc::new(Self {
                delete_fails,
                release,
                deletes: AtomicUsize::new(0),
            })
        }
    }

    #[async_trait]
    impl CredentialsRepo for ScriptedCredentials {
        async fn store(&self, _: &CredentialId, _: &str) -> Result<(), StorageError> {
            Ok(())
        }
        async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(None)
        }
        async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
            self.deletes.fetch_add(1, Ordering::SeqCst);
            if let Some(release) = &self.release {
                release.notified().await;
            }
            if self.delete_fails {
                Err(StorageError::Connection {
                    reason: "credentials store is read-only".to_owned(),
                })
            } else {
                Ok(true)
            }
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

    #[derive(Debug, PartialEq, Eq)]
    struct SignOutView {
        control_kept: bool,
        connect_flow_open: bool,
        signed_out_events: usize,
        error_toasts: usize,
    }

    struct SignedInDetail {
        view: Entity<IntegrationDetail>,
        signed_out_events: Rc<RefCell<usize>>,
        _subs: Vec<Subscription>,
    }

    fn signed_in_detail(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        id: &'static str,
        credentials: Arc<dyn CredentialsRepo>,
    ) -> SignedInDetail {
        cx.update(|cx| cx.set_global(Toasts::new()));
        let mut object = ShiftingActions::for_integration(id, "x").object();
        object.control = Some(Arc::new(LiveControl));
        let view = open_detail_with(cx, rt, object, credentials);
        let signed_out_events = Rc::new(RefCell::new(0));
        let obs_events = Rc::clone(&signed_out_events);
        let vtube_events = Rc::clone(&signed_out_events);
        let subs = cx.update(|cx| {
            vec![
                cx.subscribe(&view, move |_, _: &ObsSignedOut, _| {
                    *obs_events.borrow_mut() += 1;
                }),
                cx.subscribe(&view, move |_, _: &VTubeSignedOut, _| {
                    *vtube_events.borrow_mut() += 1;
                }),
            ]
        });
        SignedInDetail {
            view,
            signed_out_events,
            _subs: subs,
        }
    }

    struct UnreachableControl;

    #[async_trait]
    impl BuiltinControl for UnreachableControl {
        async fn reconnect(&self) -> forge_platform_core::ControlOutcome {
            Ok(())
        }
        async fn disconnect(&self) -> forge_platform_core::ControlOutcome {
            Err(forge_platform_core::ControlFailure::Transport)
        }
        async fn refresh_token(&self) -> forge_platform_core::ControlOutcome {
            Ok(())
        }
    }

    fn warnings(cx: &mut TestAppContext) -> Vec<String> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .filter(|toast| toast.kind == ToastKind::Warn)
                .map(|toast| toast.message.to_string())
                .collect()
        })
    }

    fn confirm_sign_out(cx: &mut TestAppContext, detail: &SignedInDetail) {
        detail.view.update(cx, |this, cx| {
            this.request_disconnect(cx);
            this.confirm_disconnect(cx);
        });
    }

    fn settle(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
        cx.run_until_parked();
        pump(rt);
        cx.run_until_parked();
    }

    fn observe(cx: &mut TestAppContext, detail: &SignedInDetail) -> SignOutView {
        let (control_kept, connect_flow_open) = detail.view.read_with(cx, |this, _| {
            (this.control.is_some(), this.connect.is_some())
        });
        SignOutView {
            control_kept,
            connect_flow_open,
            signed_out_events: *detail.signed_out_events.borrow(),
            error_toasts: cx.update(|cx| error_toasts(cx)),
        }
    }

    #[gpui::test]
    fn a_failed_credential_delete_keeps_the_integration_signed_in_and_toasts_the_error(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let _enter = rt.enter();
        for id in ["kick", "obs", "vtube"] {
            let detail = signed_in_detail(cx, &rt, id, ScriptedCredentials::new(true, None));

            confirm_sign_out(cx, &detail);
            settle(cx, &rt);

            assert_eq!(
                observe(cx, &detail),
                SignOutView {
                    control_kept: true,
                    connect_flow_open: false,
                    signed_out_events: 0,
                    error_toasts: 1,
                },
                "{id}"
            );
        }
    }

    #[gpui::test]
    fn a_connection_that_will_not_close_warns_and_still_signs_out(cx: &mut TestAppContext) {
        crate::i18n::install_language(forge_storage::Language::En);
        let rt = runtime();
        let _enter = rt.enter();
        for id in ["obs", "vtube"] {
            let detail = signed_in_detail(cx, &rt, id, ScriptedCredentials::new(false, None));
            detail.view.update(cx, |this, _| {
                this.control = Some(Arc::new(UnreachableControl));
            });

            confirm_sign_out(cx, &detail);
            settle(cx, &rt);

            let display_name = detail
                .view
                .read_with(cx, |this, _| this.display_name.clone());
            assert_eq!(
                (
                    observe(cx, &detail),
                    warnings(cx)
                        .iter()
                        .map(|toast| toast.replace(['\u{2068}', '\u{2069}'], ""))
                        .collect::<Vec<_>>()
                ),
                (
                    SignOutView {
                        control_kept: false,
                        connect_flow_open: false,
                        signed_out_events: 1,
                        error_toasts: 0,
                    },
                    vec![format!(
                        "Signed out of {display_name}, but its live connection could not be closed. Restart forge if it stays connected."
                    )]
                ),
                "{id}"
            );
        }
    }

    #[gpui::test]
    fn a_clean_sign_out_warns_about_nothing(cx: &mut TestAppContext) {
        let rt = runtime();
        let _enter = rt.enter();
        let detail = signed_in_detail(cx, &rt, "obs", ScriptedCredentials::new(false, None));

        confirm_sign_out(cx, &detail);
        settle(cx, &rt);

        assert_eq!(warnings(cx), Vec::<String>::new());
    }

    #[gpui::test]
    fn a_forgotten_credential_applies_the_signed_out_state_once(cx: &mut TestAppContext) {
        let rt = runtime();
        let _enter = rt.enter();
        for (id, connect_flow_open, signed_out_events) in
            [("kick", true, 0), ("obs", false, 1), ("vtube", false, 1)]
        {
            let detail = signed_in_detail(cx, &rt, id, ScriptedCredentials::new(false, None));

            confirm_sign_out(cx, &detail);
            settle(cx, &rt);

            assert_eq!(
                observe(cx, &detail),
                SignOutView {
                    control_kept: false,
                    connect_flow_open,
                    signed_out_events,
                    error_toasts: 0,
                },
                "{id}"
            );
        }
    }

    #[gpui::test]
    fn sign_out_requests_while_one_is_pending_are_ignored(cx: &mut TestAppContext) {
        let rt = runtime();
        let _enter = rt.enter();
        let release = Arc::new(tokio::sync::Notify::new());
        let credentials = ScriptedCredentials::new(false, Some(Arc::clone(&release)));
        let detail = signed_in_detail(
            cx,
            &rt,
            "obs",
            Arc::clone(&credentials) as Arc<dyn CredentialsRepo>,
        );

        confirm_sign_out(cx, &detail);
        settle(cx, &rt);
        confirm_sign_out(cx, &detail);
        detail.view.update(cx, |this, cx| this.sign_out_obs(cx));
        settle(cx, &rt);
        release.notify_one();
        settle(cx, &rt);

        assert_eq!(
            (
                credentials.deletes.load(Ordering::SeqCst),
                *detail.signed_out_events.borrow()
            ),
            (1, 1)
        );
    }

    type Journal = Arc<std::sync::Mutex<Vec<&'static str>>>;

    struct JournalingKick {
        journal: Journal,
    }

    #[async_trait]
    impl forge_runtime::IntegrationFactory<BuiltinObject> for JournalingKick {
        fn id(&self) -> IntegrationId {
            IntegrationId::new("kick")
        }
        fn teardown_grace(&self) -> Duration {
            Duration::from_millis(1)
        }
        async fn is_configured(&self) -> Result<bool, StorageError> {
            Ok(true)
        }
        async fn start(
            &self,
        ) -> Result<
            forge_runtime::RunningIntegration<BuiltinObject>,
            forge_runtime::IntegrationStartError,
        > {
            self.journal.lock().unwrap().push("start");
            let journal = Arc::clone(&self.journal);
            Ok(
                forge_runtime::RunningIntegration::idle().with_teardown(Box::pin(async move {
                    journal.lock().unwrap().push("teardown");
                })),
            )
        }
    }

    struct JournalingRows {
        journal: Journal,
        delete_fails: bool,
    }

    #[async_trait]
    impl CredentialsRepo for JournalingRows {
        async fn store(&self, _: &CredentialId, _: &str) -> Result<(), StorageError> {
            Ok(())
        }
        async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(None)
        }
        async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
            self.journal.lock().unwrap().push("forget");
            if self.delete_fails {
                Err(StorageError::Connection {
                    reason: "credentials store is read-only".to_owned(),
                })
            } else {
                Ok(true)
            }
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

    fn running_kick_slot(
        rt: &tokio::runtime::Runtime,
        journal: &Journal,
    ) -> IntegrationSlot<BuiltinObject> {
        let (settings, _writes) = crate::test_support::test_backend();
        let supervisor = forge_runtime::IntegrationSupervisor::launch(
            vec![Arc::new(JournalingKick {
                journal: Arc::clone(journal),
            })
                as Arc<dyn forge_runtime::IntegrationFactory<BuiltinObject>>],
            settings as Arc<dyn SettingsRepo>,
            forge_runtime::IntegrationGate::new(),
            Arc::new(crate::integrations::BuiltinRegistry::default()),
            forge_runtime::spawn_live_viewer_aggregator(),
        );
        let slot = supervisor.slot(&IntegrationId::new("kick")).unwrap();
        rt.block_on(slot.set_enabled(true)).unwrap();
        slot
    }

    #[gpui::test]
    fn kick_sign_out_forgets_through_the_wired_manager_before_rebuilding(cx: &mut TestAppContext) {
        let rt = runtime();
        let _enter = rt.enter();
        for (delete_fails, expected) in [
            (false, vec!["start", "forget", "teardown", "start"]),
            (true, vec!["start", "forget"]),
        ] {
            let journal: Journal = Arc::default();
            let slot = running_kick_slot(&rt, &journal);
            let managed = Arc::new(JournalingRows {
                journal: Arc::clone(&journal),
                delete_fails,
            });
            let managers = PlatformCredentialManagers {
                kick: Some(crate::platform_credentials::tests::kick_manager_over(
                    managed as Arc<dyn CredentialsRepo>,
                )),
                ..PlatformCredentialManagers::default()
            };
            let unmanaged = ScriptedCredentials::new(false, None);
            cx.update(|cx| cx.set_global(Toasts::new()));
            let mut object = ShiftingActions::for_integration("kick", "x").object();
            object.control = Some(Arc::new(LiveControl));
            let view = open_detail_over(
                cx,
                &rt,
                object,
                Arc::clone(&unmanaged) as Arc<dyn CredentialsRepo>,
                Some(slot),
            );
            view.update(cx, |this, _| this.attach_platform_credentials(managers));

            view.update(cx, |this, cx| {
                this.request_disconnect(cx);
                this.confirm_disconnect(cx);
            });
            settle(cx, &rt);

            assert_eq!(
                (
                    journal.lock().unwrap().clone(),
                    unmanaged.deletes.load(Ordering::SeqCst),
                    view.read_with(cx, |this, _| this.connect.is_some()),
                ),
                (expected, 0, !delete_fails),
                "delete_fails={delete_fails}"
            );
        }
    }
}
