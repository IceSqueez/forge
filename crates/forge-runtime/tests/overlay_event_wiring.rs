#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use forge_events::Event;
use forge_overlay::config::DURATION;
use forge_overlay::descriptor::{
    DeliveryDisposition, OverlayConfig, OverlayKindDescriptor, SectionedField,
};
use forge_overlay::kinds::alert::AlertOverlayKind;
use forge_overlay::preview::PreviewComposition;
use forge_overlay::{EventWiringTrigger, PageAssets, suggested_content};
use forge_registry::{
    ActorBlock, ActorIdentity, EventFilter, FormField, KindPlatformContract, LoginSlot,
    SubActionRegistry, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
    declared_variables,
};
use forge_runtime::actions::{
    ActionsService, OVERLAY_ALERT_QUEUE, OverlayWiringError, OverlayWiringOutcome,
    OverlayWiringPlan, OverlayWiringRecords, OverlayWiringRefusal, WiredQueue, plan_overlay_wiring,
};
use forge_runtime::sub_action_runners::{OverlaySendRunner, WAIT_KIND_ID, WAIT_MS_KEY};
use forge_runtime::{OVERLAY_SEND_KIND_ID, OVERLAY_TARGET_KEY, OverlayServiceCell};
use forge_storage::action::MockActionRepo;
use forge_storage::history::MockHistoryRepo;
use forge_storage::queue::MockQueueRepo;
use forge_storage::soundboard::MockSoundboardClipsRepo;
use forge_storage::trigger_instance::MockTriggerInstanceRepo;
use forge_storage::{OverlayCredential, OverlayDefinition, OverlayId, StorageError};
use forge_types::{
    Action, ActionId, ActorRole, ExecutionMode, PermissionRung, PlatformId, PlatformScope, Queue,
    QueueId, SubActionConfig, SubActionStep, TriggerConfig, TriggerInstance, TriggerInstanceId,
    Variant,
};
use time::OffsetDateTime;

const OVERLAY: &str = "sub-alert";
const OVERLAY_NAME: &str = "Sub alert";
const ALERT_KIND: &str = "overlay.alert";
const TUNED_KIND: &str = "overlay.tuned";
const FOLLOW_KIND: &str = "twitch.channel.follow";
const FOLLOW_LABEL: &str = "Followed";
const UNCURATED_KIND: &str = "stub.something_happened";
const UNCURATED_LABEL: &str = "Something happened";
const OTHER_KIND: &str = "stub.other_thing";
const ALERT_DEFAULT_SECS: i64 = 5;

#[derive(Clone, Copy)]
enum Declares {
    Nothing,
    AnEmptyList,
    APrincipal,
}

#[derive(Clone, Copy)]
struct StubTrigger {
    id: &'static str,
    label: &'static str,
    declares: Declares,
}

fn follow() -> StubTrigger {
    StubTrigger {
        id: FOLLOW_KIND,
        label: FOLLOW_LABEL,
        declares: Declares::APrincipal,
    }
}

fn uncurated(declares: Declares) -> StubTrigger {
    StubTrigger {
        id: UNCURATED_KIND,
        label: UNCURATED_LABEL,
        declares,
    }
}

impl TriggerKindDescriptor for StubTrigger {
    fn id(&self) -> &str {
        self.id
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        self.label
    }

    fn summary(&self) -> &str {
        self.label
    }

    fn search_text(&self) -> &str {
        self.label
    }

    fn icon_name(&self) -> &str {
        "bell"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        String::new()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: None,
            kind_prefix: None,
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        match self.declares {
            Declares::Nothing => None,
            Declares::AnEmptyList => Some(TriggerVariables::new()),
            Declares::APrincipal => Some(TriggerVariables::new().actor(
                ActorBlock {
                    role: ActorRole::Principal,
                    platform: PlatformId::Twitch,
                    login: LoginSlot::Declared,
                },
                |_| ActorIdentity {
                    id: "1".to_owned(),
                    display_name: "Someone".to_owned(),
                    login: Some("someone".to_owned()),
                },
            )),
        }
    }
}

struct Tuned {
    disposition: DeliveryDisposition,
    draws: bool,
    reveal_secs: Option<i64>,
}

impl Tuned {
    fn eligible(reveal_secs: Option<i64>) -> Self {
        Self {
            disposition: DeliveryDisposition::Transient,
            draws: true,
            reveal_secs,
        }
    }
}

impl OverlayKindDescriptor for Tuned {
    fn id(&self) -> &str {
        TUNED_KIND
    }

    fn label(&self) -> &str {
        AlertOverlayKind.label()
    }

    fn summary(&self) -> &str {
        AlertOverlayKind.summary()
    }

    fn icon_name(&self) -> &str {
        AlertOverlayKind.icon_name()
    }

    fn delivery_disposition(&self) -> DeliveryDisposition {
        self.disposition
    }

    fn order_sensitive(&self) -> bool {
        AlertOverlayKind.order_sensitive()
    }

    fn config_schema_version(&self) -> u32 {
        AlertOverlayKind.config_schema_version()
    }

    fn default_config(&self) -> OverlayConfig {
        let mut config = AlertOverlayKind.default_config();
        config.remove(DURATION);
        if let Some(secs) = self.reveal_secs {
            config.insert(DURATION.to_owned(), Variant::Int(secs));
        }
        config
    }

    fn config_fields(&self) -> Vec<SectionedField> {
        AlertOverlayKind.config_fields()
    }

    fn page_assets(&self) -> PageAssets {
        AlertOverlayKind.page_assets()
    }

    fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
        AlertOverlayKind.preview(config)
    }

    fn has_visual_page(&self) -> bool {
        self.draws
    }
}

fn definition(kind_id: &str, config: OverlayConfig) -> OverlayDefinition {
    OverlayDefinition {
        id: OverlayId::new(OVERLAY),
        display_name: OVERLAY_NAME.to_owned(),
        kind_id: kind_id.to_owned(),
        enabled: true,
        position: 0,
        config,
        config_schema_version: 2,
        generator_version: 0,
        source_overrides: Vec::new(),
        credential: OverlayCredential::new("2f8b1d0c9a7e6f5b4c3d2e1f0a9b8c7d"),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn reveal_for(secs: i64) -> OverlayConfig {
    OverlayConfig::from([(DURATION.to_owned(), Variant::Int(secs))])
}

fn alert_plan() -> OverlayWiringPlan {
    plan_overlay_wiring(
        &definition(ALERT_KIND, OverlayConfig::new()),
        &AlertOverlayKind,
        &follow(),
    )
    .expect("an alert accepts event wiring")
}

fn kind_ids(steps: &[SubActionStep]) -> Vec<&str> {
    steps.iter().map(|step| step.kind_id.as_str()).collect()
}

#[test]
fn an_overlay_kind_that_takes_no_event_wiring_is_refused_by_the_overlays_kind_id() {
    for (kind, label) in [
        (
            Tuned {
                disposition: DeliveryDisposition::Replace,
                draws: true,
                reveal_secs: Some(ALERT_DEFAULT_SECS),
            },
            "a kind that keeps showing what it was last given",
        ),
        (
            Tuned {
                disposition: DeliveryDisposition::Append,
                draws: true,
                reveal_secs: Some(ALERT_DEFAULT_SECS),
            },
            "a kind that appends what it is given",
        ),
        (
            Tuned {
                disposition: DeliveryDisposition::Transient,
                draws: false,
                reveal_secs: Some(ALERT_DEFAULT_SECS),
            },
            "a kind that draws nothing",
        ),
    ] {
        let refusal = plan_overlay_wiring(
            &definition(TUNED_KIND, OverlayConfig::new()),
            &kind,
            &follow(),
        )
        .expect_err("the kind is not eligible");

        assert_eq!(
            refusal,
            OverlayWiringRefusal::OverlayKindNotEligible {
                overlay_kind_id: TUNED_KIND.to_owned(),
            },
            "{label}",
        );
    }
}

#[test]
fn a_trigger_with_nothing_to_show_is_refused_by_its_own_kind_id() {
    for (declares, label) in [
        (Declares::Nothing, "a trigger that declares no variables"),
        (
            Declares::AnEmptyList,
            "a trigger that declares an empty list",
        ),
    ] {
        let refusal = plan_overlay_wiring(
            &definition(ALERT_KIND, OverlayConfig::new()),
            &AlertOverlayKind,
            &uncurated(declares),
        )
        .expect_err("there is nothing to pre-fill the overlay with");

        assert_eq!(
            refusal,
            OverlayWiringRefusal::TriggerDeclaresNoVariables {
                trigger_kind_id: UNCURATED_KIND.to_owned(),
            },
            "{label}",
        );
    }
}

#[test]
fn an_alert_plan_sends_to_the_overlay_and_then_waits() {
    assert_eq!(
        kind_ids(&alert_plan().action.steps),
        vec![OVERLAY_SEND_KIND_ID, WAIT_KIND_ID],
    );
}

#[test]
fn the_send_targets_the_overlay_being_wired() {
    assert_eq!(
        alert_plan().action.steps[0].config.get(OVERLAY_TARGET_KEY),
        Some(&Variant::String(OVERLAY.to_owned())),
    );
}

#[test]
fn the_send_carries_the_wording_the_overlay_kind_suggests_for_that_trigger() {
    for trigger in [follow(), uncurated(Declares::APrincipal)] {
        let declared = declared_variables(&trigger).expect("the stub declares variables");
        let mut expected = suggested_content(
            &AlertOverlayKind,
            &EventWiringTrigger {
                kind_id: trigger.id,
                label: trigger.label,
                variables: &declared,
            },
        );
        expected.insert(
            OVERLAY_TARGET_KEY.to_owned(),
            Variant::String(OVERLAY.to_owned()),
        );

        let plan = plan_overlay_wiring(
            &definition(ALERT_KIND, OverlayConfig::new()),
            &AlertOverlayKind,
            &trigger,
        )
        .expect("an alert accepts event wiring");

        assert_eq!(plan.action.steps[0].config, expected, "{}", trigger.id);
    }
}

#[test]
fn the_wait_lasts_the_reveal_duration_the_effective_config_resolves() {
    for (stored, expected_ms) in [
        (OverlayConfig::new(), ALERT_DEFAULT_SECS * 1_000),
        (reveal_for(1), 1_000),
        (reveal_for(15), 15_000),
    ] {
        let plan = plan_overlay_wiring(
            &definition(ALERT_KIND, stored.clone()),
            &AlertOverlayKind,
            &follow(),
        )
        .expect("an alert accepts event wiring");

        assert_eq!(
            plan.action.steps[1].config.get(WAIT_MS_KEY),
            Some(&Variant::Int(expected_ms)),
            "{stored:?}",
        );
    }
}

#[test]
fn a_kind_that_declares_no_reveal_duration_gets_no_wait_step() {
    let plan = plan_overlay_wiring(
        &definition(TUNED_KIND, OverlayConfig::new()),
        &Tuned::eligible(None),
        &follow(),
    )
    .expect("the tuned kind accepts event wiring");

    assert_eq!(kind_ids(&plan.action.steps), vec![OVERLAY_SEND_KIND_ID]);
}

#[test]
fn the_action_and_the_trigger_instance_name_the_two_sides_in_opposite_order() {
    let plan = alert_plan();

    assert_eq!(
        (plan.action.name.as_str(), plan.trigger.name.as_str()),
        ("Sub alert on Followed", "Followed for Sub alert"),
    );
}

#[test]
fn the_planned_trigger_instance_is_an_unfiltered_user_defined_instance_of_the_chosen_kind() {
    let plan = alert_plan();

    assert_eq!(
        plan.trigger,
        TriggerInstance {
            id: plan.trigger.id,
            kind_id: FOLLOW_KIND.to_owned(),
            name: plan.trigger.name.clone(),
            overrides: TriggerConfig::new(),
            enabled: true,
            user_defined: true,
            platform_scope: PlatformScope::Any,
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        },
    );
}

#[test]
fn each_plan_mints_its_own_action_and_trigger_identity() {
    let first = alert_plan();
    let second = alert_plan();

    assert_ne!(first.action.id, second.action.id);
    assert_ne!(first.trigger.id, second.trigger.id);
}

#[test]
fn placing_the_planned_action_on_a_queue_keeps_the_identity_and_steps_the_plan_minted() {
    let plan = alert_plan();
    let queue_id = QueueId::new();

    let action = plan.action.on_queue(queue_id);

    assert_eq!(action.id, plan.action.id);
    assert_eq!(action.name, plan.action.name);
    assert_eq!(action.sub_actions, plan.action.steps);
    assert_eq!(action.queue_id, queue_id);
    assert!(
        action.enabled,
        "a wired action must run when its event fires"
    );
    assert_eq!(
        plan.action.on_queue(QueueId::new()).id,
        plan.action.id,
        "placing the plan again must not mint a second action",
    );
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Step {
    Lookup,
    QueueLookup,
    QueueSave,
    ActionSave,
    TriggerSave,
    Link,
}

enum Reach {
    Runs,
    Fails,
    Never,
}

#[derive(Default)]
struct Written {
    order: Vec<&'static str>,
    looked_up: Vec<String>,
    queues: Vec<Queue>,
    actions: Vec<Action>,
    triggers: Vec<TriggerInstance>,
}

struct Harness {
    service: ActionsService,
    written: Arc<Mutex<Written>>,
    fed_action: Option<ActionId>,
}

struct Wiring {
    fed_by: Option<&'static str>,
    existing_queue: Option<Queue>,
    stop: Option<Step>,
    writes: bool,
}

fn fault() -> StorageError {
    StorageError::Connection {
        reason: "the store is unavailable".to_owned(),
    }
}

fn send_step(target: &str) -> SubActionStep {
    SubActionStep {
        kind_id: OVERLAY_SEND_KIND_ID.to_owned(),
        config: SubActionConfig::from([(
            OVERLAY_TARGET_KEY.to_owned(),
            Variant::String(target.to_owned()),
        )]),
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn feeding_action() -> Action {
    Action {
        id: ActionId::new(),
        name: "Shows the alert already".to_owned(),
        group: None,
        queue_id: QueueId::new(),
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![send_step(OVERLAY)],
    }
}

fn instance_of(kind_id: &str) -> TriggerInstance {
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: kind_id.to_owned(),
        name: "already there".to_owned(),
        overrides: TriggerConfig::new(),
        enabled: true,
        user_defined: true,
        platform_scope: PlatformScope::Any,
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    }
}

fn registry() -> SubActionRegistry {
    let mut reg = SubActionRegistry::new();
    reg.register(Box::new(OverlaySendRunner::new(OverlayServiceCell::new())))
        .expect("the overlay send runner registers");
    reg
}

impl Wiring {
    fn new() -> Self {
        Self {
            fed_by: None,
            existing_queue: None,
            stop: None,
            writes: true,
        }
    }

    fn fed_by(mut self, trigger_kind_id: &'static str) -> Self {
        self.fed_by = Some(trigger_kind_id);
        self
    }

    fn with_queue(mut self, queue: Queue) -> Self {
        self.existing_queue = Some(queue);
        self
    }

    fn stopping_at(mut self, step: Step) -> Self {
        self.stop = Some(step);
        self
    }

    fn expecting_no_writes(mut self) -> Self {
        self.writes = false;
        self
    }

    fn reach(&self, step: Step) -> Reach {
        if !self.writes && step >= Step::QueueLookup {
            return Reach::Never;
        }
        match self.stop {
            Some(stop) if step == stop => Reach::Fails,
            Some(stop) if step > stop => Reach::Never,
            _ => Reach::Runs,
        }
    }

    fn build(self) -> Harness {
        let written = Arc::new(Mutex::new(Written::default()));
        let seeded = self
            .fed_by
            .map(|kind| (feeding_action(), instance_of(kind)));
        let fed_action = seeded.as_ref().map(|(action, _)| action.id);
        let listed: Vec<Action> = seeded
            .iter()
            .map(|(action, _)| action.clone())
            .collect::<Vec<_>>();
        let linked = seeded.map(|(action, instance)| (action.id, instance));

        let mut actions = MockActionRepo::new();
        match self.reach(Step::Lookup) {
            Reach::Fails => {
                actions.expect_list().returning(|| Err(fault()));
            }
            _ => {
                actions.expect_list().returning(move || Ok(listed.clone()));
            }
        }
        match self.reach(Step::ActionSave) {
            Reach::Runs => {
                let log = Arc::clone(&written);
                actions.expect_save().returning(move |action| {
                    let mut written = log.lock().expect("the recorder is not poisoned");
                    written.order.push("action");
                    written.actions.push(action.clone());
                    Ok(())
                });
            }
            Reach::Fails => {
                actions.expect_save().returning(|_| Err(fault()));
            }
            Reach::Never => {
                actions.expect_save().times(0);
            }
        }

        let mut queues = MockQueueRepo::new();
        match self.reach(Step::QueueLookup) {
            Reach::Runs => {
                let log = Arc::clone(&written);
                let existing = self.existing_queue.clone();
                queues.expect_get_by_name().returning(move |name| {
                    log.lock()
                        .expect("the recorder is not poisoned")
                        .looked_up
                        .push(name.to_owned());
                    Ok(existing.clone())
                });
            }
            Reach::Fails => {
                queues.expect_get_by_name().returning(|_| Err(fault()));
            }
            Reach::Never => {
                queues.expect_get_by_name().times(0);
            }
        }
        match self.reach(Step::QueueSave) {
            Reach::Runs if self.existing_queue.is_none() => {
                let log = Arc::clone(&written);
                queues.expect_save().returning(move |queue| {
                    let mut written = log.lock().expect("the recorder is not poisoned");
                    written.order.push("queue");
                    written.queues.push(queue.clone());
                    Ok(())
                });
            }
            Reach::Fails => {
                queues.expect_save().returning(|_| Err(fault()));
            }
            _ => {
                queues.expect_save().times(0);
            }
        }

        let mut triggers = MockTriggerInstanceRepo::new();
        triggers
            .expect_list_for_action()
            .returning(move |id| match &linked {
                Some((fed, instance)) if *fed == id => Ok(vec![instance.clone()]),
                _ => Ok(Vec::new()),
            });
        match self.reach(Step::TriggerSave) {
            Reach::Runs => {
                let log = Arc::clone(&written);
                triggers.expect_save().returning(move |instance| {
                    let mut written = log.lock().expect("the recorder is not poisoned");
                    written.order.push("trigger");
                    written.triggers.push(instance.clone());
                    Ok(())
                });
            }
            Reach::Fails => {
                triggers.expect_save().returning(|_| Err(fault()));
            }
            Reach::Never => {
                triggers.expect_save().times(0);
            }
        }
        match self.reach(Step::Link) {
            Reach::Runs => {
                let log = Arc::clone(&written);
                triggers.expect_link_action().returning(move |_, _, _| {
                    log.lock()
                        .expect("the recorder is not poisoned")
                        .order
                        .push("link");
                    Ok(())
                });
            }
            Reach::Fails => {
                triggers
                    .expect_link_action()
                    .returning(|_, _, _| Err(fault()));
            }
            Reach::Never => {
                triggers.expect_link_action().times(0);
            }
        }

        Harness {
            service: ActionsService::new(
                Arc::new(actions),
                Arc::new(queues),
                Arc::new(MockHistoryRepo::new()),
                Arc::new(triggers),
                Arc::new(MockSoundboardClipsRepo::new()),
            ),
            written,
            fed_action,
        }
    }
}

async fn wire(service: &ActionsService) -> Result<OverlayWiringOutcome, OverlayWiringError> {
    service
        .wire_overlay_to_event(
            &definition(ALERT_KIND, OverlayConfig::new()),
            &AlertOverlayKind,
            &follow(),
            &registry(),
        )
        .await
}

async fn records_of(service: &ActionsService) -> OverlayWiringRecords {
    match wire(service).await.expect("the wiring completes") {
        OverlayWiringOutcome::Wired(records) => records,
        OverlayWiringOutcome::AlreadyWired { .. } => {
            panic!("nothing fed this overlay from that trigger kind")
        }
    }
}

#[tokio::test]
async fn an_overlay_already_fed_by_that_trigger_kind_is_reported_without_writing_anything() {
    let harness = Wiring::new()
        .fed_by(FOLLOW_KIND)
        .expecting_no_writes()
        .build();

    let outcome = wire(&harness.service).await.expect("the pre-check answers");

    assert_eq!(
        outcome,
        OverlayWiringOutcome::AlreadyWired {
            action_id: harness.fed_action.expect("an action was seeded"),
        },
    );
}

#[tokio::test]
async fn an_overlay_fed_by_another_trigger_kind_is_wired_onto_the_queue_already_there() {
    let existing = Queue {
        id: QueueId::new(),
        name: OVERLAY_ALERT_QUEUE.name.to_owned(),
        description: "widened by hand".to_owned(),
        concurrency: 8,
    };
    let harness = Wiring::new()
        .fed_by(OTHER_KIND)
        .with_queue(existing.clone())
        .build();

    let records = records_of(&harness.service).await;

    assert_eq!(records.queue, Some(WiredQueue::Existing(existing)));
}

#[tokio::test]
async fn a_queue_found_by_name_is_reused_exactly_as_the_user_left_it() {
    let existing = Queue {
        id: QueueId::new(),
        name: OVERLAY_ALERT_QUEUE.name.to_owned(),
        description: "widened by hand".to_owned(),
        concurrency: 8,
    };
    let harness = Wiring::new().with_queue(existing.clone()).build();

    let records = records_of(&harness.service).await;

    assert_eq!(records.queue, Some(WiredQueue::Existing(existing.clone())));
    assert_eq!(
        harness
            .written
            .lock()
            .expect("the recorder is not poisoned")
            .actions[0]
            .queue_id,
        existing.id,
    );
}

#[tokio::test]
async fn a_missing_queue_is_created_serial_so_two_alerts_never_overlap() {
    let harness = Wiring::new().build();

    let records = records_of(&harness.service).await;

    let created = records
        .queue
        .as_ref()
        .and_then(WiredQueue::created)
        .expect("the queue was created here");
    assert!(created.is_serial());
    assert_eq!(
        harness
            .written
            .lock()
            .expect("the recorder is not poisoned")
            .queues,
        vec![created.clone()],
    );
}

#[tokio::test]
async fn a_created_queue_carries_the_name_the_next_wiring_looks_up() {
    let harness = Wiring::new().build();

    let records = records_of(&harness.service).await;

    let created = records
        .queue
        .as_ref()
        .and_then(WiredQueue::created)
        .expect("the queue was created here");
    assert_eq!(
        vec![created.name.clone()],
        harness
            .written
            .lock()
            .expect("the recorder is not poisoned")
            .looked_up,
    );
}

#[tokio::test]
async fn the_wiring_writes_the_queue_then_the_action_then_the_trigger_then_the_link() {
    let harness = Wiring::new().build();

    records_of(&harness.service).await;

    assert_eq!(
        harness
            .written
            .lock()
            .expect("the recorder is not poisoned")
            .order,
        vec!["queue", "action", "trigger", "link"],
    );
}

#[tokio::test]
async fn the_action_and_the_trigger_instance_that_land_are_the_ones_the_outcome_reports() {
    let harness = Wiring::new().build();

    let records = records_of(&harness.service).await;

    let written = harness
        .written
        .lock()
        .expect("the recorder is not poisoned");
    assert_eq!(records.action_id, Some(written.actions[0].id));
    assert_eq!(records.trigger_instance_id, Some(written.triggers[0].id));
    assert!(records.linked);
}

#[tokio::test]
async fn a_failure_at_each_step_reports_exactly_the_records_that_landed() {
    for (stop, expected) in [
        (Step::Lookup, None),
        (Step::QueueLookup, None),
        (Step::QueueSave, None),
        (Step::ActionSave, Some((true, false, false, false))),
        (Step::TriggerSave, Some((true, true, false, false))),
        (Step::Link, Some((true, true, true, false))),
    ] {
        let harness = Wiring::new().stopping_at(stop).build();

        let landed = match wire(&harness.service)
            .await
            .expect_err("the store failed mid-wiring")
        {
            OverlayWiringError::NothingWritten { .. } => None,
            OverlayWiringError::Incomplete { records, .. } => Some((
                records.queue.is_some(),
                records.action_id.is_some(),
                records.trigger_instance_id.is_some(),
                records.linked,
            )),
            OverlayWiringError::Refused(refusal) => panic!("{refusal}"),
        };

        assert_eq!(landed, expected, "{stop:?}");
    }
}

#[tokio::test]
async fn an_incomplete_wiring_names_the_records_that_landed_in_its_message() {
    let harness = Wiring::new().stopping_at(Step::Link).build();

    let error = wire(&harness.service)
        .await
        .expect_err("the link failed after two writes");

    let OverlayWiringError::Incomplete { records, .. } = &error else {
        panic!("the action and the trigger instance were written")
    };
    let message = error.to_string();
    for id in [
        records.action_id.expect("the action landed").to_string(),
        records
            .trigger_instance_id
            .expect("the trigger instance landed")
            .to_string(),
    ] {
        assert!(message.contains(&id), "{message}");
    }
}
