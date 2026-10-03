#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::DONATION_RECEIVED_KIND;
use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_registry::SubActionRegistry;
use forge_runtime::sub_action_runners::{CoreLogicIfThenElseRunner, OverlaySendRunner};
use forge_runtime::{
    Catalog, ConditionGate, Config, DonationAudience, DonationOverlayAudience, EventBus,
    NullEventLogRepo, OVERLAY_SEND_KIND_ID, OVERLAY_TARGET_KEY, OverlayFrameSink, OverlayReceivers,
    OverlayServiceCell, OverlayServiceHandle,
};
use forge_storage::action::MockActionRepo;
use forge_storage::settings::MockSettingsRepo;
use forge_storage::trigger_instance::MockTriggerInstanceRepo;
use forge_storage::{
    CatalogRevision, MockOverlayRepo, OverlayConfig, OverlayCredential, OverlayDefinition,
    OverlayId, OverlayRepo, SettingsRepo, StorageError,
};
use forge_types::{
    Action, ActionId, ExecutionMode, LATEST_DONATION_SLOT, NOW_PLAYING_SLOT, PermissionRung,
    PlatformScope, QueueId, SubActionConfig, SubActionStep, TriggerConfig, TriggerInstance,
    TriggerInstanceId, Variant,
};
use time::OffsetDateTime;

const ALERT_KIND: &str = "overlay.alert";
const LATEST_KIND: &str = "overlay.latest";
const BRANCH_KIND: &str = "core.logic.if_then_else";
const THEN_CHAIN_KEY: &str = "then_chain";
const FOLLOW_KIND: &str = "twitch.follow";
const ALERT_BOX: &str = "donation-alert";
const SECOND_BOX: &str = "donation-ticker";
const LAST_DONATION: &str = "last-donation";

const NOBODY: OverlayReceivers = OverlayReceivers {
    sources: 0,
    preview_tabs: 0,
};
const PREVIEW_ONLY: OverlayReceivers = OverlayReceivers {
    sources: 0,
    preview_tabs: 2,
};
const ONE_SOURCE: OverlayReceivers = OverlayReceivers {
    sources: 1,
    preview_tabs: 0,
};

struct ConnectedPages(HashMap<String, OverlayReceivers>);

#[async_trait]
impl OverlayFrameSink for ConnectedPages {
    async fn deliver_content(
        &self,
        _: &OverlayId,
        _: serde_json::Value,
        _: Option<u64>,
    ) -> OverlayReceivers {
        OverlayReceivers::default()
    }

    async fn deliver_reload(&self, _: &OverlayId) {}

    async fn revoke(&self, _: &OverlayId) {}

    async fn receivers(&self, identity: &OverlayId) -> OverlayReceivers {
        self.0.get(identity.as_str()).copied().unwrap_or_default()
    }
}

fn failure() -> StorageError {
    StorageError::Connection {
        reason: "disk gone".to_owned(),
    }
}

fn sub_actions() -> Arc<SubActionRegistry> {
    let mut reg = SubActionRegistry::new();
    reg.register(Box::new(OverlaySendRunner::new(OverlayServiceCell::new())))
        .expect("the overlay send runner registers");
    reg.register(Box::new(CoreLogicIfThenElseRunner::new(Arc::new(
        ConditionGate::new(&Config::default()),
    ))))
    .expect("the branching runner registers");
    Arc::new(reg)
}

fn send(identity: &str) -> SubActionStep {
    let mut config = SubActionConfig::new();
    config.insert(
        OVERLAY_TARGET_KEY.to_owned(),
        Variant::String(identity.to_owned()),
    );
    SubActionStep {
        kind_id: OVERLAY_SEND_KIND_ID.to_owned(),
        config,
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn inside_branch(body: SubActionStep) -> SubActionStep {
    let mut encoded = SubActionConfig::new();
    encoded.insert("kind_id".to_owned(), Variant::String(body.kind_id.clone()));
    encoded.insert("config".to_owned(), Variant::Object(body.config.clone()));
    encoded.insert("enabled".to_owned(), Variant::Bool(true));
    let mut config = SubActionConfig::new();
    config.insert(
        THEN_CHAIN_KEY.to_owned(),
        Variant::Array(vec![Variant::Object(encoded)]),
    );
    SubActionStep {
        kind_id: BRANCH_KIND.to_owned(),
        config,
        ..send("")
    }
}

struct Wiring {
    trigger_kind: &'static str,
    steps: Vec<SubActionStep>,
}

fn on(trigger_kind: &'static str, steps: Vec<SubActionStep>) -> Wiring {
    Wiring {
        trigger_kind,
        steps,
    }
}

fn action(steps: Vec<SubActionStep>) -> Action {
    Action {
        id: ActionId::new(),
        name: "Donation alert".to_owned(),
        group: None,
        queue_id: QueueId::new(),
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: steps,
    }
}

fn trigger(kind_id: &str) -> TriggerInstance {
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: kind_id.to_owned(),
        name: kind_id.to_owned(),
        overrides: TriggerConfig::new(),
        enabled: true,
        user_defined: false,
        platform_scope: PlatformScope::Any,
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::default(),
    }
}

fn catalog(wirings: Vec<Wiring>, readable: bool) -> Arc<Catalog> {
    let seeded: Vec<(Action, TriggerInstance)> = wirings
        .into_iter()
        .map(|wiring| (action(wiring.steps), trigger(wiring.trigger_kind)))
        .collect();
    let actions: Vec<Action> = seeded.iter().map(|(action, _)| action.clone()).collect();
    let instances: HashMap<ActionId, Vec<TriggerInstance>> = seeded
        .into_iter()
        .map(|(action, instance)| (action.id, vec![instance]))
        .collect();

    let mut action_repo = MockActionRepo::new();
    action_repo.expect_list().returning(move || {
        if readable {
            Ok(actions.clone())
        } else {
            Err(failure())
        }
    });
    let mut trigger_repo = MockTriggerInstanceRepo::new();
    trigger_repo
        .expect_list_for_action()
        .returning(move |id| Ok(instances.get(&id).cloned().unwrap_or_default()));
    Catalog::new(
        Arc::new(action_repo),
        Arc::new(trigger_repo),
        CatalogRevision::new(),
    )
}

struct Page {
    id: &'static str,
    kind_id: &'static str,
    slot: Option<&'static str>,
    enabled: bool,
    receivers: OverlayReceivers,
}

const fn alert(id: &'static str, receivers: OverlayReceivers) -> Page {
    Page {
        id,
        kind_id: ALERT_KIND,
        slot: None,
        enabled: true,
        receivers,
    }
}

const fn latest(slot: &'static str, receivers: OverlayReceivers) -> Page {
    Page {
        id: LAST_DONATION,
        kind_id: LATEST_KIND,
        slot: Some(slot),
        enabled: true,
        receivers,
    }
}

const fn switched_off(page: Page) -> Page {
    Page {
        enabled: false,
        ..page
    }
}

fn definition(page: &Page) -> OverlayDefinition {
    let mut config = OverlayConfig::new();
    if let Some(slot) = page.slot {
        config.insert("slot".to_owned(), Variant::String(slot.to_owned()));
    }
    OverlayDefinition {
        id: OverlayId::new(page.id),
        display_name: page.id.to_owned(),
        kind_id: page.kind_id.to_owned(),
        enabled: page.enabled,
        position: 0,
        config,
        config_schema_version: 1,
        generator_version: 0,
        source_overrides: Vec::new(),
        credential: OverlayCredential::new("2f8b1d0c9a7e6f5b4c3d2e1f0a9b8c7d"),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn overlays(pages: &[Page], readable: bool) -> OverlayServiceHandle {
    let definitions: Vec<OverlayDefinition> = pages.iter().map(definition).collect();
    let mut repo = MockOverlayRepo::new();
    repo.expect_list().returning(move || {
        if readable {
            Ok(definitions.clone())
        } else {
            Err(failure())
        }
    });
    let connected = ConnectedPages(
        pages
            .iter()
            .map(|page| (page.id.to_owned(), page.receivers))
            .collect(),
    );
    let mut kinds = OverlayKindRegistry::new();
    register_builtin_kinds(&mut kinds).expect("the builtin overlay kinds register");
    OverlayServiceHandle::new(
        Arc::new(repo) as Arc<dyn OverlayRepo>,
        Arc::new(MockSettingsRepo::new()) as Arc<dyn SettingsRepo>,
        Arc::new(kinds),
        EventBus::new(Arc::new(NullEventLogRepo)),
        Some(Arc::new(connected) as Arc<dyn OverlayFrameSink>),
    )
}

fn audience(wirings: Vec<Wiring>, pages: &[Page]) -> DonationOverlayAudience {
    DonationOverlayAudience::new(catalog(wirings, true), overlays(pages, true), sub_actions())
}

#[tokio::test]
async fn nothing_waits_when_no_enabled_overlay_shows_donations() {
    for (label, wirings, pages) in [
        ("no overlay and no action at all", vec![], vec![]),
        (
            "an alert fed only by a follow trigger",
            vec![on(FOLLOW_KIND, vec![send(ALERT_BOX)])],
            vec![alert(ALERT_BOX, NOBODY)],
        ),
        (
            "a donation step naming its overlay with a variable",
            vec![on(DONATION_RECEIVED_KIND, vec![send("%overlay%")])],
            vec![alert(ALERT_BOX, NOBODY)],
        ),
        (
            "a donation step aimed at a switched-off overlay",
            vec![on(DONATION_RECEIVED_KIND, vec![send(ALERT_BOX)])],
            vec![switched_off(alert(ALERT_BOX, NOBODY))],
        ),
        (
            "a latest widget bound to another slot",
            vec![],
            vec![latest(NOW_PLAYING_SLOT, NOBODY)],
        ),
        (
            "a switched-off latest widget bound to the donation slot",
            vec![],
            vec![switched_off(latest(LATEST_DONATION_SLOT, NOBODY))],
        ),
    ] {
        assert!(audience(wirings, &pages).is_listening().await, "{label}");
    }
}

type OverlayShape = (
    &'static str,
    fn() -> Vec<Wiring>,
    fn(OverlayReceivers) -> Page,
);

#[tokio::test]
async fn a_donation_overlay_listens_only_once_a_browser_source_is_connected() {
    let shapes: [OverlayShape; 3] = [
        (
            "an alert a donation step sends to",
            || vec![on(DONATION_RECEIVED_KIND, vec![send(ALERT_BOX)])],
            |receivers| alert(ALERT_BOX, receivers),
        ),
        (
            "an alert a donation step sends to from inside a branch",
            || {
                vec![on(
                    DONATION_RECEIVED_KIND,
                    vec![inside_branch(send(ALERT_BOX))],
                )]
            },
            |receivers| alert(ALERT_BOX, receivers),
        ),
        (
            "a latest widget bound to the donation slot",
            Vec::new,
            |receivers| latest(LATEST_DONATION_SLOT, receivers),
        ),
    ];
    for (shape, wiring, page) in shapes {
        for (connected, receivers, expected) in [
            ("nothing connected", NOBODY, false),
            ("only preview tabs", PREVIEW_ONLY, false),
            ("one browser source", ONE_SOURCE, true),
        ] {
            assert_eq!(
                audience(wiring(), &[page(receivers)]).is_listening().await,
                expected,
                "{shape} with {connected}"
            );
        }
    }
}

#[tokio::test]
async fn one_connected_donation_overlay_is_enough_among_several() {
    let audience = audience(
        vec![on(
            DONATION_RECEIVED_KIND,
            vec![send(ALERT_BOX), send(SECOND_BOX)],
        )],
        &[
            alert(ALERT_BOX, NOBODY),
            alert(SECOND_BOX, ONE_SOURCE),
            latest(LATEST_DONATION_SLOT, PREVIEW_ONLY),
        ],
    );

    assert!(audience.is_listening().await);
}

#[tokio::test]
async fn an_unreadable_catalog_or_overlay_list_keeps_donations_held() {
    let pages = [latest(LATEST_DONATION_SLOT, ONE_SOURCE)];
    for (label, catalog_readable, overlays_readable) in [
        ("the action list fails", false, true),
        ("the overlay list fails", true, false),
    ] {
        let audience = DonationOverlayAudience::new(
            catalog(Vec::new(), catalog_readable),
            overlays(&pages, overlays_readable),
            sub_actions(),
        );

        assert!(!audience.is_listening().await, "{label}");
    }
}
