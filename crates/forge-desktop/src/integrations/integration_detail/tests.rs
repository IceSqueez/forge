use super::*;
use crate::integrations::integration_quick_actions::reachable_quick_actions;
use forge_events::EventPublisher;
use forge_host::integration_sign_out::{
    IntegrationSignOut, SignOutRoute, SignOutRoutes, SlotRebuild,
};
use forge_host::integrations::{ObsInstallSeed, VTubeInstallSeed};
use forge_host::platform_credentials::PlatformCredentialManagers;
use forge_platform_core::CollectionId;
use forge_platform_core::QuickAction;
use forge_platform_core::{
    BuiltinCollections, BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinStatus,
    CapabilityFlags, DetailSection, HeaderAction, HealthMetric, QuickActions, SectionIcon,
};
use forge_registry::TriggerRegistry;
use forge_runtime::EventBus;
use forge_storage::CredentialsRepo;
use forge_types::VerbatimText;
use std::sync::Arc;

struct NoSlots;

#[async_trait::async_trait]
impl SlotRebuild for NoSlots {
    async fn rebuild(&self, _: &forge_types::IntegrationId) {}
}

fn sign_out_over(
    credentials: Arc<dyn CredentialsRepo>,
    managers: PlatformCredentialManagers,
    slots: Arc<dyn SlotRebuild>,
) -> IntegrationSignOut {
    let mut routes = SignOutRoutes::default();
    for platform in [PlatformId::Twitch, PlatformId::YouTube, PlatformId::Kick] {
        routes.register(
            forge_types::IntegrationId::new(platform.as_str()),
            SignOutRoute::platform(platform, managers.clone(), Arc::clone(&credentials)),
        );
    }
    routes.register(
        forge_obs::OBS_INTEGRATION.id,
        SignOutRoute::obs(
            Arc::clone(&credentials),
            ObsInstallSeed::new(forge_obs::SwitchableObsSink::new(), Default::default()),
        ),
    );
    routes.register(
        forge_vtube::VTUBE_INTEGRATION.id,
        SignOutRoute::vtube(
            credentials,
            VTubeInstallSeed::new(forge_vtube::SwitchableVTubeSink::new(), Default::default()),
        ),
    );
    routes.over(slots)
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
    use forge_platform_core::{
        CollectionRevisionSignal, CollectionRevisions, CredentialState, HealthStream,
    };

    use forge_types::{IntegrationId, MessageCode, RecoverMutex, VerbatimText};
    use gpui::TestAppContext;

    use super::action;
    use super::*;
    use crate::integrations::twitch_bot_presence::TwitchBotPresence;
    use crate::integrations::twitch_bot_section::StreamerSignInRequested;
    use crate::test_support::{StubEventLog, StubHistory, runtime, test_backend};
    use forge_components::Presentation;
    use forge_platform_core::HealthDelta;
    use forge_platform_core::HealthMetricId;
    use forge_platform_core::HealthText;
    use forge_platform_core::HealthValue;
    use std::time::Duration;

    pub(super) struct ShiftingActions {
        id: IntegrationId,
        label: Mutex<&'static str>,
        signal: CollectionRevisionSignal,
        refreshes: std::sync::atomic::AtomicUsize,
        version: Mutex<Option<String>>,
        status_signal: CollectionRevisionSignal,
        banner: Mutex<&'static str>,
        content_signal: CollectionRevisionSignal,
        token_expiry: Mutex<Option<std::time::SystemTime>>,
        credential: Mutex<CredentialState>,
        header_actions: Mutex<Vec<HeaderAction>>,
    }

    impl ShiftingActions {
        fn labelled(label: &'static str) -> Arc<Self> {
            Self::for_integration("discord", label)
        }

        pub(super) fn for_integration(id: &str, label: &'static str) -> Arc<Self> {
            Arc::new(Self {
                id: IntegrationId::new(id),
                label: Mutex::new(label),
                signal: CollectionRevisionSignal::new(),
                refreshes: std::sync::atomic::AtomicUsize::new(0),
                version: Mutex::new(None),
                status_signal: CollectionRevisionSignal::new(),
                banner: Mutex::new(""),
                content_signal: CollectionRevisionSignal::new(),
                token_expiry: Mutex::new(None),
                credential: Mutex::new(CredentialState::Valid),
                header_actions: Mutex::new(Vec::new()),
            })
        }

        fn set_version(&self, version: &str) {
            *self.version.lock_recover() = Some(version.to_owned());
            self.status_signal.bump();
        }

        fn set_banner(&self, banner: &'static str) {
            *self.banner.lock_recover() = banner;
            self.content_signal.bump();
        }

        pub(super) fn report_credential(&self, state: CredentialState) {
            *self.credential.lock_recover() = state;
            self.status_signal.bump();
        }

        pub(super) fn offer_header_action(&self, action: HeaderAction) {
            self.header_actions.lock_recover().push(action);
            self.status_signal.bump();
        }

        fn set_token_expiry(&self, expiry: Option<std::time::SystemTime>) {
            *self.token_expiry.lock_recover() = expiry;
            self.status_signal.bump();
        }

        fn relabel(&self, label: &'static str) {
            *self.label.lock_recover() = label;
            self.signal.bump();
        }

        pub(super) fn object(self: &Arc<Self>) -> BuiltinObject {
            BuiltinObject {
                status: self.clone(),
                health: self.clone(),
                content: self.clone(),
                quick: self.clone(),
                control: None,
                collections: None,
                option_source: None,
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
        fn credential_state(&self) -> CredentialState {
            *self.credential.lock_recover()
        }
        fn display_name(&self) -> &str {
            "Discord"
        }
        fn version(&self) -> Option<String> {
            self.version.lock_recover().clone()
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
            self.header_actions.lock_recover().clone()
        }

        fn token_expiry(&self) -> Option<std::time::SystemTime> {
            *self.token_expiry.lock_recover()
        }

        fn status_revisions(&self) -> Option<forge_platform_core::CollectionRevisions> {
            Some(self.status_signal.subscribe())
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
            vec![DetailSection::WarningBanner {
                level: forge_platform_core::BannerLevel::Info,
                title: VerbatimText::new(*self.banner.lock_recover()).into(),
                body: VerbatimText::new("").into(),
                cta: None,
            }]
        }

        fn request_refresh(&self) {
            self.refreshes
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }

        fn content_revisions(&self) -> Option<forge_platform_core::CollectionRevisions> {
            Some(self.content_signal.subscribe())
        }
    }

    impl QuickActions for ShiftingActions {
        fn actions(&self) -> Vec<QuickAction> {
            vec![action(*self.label.lock_recover(), None)]
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
        open_detail_with(cx, rt, object, backend.credentials_repo())
    }

    pub(super) fn open_detail_with(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        object: BuiltinObject,
        credentials: Arc<dyn CredentialsRepo>,
    ) -> Entity<IntegrationDetail> {
        let sign_out = sign_out_over(
            Arc::clone(&credentials),
            PlatformCredentialManagers::default(),
            Arc::new(NoSlots),
        );
        open_detail_over(cx, rt, object, credentials, sign_out)
    }

    pub(super) fn open_detail_over(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        object: BuiltinObject,
        credentials: Arc<dyn CredentialsRepo>,
        sign_out: IntegrationSignOut,
    ) -> Entity<IntegrationDetail> {
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
        });
        let bus = EventBus::new(Arc::new(StubEventLog));
        let engine = crate::test_support::stub_action_engine(Arc::clone(&bus));
        let (backend, _writes) = test_backend();
        let view = cx.new(|cx| {
            IntegrationDetail::new(
                object,
                DetailServices {
                    rt_handle: rt.handle().clone(),
                    action_engine: engine,
                    credentials,
                    settings: backend.settings_repo(),
                    history: Arc::new(StubHistory),
                    trigger_registry: Arc::new(TriggerRegistry::new()),
                    bus: Arc::clone(&bus) as Arc<dyn EventPublisher>,
                    twitch_slot: None,
                    kick_slot: None,
                    youtube_slot: None,
                    twitch_sign_in: None,
                    youtube_sign_in: None,
                    kick_sign_in: None,
                    obs_install_seed: ObsInstallSeed::new(
                        forge_obs::SwitchableObsSink::new(),
                        Default::default(),
                    ),
                    sign_out,
                },
                cx,
            )
        });
        cx.run_until_parked();
        view
    }

    fn shown_labels(cx: &mut TestAppContext, view: &Entity<IntegrationDetail>) -> Vec<String> {
        let panel = view.read_with(cx, |detail, _| detail.quick.clone());
        panel.read_with(cx, |panel, _| {
            panel
                .actions
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

    fn shown_banner(cx: &mut TestAppContext, view: &Entity<IntegrationDetail>) -> Option<String> {
        view.read_with(cx, |detail, _| match detail.shown.sections.as_slice() {
            [DetailSection::WarningBanner { title, .. }] => Some(title.code_or_text()),
            _ => None,
        })
    }

    #[gpui::test]
    fn a_content_revision_rereads_the_sections_and_leaves_the_rest_alone(cx: &mut TestAppContext) {
        let rt = runtime();
        let _enter = rt.enter();
        let builtin = ShiftingActions::labelled("before");
        let view = open_detail(cx, &rt, builtin.object());

        *builtin.label.lock_recover() = "not re-read";
        builtin.set_banner("new rows");
        cx.run_until_parked();

        assert_eq!(shown_banner(cx, &view).as_deref(), Some("new rows"));
        assert_eq!(shown_labels(cx, &view), vec!["before".to_owned()]);
    }

    #[gpui::test]
    fn a_status_revision_reloads_the_shown_status(cx: &mut TestAppContext) {
        let rt = runtime();
        let _enter = rt.enter();
        let builtin = ShiftingActions::labelled("x");
        let view = open_detail(cx, &rt, builtin.object());

        builtin.set_version("31.0");
        cx.run_until_parked();

        let version = view.read_with(cx, |detail, _| detail.shown.version.clone());
        assert_eq!(version.as_deref(), Some("31.0"));
    }

    #[gpui::test]
    fn adoption_moves_the_change_bridge_to_the_new_object(cx: &mut TestAppContext) {
        let rt = runtime();
        let _enter = rt.enter();
        let first = ShiftingActions::labelled("x");
        let view = open_detail(cx, &rt, first.object());
        let second = ShiftingActions::labelled("x");
        view.update(cx, |detail, cx| detail.adopt_builtin(second.object(), cx));

        first.set_banner("retired object");
        first.set_version("retired");
        cx.run_until_parked();
        assert_eq!(shown_banner(cx, &view).as_deref(), Some(""));

        second.set_banner("adopted rows");
        second.set_version("adopted");
        cx.run_until_parked();
        assert_eq!(shown_banner(cx, &view).as_deref(), Some("adopted rows"));
        let version = view.read_with(cx, |detail, _| detail.shown.version.clone());
        assert_eq!(version.as_deref(), Some("adopted"));
    }

    #[gpui::test]
    fn the_countdown_repaint_runs_only_while_a_live_expiry_is_shown(cx: &mut TestAppContext) {
        let rt = runtime();
        let _enter = rt.enter();
        let builtin = ShiftingActions::labelled("x");
        let view = open_detail(cx, &rt, builtin.object());
        let armed = |cx: &mut TestAppContext| {
            view.read_with(cx, |detail, _| {
                detail.background._token_countdown_repaint.is_some()
            })
        };
        assert!(!armed(cx), "no expiry reported");

        let hour = Duration::from_secs(3600);
        builtin.set_token_expiry(Some(std::time::SystemTime::now() + hour));
        cx.run_until_parked();
        assert!(armed(cx), "an expiry is shown");

        builtin.set_token_expiry(Some(std::time::SystemTime::UNIX_EPOCH));
        cx.run_until_parked();
        assert!(!armed(cx), "the shown expiry passed");
    }

    fn shown_metric_values(
        cx: &mut TestAppContext,
        view: &Entity<IntegrationDetail>,
    ) -> Vec<HealthValue> {
        view.read_with(cx, |detail, _| {
            detail
                .shown
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
            detail.shown.health_metrics[1].id = HealthMetricId::VIEWERS;
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
    fn the_section_refresh_asks_the_adopted_integration_to_fetch_again(cx: &mut TestAppContext) {
        let rt = runtime();
        let _enter = rt.enter();
        let opened = ShiftingActions::labelled("opened");
        let view = open_detail(cx, &rt, opened.object());
        let adopted = ShiftingActions::labelled("adopted");
        view.update(cx, |detail, cx| detail.adopt_builtin(adopted.object(), cx));

        view.update(cx, |detail, cx| detail.resync_content(cx));

        let refreshes =
            |builtin: &ShiftingActions| builtin.refreshes.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!((refreshes(&opened), refreshes(&adopted)), (0, 1));
    }

    struct LiveOptionSource(CollectionRevisionSignal);

    #[async_trait::async_trait]
    impl forge_platform_core::BuiltinOptionSource for LiveOptionSource {
        fn option_lists(&self) -> Vec<forge_platform_core::OptionListMetadata> {
            vec![forge_platform_core::OptionListMetadata::new(
                forge_platform_core::OptionListId::new("twitch.live"),
            )]
        }

        fn revisions(&self) -> CollectionRevisions {
            self.0.subscribe()
        }

        async fn resolve(
            &self,
            _: &forge_platform_core::OptionListId,
            _: &std::collections::BTreeMap<String, forge_platform_core::QuickActionFieldValue>,
        ) -> forge_platform_core::CollectionOutcome<Vec<forge_platform_core::OptionEntry>> {
            Ok(Vec::new())
        }
    }

    fn adopted_option_lists(
        cx: &mut TestAppContext,
        view: &Entity<IntegrationDetail>,
    ) -> Option<Vec<forge_platform_core::OptionListMetadata>> {
        view.read_with(cx, |detail, _| {
            detail
                .ports
                .option_source
                .as_ref()
                .map(|source| source.option_lists())
        })
    }

    #[gpui::test]
    fn a_page_opened_signed_out_gets_its_pickers_from_the_object_it_adopts_on_sign_in(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let _enter = rt.enter();
        let signed_out = ShiftingActions::labelled("signed out").object();
        let view = open_detail(cx, &rt, signed_out.clone());
        let mut signed_in = ShiftingActions::labelled("signed in").object();
        signed_in.option_source = Some(Arc::new(LiveOptionSource(CollectionRevisionSignal::new())));
        let declared = signed_in
            .option_source
            .as_ref()
            .map(|source| source.option_lists());

        view.update(cx, |detail, cx| detail.adopt_builtin(signed_in, cx));
        let after_sign_in = adopted_option_lists(cx, &view);
        view.update(cx, |detail, cx| detail.adopt_builtin(signed_out, cx));

        assert_eq!(
            (after_sign_in, adopted_option_lists(cx, &view)),
            (declared, None)
        );
    }

    fn viewer_metric(count: u64) -> HealthDelta {
        HealthDelta {
            metric: HealthMetricId::VIEWERS,
            new_value: HealthValue::Text {
                primary: HealthText::Count(count),
                secondary: None,
            },
        }
    }

    fn open_viewer_page(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
    ) -> Entity<IntegrationDetail> {
        let view = open_detail(cx, rt, ShiftingActions::labelled("idle").object());
        view.update(cx, |detail, _| {
            detail.shown.health_metrics[1].id = HealthMetricId::VIEWERS;
        });
        view
    }

    fn feed(
        cx: &mut TestAppContext,
        view: &Entity<IntegrationDetail>,
        deltas: impl IntoIterator<Item = HealthDelta>,
    ) -> Option<i64> {
        view.update(cx, |detail, cx| {
            for delta in deltas {
                detail.apply_health_delta(delta, cx);
            }
            detail.viewer_delta()
        })
    }

    #[gpui::test]
    fn the_viewer_delta_comes_from_the_pages_own_metric_not_from_other_platforms(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let _enter = rt.enter();
        let twitch = open_viewer_page(cx, &rt);
        let kick = open_viewer_page(cx, &rt);

        let twitch_delta = feed(cx, &twitch, [viewer_metric(100), viewer_metric(130)]);
        let kick_delta = feed(cx, &kick, [viewer_metric(40), viewer_metric(35)]);

        assert_eq!((twitch_delta, kick_delta), (Some(30), Some(-5)));
    }

    #[gpui::test]
    fn the_viewer_delta_ignores_offline_zeroes_and_other_metrics(cx: &mut TestAppContext) {
        let rt = runtime();
        let _enter = rt.enter();
        let view = open_viewer_page(cx, &rt);
        let other = HealthDelta {
            metric: view.read_with(cx, |detail, _| detail.shown.health_metrics[0].id.clone()),
            new_value: HealthValue::Text {
                primary: HealthText::Count(9_999),
                secondary: None,
            },
        };

        let before_any_reading = feed(cx, &view, [viewer_metric(0), other.clone()]);
        let after_stream_end = feed(
            cx,
            &view,
            [
                viewer_metric(50),
                other,
                viewer_metric(0),
                viewer_metric(60),
            ],
        );

        assert_eq!((before_any_reading, after_stream_end), (None, Some(10)));
    }

    #[gpui::test]
    fn a_page_opened_on_a_live_stream_measures_from_the_count_it_opened_on(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let _enter = rt.enter();
        let view = open_viewer_page(cx, &rt);
        view.update(cx, |detail, _| {
            detail.shown.health_metrics[1].value = viewer_metric(70).new_value;
            detail.sample_viewers();
        });

        assert_eq!(feed(cx, &view, [viewer_metric(75)]), Some(5));
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
            backend.credentials_repo(),
            bus as Arc<dyn EventPublisher>,
        );
        let presence = cx.new(|_| TwitchBotPresence::new());
        view.update(cx, |detail, cx| detail.attach_twitch_bot(bot, presence, cx));
        let section = view.read_with(cx, |detail, _| detail.sign_in.twitch_bot.clone());
        let connect_flow = |cx: &mut TestAppContext| {
            view.read_with(cx, |detail, _| {
                detail
                    .sign_in
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
    use forge_types::IntegrationId;
    use gpui::TestAppContext;
    use time::OffsetDateTime;

    use super::super::banners::{CREDENTIAL_RECOVERY_SELECTOR, CredentialRecovery};
    use super::quick_action_bridge::{
        LiveControl, ShiftingActions, open_detail_over, open_detail_with,
    };
    use super::*;
    use crate::test_support::{click_selector, error_toasts, open_under_root, pump, runtime};
    use forge_components::ToastKind;
    use forge_components::Toasts;
    use forge_host::integration_catalog::declarations;
    use forge_platform_core::{ControlFailure, ControlOutcome, CredentialState};
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
        let events = Rc::clone(&signed_out_events);
        let subs = cx.update(|cx| {
            vec![cx.subscribe(&view, move |_, _: &IntegrationSignedOut, _| {
                *events.borrow_mut() += 1;
            })]
        });
        SignedInDetail {
            view,
            signed_out_events,
            _subs: subs,
        }
    }

    fn detail_with_credentials(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        actions: &Arc<ShiftingActions>,
        credentials: Arc<ScriptedCredentials>,
    ) -> SignedInDetail {
        cx.update(|cx| cx.set_global(Toasts::new()));
        let mut object = actions.object();
        object.control = Some(Arc::new(LiveControl));
        let view = open_detail_with(cx, rt, object, credentials as Arc<dyn CredentialsRepo>);
        let signed_out_events = Rc::new(RefCell::new(0));
        let events = Rc::clone(&signed_out_events);
        let subs = cx.update(|cx| {
            vec![cx.subscribe(&view, move |_, _: &IntegrationSignedOut, _| {
                *events.borrow_mut() += 1;
            })]
        });
        SignedInDetail {
            view,
            signed_out_events,
            _subs: subs,
        }
    }

    fn signed_in_detail_over(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        actions: &Arc<ShiftingActions>,
    ) -> SignedInDetail {
        detail_with_credentials(cx, rt, actions, ScriptedCredentials::new(false, None))
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
            (this.ports.control.is_some(), this.sign_in.connect.is_some())
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
        for id in ["twitch", "youtube", "kick", "obs", "vtube"] {
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
        crate::app::i18n::install_language(forge_storage::Language::En);
        let rt = runtime();
        let _enter = rt.enter();
        for id in ["obs", "vtube"] {
            let detail = signed_in_detail(cx, &rt, id, ScriptedCredentials::new(false, None));
            detail.view.update(cx, |this, _| {
                this.ports.control = Some(Arc::new(UnreachableControl));
            });

            confirm_sign_out(cx, &detail);
            settle(cx, &rt);

            let display_name = detail
                .view
                .read_with(cx, |this, _| this.shown.display_name.clone());
            assert_eq!(
                (
                    observe(cx, &detail),
                    warnings(cx)
                        .iter()
                        .map(|toast| toast.replace(crate::test_support::FORMAT_ISOLATES, ""))
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
        detail.view.update(cx, |this, cx| this.sign_out(cx));
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

    fn expected_recovery(id: &str) -> Option<CredentialRecovery> {
        match id {
            "twitch" | "youtube" | "kick" => Some(CredentialRecovery::SignInAgain),
            "obs" | "vtube" | "donatello" | "monobank" => {
                Some(CredentialRecovery::ReplaceCredentials)
            }
            "discord" | "midi" | "hotkey" => None,
            other => panic!("{other} has no expected recovery"),
        }
    }

    #[gpui::test]
    fn each_credential_state_renders_the_recovery_its_declaration_routes_to(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let _enter = rt.enter();
        for declaration in declarations() {
            let id = declaration.id.as_str().to_owned();
            let expected = expected_recovery(&id);
            let actions = ShiftingActions::for_integration(&id, "x");
            let detail = signed_in_detail_over(cx, &rt, &actions);
            let view = detail.view.clone();
            let (_, vcx) = open_under_root(cx, |_| view.clone());

            let mut seen = Vec::new();
            for state in [
                CredentialState::Valid,
                CredentialState::ReauthRequired,
                CredentialState::Rejected,
                CredentialState::Valid,
            ] {
                actions.report_credential(state);
                vcx.run_until_parked();
                seen.push((
                    state,
                    view.read_with(vcx, |this, _| this.needed_credential_recovery()),
                    vcx.debug_bounds(CREDENTIAL_RECOVERY_SELECTOR).is_some(),
                ));
            }

            assert_eq!(
                seen,
                vec![
                    (CredentialState::Valid, None, false),
                    (
                        CredentialState::ReauthRequired,
                        expected,
                        expected.is_some()
                    ),
                    (CredentialState::Rejected, expected, expected.is_some()),
                    (CredentialState::Valid, None, false),
                ],
                "{id}"
            );
        }
    }

    #[gpui::test]
    fn a_page_opened_after_the_credential_failed_still_offers_the_recovery(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let _enter = rt.enter();
        let actions = ShiftingActions::for_integration("twitch", "x");
        actions.report_credential(CredentialState::ReauthRequired);

        let detail = signed_in_detail_over(cx, &rt, &actions);

        assert_eq!(
            detail
                .view
                .read_with(cx, |this, _| this.needed_credential_recovery()),
            Some(CredentialRecovery::SignInAgain)
        );
    }

    #[gpui::test]
    fn the_recovery_button_opens_the_flow_the_page_can_finish(cx: &mut TestAppContext) {
        let rt = runtime();
        let _enter = rt.enter();
        for (id, offers_settings, state, connect_flow, signed_out, settings_modal, deletes) in [
            (
                "twitch",
                false,
                CredentialState::ReauthRequired,
                true,
                0,
                false,
                1,
            ),
            (
                "vtube",
                false,
                CredentialState::Rejected,
                false,
                1,
                false,
                1,
            ),
            ("obs", true, CredentialState::Rejected, false, 0, true, 0),
        ] {
            let credentials = ScriptedCredentials::new(false, None);
            let actions = ShiftingActions::for_integration(id, "x");
            if offers_settings {
                actions.offer_header_action(HeaderAction::Settings);
            }
            actions.report_credential(state);
            let mut object = actions.object();
            object.control = Some(Arc::new(LiveControl));
            cx.update(|cx| cx.set_global(Toasts::new()));
            let view = open_detail_with(
                cx,
                &rt,
                object,
                Arc::clone(&credentials) as Arc<dyn CredentialsRepo>,
            );
            let events = Rc::new(RefCell::new(0));
            let counted = Rc::clone(&events);
            let _sub = cx.update(|cx| {
                cx.subscribe(&view, move |_, _: &IntegrationSignedOut, _| {
                    *counted.borrow_mut() += 1;
                })
            });
            let shown = view.clone();
            let (_, vcx) = open_under_root(cx, |_| shown.clone());

            click_selector(vcx, CREDENTIAL_RECOVERY_SELECTOR);
            vcx.run_until_parked();
            pump(&rt);
            vcx.run_until_parked();

            assert_eq!(
                (
                    view.read_with(vcx, |this, _| this.sign_in.connect.is_some()),
                    *events.borrow(),
                    view.read_with(vcx, |this, _| this.modals.obs_settings_modal.is_some()),
                    credentials.deletes.load(Ordering::SeqCst),
                ),
                (connect_flow, signed_out, settings_modal, deletes),
                "{id}"
            );
        }
    }

    #[gpui::test]
    fn disconnecting_on_rejected_credentials_keeps_the_sign_in_reachable(cx: &mut TestAppContext) {
        let rt = runtime();
        let _enter = rt.enter();
        for (id, connect_flow_open, signed_out_events) in
            [("kick", true, 0), ("obs", false, 1), ("vtube", false, 1)]
        {
            let actions = ShiftingActions::for_integration(id, "x");
            actions.report_credential(CredentialState::Rejected);
            let detail =
                detail_with_credentials(cx, &rt, &actions, ScriptedCredentials::new(false, None));

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

    struct ScriptedRefresh(ControlOutcome);

    #[async_trait]
    impl BuiltinControl for ScriptedRefresh {
        async fn reconnect(&self) -> ControlOutcome {
            Ok(())
        }
        async fn disconnect(&self) -> ControlOutcome {
            Ok(())
        }
        async fn refresh_token(&self) -> ControlOutcome {
            self.0.clone()
        }
    }

    #[gpui::test]
    fn a_refused_refresh_leaves_recovery_to_the_banner_and_any_other_failure_toasts(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let _enter = rt.enter();
        for (id, outcome, toasts) in [
            ("twitch", Err(ControlFailure::Unauthorized), 0),
            ("youtube", Err(ControlFailure::Unauthorized), 0),
            ("kick", Err(ControlFailure::Unauthorized), 0),
            ("twitch", Err(ControlFailure::Transport), 1),
            ("kick", Ok(()), 0),
        ] {
            let actions = ShiftingActions::for_integration(id, "x");
            let mut object = actions.object();
            object.control = Some(Arc::new(ScriptedRefresh(outcome.clone())));
            cx.update(|cx| cx.set_global(Toasts::new()));
            let view = open_detail_with(cx, &rt, object, ScriptedCredentials::new(false, None));
            let shown = view.clone();
            let (_, vcx) = open_under_root(cx, |_| shown.clone());

            view.update_in(vcx, |this, window, cx| {
                this.on_header_action(HeaderAction::RefreshToken, window, cx)
            });
            vcx.run_until_parked();
            pump(&rt);
            vcx.run_until_parked();

            assert_eq!(
                vcx.update(|_, cx| error_toasts(cx)),
                toasts,
                "{id} {outcome:?}"
            );
        }
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

    fn running_kick_supervisor(
        rt: &tokio::runtime::Runtime,
        journal: &Journal,
    ) -> forge_runtime::IntegrationSupervisor<BuiltinObject> {
        let (settings, _writes) = crate::test_support::test_backend();
        let supervisor = forge_runtime::IntegrationSupervisor::launch(
            vec![Arc::new(JournalingKick {
                journal: Arc::clone(journal),
            })
                as Arc<dyn forge_runtime::IntegrationFactory<BuiltinObject>>],
            settings.settings_repo(),
            forge_runtime::IntegrationGate::new(),
            Arc::new(forge_host::integrations::BuiltinRegistry::default()),
            forge_runtime::spawn_live_viewer_aggregator(&forge_net::TaskOwner::new()),
        );
        let slot = supervisor.slot(&IntegrationId::new("kick")).unwrap();
        rt.block_on(slot.set_enabled(true)).unwrap();
        supervisor
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
            let supervisor = running_kick_supervisor(&rt, &journal);
            let managed = Arc::new(JournalingRows {
                journal: Arc::clone(&journal),
                delete_fails,
            });
            let managers = PlatformCredentialManagers {
                kick: Some(forge_host::test_support::kick_manager_over(
                    managed as Arc<dyn CredentialsRepo>,
                )),
                ..PlatformCredentialManagers::default()
            };
            let unmanaged = ScriptedCredentials::new(false, None);
            cx.update(|cx| cx.set_global(Toasts::new()));
            let mut object = ShiftingActions::for_integration("kick", "x").object();
            object.control = Some(Arc::new(LiveControl));
            let unmanaged_repo = Arc::clone(&unmanaged) as Arc<dyn CredentialsRepo>;
            let sign_out =
                sign_out_over(Arc::clone(&unmanaged_repo), managers, Arc::new(supervisor));
            let view = open_detail_over(cx, &rt, object, unmanaged_repo, sign_out);

            view.update(cx, |this, cx| {
                this.request_disconnect(cx);
                this.confirm_disconnect(cx);
            });
            settle(cx, &rt);

            assert_eq!(
                (
                    journal.lock().unwrap().clone(),
                    unmanaged.deletes.load(Ordering::SeqCst),
                    view.read_with(cx, |this, _| this.sign_in.connect.is_some()),
                ),
                (expected, 0, !delete_fails),
                "delete_fails={delete_fails}"
            );
        }
    }
}

mod detail_lifetime {
    use std::sync::Mutex;

    use forge_components::{Density, ThemeId};
    use forge_platform_core::{
        BuiltinContent, BuiltinHealth, BuiltinStatus, CapabilityFlags, ConnectionState,
        DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthMetricId, HealthStream,
        HealthValue, QuickAction, QuickActions,
    };

    use forge_types::{IntegrationId, MessageCode};
    use gpui::TestAppContext;

    use crate::test_support::{runtime, test_backend};
    use forge_components::Presentation;
    use forge_host::integrations::BuiltinObject;

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
        fn credential_state(&self) -> forge_platform_core::CredentialState {
            forge_platform_core::CredentialState::Valid
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

        fn status_revisions(&self) -> Option<forge_platform_core::CollectionRevisions> {
            None
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

        fn content_revisions(&self) -> Option<forge_platform_core::CollectionRevisions> {
            None
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
            status: probe.clone(),
            health: probe.clone(),
            content: probe.clone(),
            quick: probe,
            control: None,
            collections: None,
            option_source: None,
            follow: None,
            ban_list: None,
        };
        let (backend, _writes) = test_backend();
        let view = super::quick_action_bridge::open_detail_with(
            cx,
            &rt,
            object,
            backend.credentials_repo(),
        );
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
