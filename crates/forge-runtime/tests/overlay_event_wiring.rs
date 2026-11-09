#![expect(clippy::expect_used, clippy::panic, reason = "test fixtures")]
mod common;

use std::sync::Arc;

use forge_events::Event;
use forge_overlay::config::HEADLINE;
use forge_overlay::kinds::alert::{self, AlertOverlayKind};
use forge_overlay::kinds::blank::BlankOverlayKind;
use forge_overlay::{OverlayConfig, OverlayKindDescriptor};
use forge_registry::{
    EventFilter, KindPlatformContract, SubActionRegistry, TriggerCatalogEntry, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_runtime::{
    ActionsService, OVERLAY_SEND_KIND_ID, OVERLAY_TARGET_KEY, OverlayWiringOutcome,
    OverlayWiringPlan, OverlayWiringPreview, OverlayWiringRecords, OverlayWiringRefusal,
    OverlayWiringRequest, PlannedTrigger, RuntimeError, WAIT_KIND_ID, WAIT_MS_KEY, WiringTarget,
};
use forge_storage::{DataProvider, OverlayDefinition};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, DEFAULT_QUEUE_ID, PermissionRung, PlatformScope, Queue, QueueId,
    SubActionConfig, SubActionStep, TriggerConfig, TriggerInstance, TriggerInstanceId, Variant,
};

const FOLLOW_KIND: &str = "twitch.channel.follow";
const OTHER_KIND: &str = "stub.other_thing";

struct StubTrigger(&'static str);

impl TriggerCatalogEntry for StubTrigger {
    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Followed"
    }

    fn summary(&self) -> &str {
        "Followed"
    }

    fn search_text(&self) -> &str {
        "Followed"
    }

    fn icon_name(&self) -> &str {
        "bell"
    }
}

impl TriggerKindDescriptor for StubTrigger {
    fn id(&self) -> &str {
        self.0
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
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
        None
    }
}

struct Stored {
    provider: Arc<dyn DataProvider>,
    service: ActionsService,
    steps: Arc<SubActionRegistry>,
    overlay: OverlayDefinition,
    _media_root: tempfile::TempDir,
}

async fn stored() -> Stored {
    let media_root = tempfile::tempdir().expect("a temporary media root");
    let backend = SqliteBackend::open_for_test(
        "sqlite::memory:",
        [0x24; 32],
        media_root.path().to_owned(),
        None,
    )
    .await
    .expect("an in-memory backend");
    let provider: Arc<dyn DataProvider> = Arc::new(backend);
    let service = ActionsService::new(
        provider.action_repo(),
        provider.queue_repo(),
        provider.history_repo(),
        provider.trigger_instance_repo(),
    );
    Stored {
        provider,
        service,
        steps: common::core_steps_off_any_runtime(),
        overlay: common::overlay_definition(
            "sub-alert",
            "Sub alert",
            alert::KIND_ID,
            OverlayConfig::new(),
        ),
        _media_root: media_root,
    }
}

fn wait_step() -> SubActionStep {
    SubActionStep {
        kind_id: WAIT_KIND_ID.to_owned(),
        config: SubActionConfig::from([(WAIT_MS_KEY.to_owned(), Variant::Int(250))]),
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
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

fn new_action(name: &str, queue_id: Option<QueueId>) -> WiringTarget {
    WiringTarget::NewAction {
        name: name.to_owned(),
        queue_id,
    }
}

fn kind_ids(action: &Action) -> Vec<&str> {
    action
        .sub_actions
        .iter()
        .map(|step| step.kind_id.as_str())
        .collect()
}

impl Stored {
    async fn preview_with(
        &self,
        kind: &dyn OverlayKindDescriptor,
        target: WiringTarget,
        content: OverlayConfig,
    ) -> Result<OverlayWiringPreview, RuntimeError> {
        self.service
            .plan_overlay_wiring(
                OverlayWiringRequest {
                    overlay: &self.overlay,
                    kind,
                    trigger: &StubTrigger(FOLLOW_KIND),
                    target,
                    content,
                },
                &self.steps,
            )
            .await
    }

    async fn preview(&self, target: WiringTarget) -> Result<OverlayWiringPreview, RuntimeError> {
        self.preview_with(&AlertOverlayKind, target, OverlayConfig::new())
            .await
    }

    async fn plan(&self, target: WiringTarget) -> OverlayWiringPlan {
        match self.preview(target).await.expect("the plan builds") {
            OverlayWiringPreview::Ready(plan) => *plan,
            OverlayWiringPreview::AlreadyWired { .. } => panic!("nothing wired this yet"),
        }
    }

    async fn commit(&self, plan: &OverlayWiringPlan) -> Result<OverlayWiringOutcome, RuntimeError> {
        self.service.commit_overlay_wiring(plan, &self.steps).await
    }

    async fn wired(&self, plan: &OverlayWiringPlan) -> OverlayWiringRecords {
        match self.commit(plan).await.expect("the commit lands") {
            OverlayWiringOutcome::Wired(records) => records,
            OverlayWiringOutcome::AlreadyWired { .. } => panic!("nothing wired this yet"),
        }
    }

    async fn seed_action(&self, steps: Vec<SubActionStep>) -> Action {
        let action = common::sequential_action(ActionId::new(), "Greets", DEFAULT_QUEUE_ID, steps);
        self.provider
            .action_repo()
            .save(&action)
            .await
            .expect("the action is stored");
        action
    }

    async fn link(&self, action: ActionId, kind_id: &str) -> TriggerInstance {
        let instance = instance_of(kind_id);
        let repo = self.provider.trigger_instance_repo();
        repo.save(&instance).await.expect("the instance is stored");
        repo.link_action(action, instance.id)
            .await
            .expect("the link is stored");
        instance
    }

    async fn seed_queue(&self, name: &str) -> Queue {
        let queue = Queue {
            id: QueueId::new(),
            name: name.to_owned(),
            description: String::new(),
            concurrency: std::num::NonZeroU32::MIN,
        };
        self.provider
            .queue_repo()
            .save(&queue)
            .await
            .expect("the queue is stored");
        queue
    }

    async fn action(&self, id: ActionId) -> Action {
        self.provider
            .action_repo()
            .get(id)
            .await
            .expect("the action reads")
            .expect("the action is stored")
    }

    async fn linked(&self, id: ActionId) -> Vec<TriggerInstanceId> {
        self.provider
            .trigger_instance_repo()
            .list_for_action(id)
            .await
            .expect("the links read")
            .into_iter()
            .map(|instance| instance.id)
            .collect()
    }

    async fn queue_count(&self) -> usize {
        self.provider
            .queue_repo()
            .list()
            .await
            .expect("the queues read")
            .len()
    }
}

#[tokio::test]
async fn a_new_action_without_a_chosen_queue_runs_on_the_default_queue_and_creates_none() {
    let stored = stored().await;
    let queues_before = stored.queue_count().await;
    let plan = stored.plan(new_action("Alert on follow", None)).await;

    let records = stored.wired(&plan).await;

    assert_eq!(
        (
            stored.action(records.action_id).await.queue_id,
            stored.queue_count().await,
        ),
        (DEFAULT_QUEUE_ID, queues_before),
    );
}

#[tokio::test]
async fn a_new_action_runs_on_the_queue_the_user_chose() {
    let stored = stored().await;
    let chosen = stored.seed_queue("Alerts").await;
    let plan = stored
        .plan(new_action("Alert on follow", Some(chosen.id)))
        .await;

    let records = stored.wired(&plan).await;

    assert_eq!(stored.action(records.action_id).await.queue_id, chosen.id);
}

#[tokio::test]
async fn a_committed_new_action_is_fired_by_the_trigger_the_plan_shows() {
    let stored = stored().await;
    let plan = stored.plan(new_action("  Alert on follow  ", None)).await;

    let records = stored.wired(&plan).await;

    assert_eq!(
        (
            stored.linked(records.action_id).await,
            stored.action(records.action_id).await.name,
        ),
        (
            vec![plan.trigger().instance().id],
            "Alert on follow".to_owned()
        ),
    );
}

#[tokio::test]
async fn a_plan_whose_target_cannot_be_built_is_refused_naming_why() {
    let stored = stored().await;
    let unknown_queue = QueueId::new();
    let unknown_action = ActionId::new();

    for (target, refusal) in [
        (
            new_action(" \t ", None),
            OverlayWiringRefusal::ActionNameEmpty,
        ),
        (
            new_action("Alert", Some(unknown_queue)),
            OverlayWiringRefusal::QueueMissing {
                queue_id: unknown_queue,
            },
        ),
        (
            WiringTarget::ExistingAction(unknown_action),
            OverlayWiringRefusal::ActionMissing {
                action_id: unknown_action,
            },
        ),
    ] {
        let refused = stored.preview(target.clone()).await;

        assert!(
            matches!(&refused, Err(RuntimeError::OverlayWiringRefused(got)) if *got == refusal),
            "{target:?} gave {refused:?}",
        );
    }
}

#[tokio::test]
async fn an_overlay_kind_that_takes_no_content_from_actions_is_refused() {
    let stored = stored().await;

    let refused = stored
        .preview_with(
            &BlankOverlayKind,
            new_action("Alert", None),
            OverlayConfig::new(),
        )
        .await;

    assert!(matches!(
        refused,
        Err(RuntimeError::OverlayWiringRefused(
            OverlayWiringRefusal::OverlayKindNotEligible { .. }
        ))
    ));
}

#[tokio::test]
async fn the_added_step_keeps_only_the_kinds_content_and_always_targets_the_overlay() {
    let stored = stored().await;
    let content = OverlayConfig::from([
        (HEADLINE.to_owned(), Variant::String("Thanks!".to_owned())),
        ("not_a_content_field".to_owned(), Variant::Int(1)),
        (
            OVERLAY_TARGET_KEY.to_owned(),
            Variant::String("some-other-overlay".to_owned()),
        ),
    ]);

    let preview = stored
        .preview_with(&AlertOverlayKind, new_action("Alert", None), content)
        .await;

    let Ok(OverlayWiringPreview::Ready(plan)) = preview else {
        panic!("the plan did not build: {preview:?}");
    };
    assert_eq!(
        plan.added_step()
            .expect("a new action carries the send step")
            .config,
        SubActionConfig::from([
            (HEADLINE.to_owned(), Variant::String("Thanks!".to_owned())),
            (
                OVERLAY_TARGET_KEY.to_owned(),
                Variant::String(stored.overlay.id.as_str().to_owned()),
            ),
        ]),
    );
}

#[tokio::test]
async fn an_existing_action_gains_one_send_step_after_its_own_steps() {
    let stored = stored().await;
    let action = stored.seed_action(vec![wait_step()]).await;
    let plan = stored.plan(WiringTarget::ExistingAction(action.id)).await;

    stored.wired(&plan).await;

    assert_eq!(
        kind_ids(&stored.action(action.id).await),
        [WAIT_KIND_ID, OVERLAY_SEND_KIND_ID],
    );
}

#[tokio::test]
async fn an_existing_action_without_that_trigger_gets_a_new_trigger_linked() {
    let stored = stored().await;
    let action = stored.seed_action(vec![]).await;
    stored.link(action.id, OTHER_KIND).await;
    let plan = stored.plan(WiringTarget::ExistingAction(action.id)).await;

    let records = stored.wired(&plan).await;

    assert!(records.trigger_created);
    assert!(
        stored
            .linked(action.id)
            .await
            .contains(&plan.trigger().instance().id)
    );
}

#[tokio::test]
async fn an_existing_action_already_fired_by_that_trigger_reuses_it_instead_of_adding_one() {
    let stored = stored().await;
    let action = stored.seed_action(vec![]).await;
    let linked = stored.link(action.id, FOLLOW_KIND).await;
    let plan = stored.plan(WiringTarget::ExistingAction(action.id)).await;

    stored.wired(&plan).await;

    assert!(matches!(plan.trigger(), PlannedTrigger::Reuse(reused) if reused.id == linked.id));
    assert_eq!(stored.linked(action.id).await, [linked.id]);
}

#[tokio::test]
async fn an_action_that_already_sends_to_the_overlay_on_that_trigger_is_reported_as_wired() {
    let stored = stored().await;
    let first = stored.plan(new_action("Alert", None)).await;
    let records = stored.wired(&first).await;

    let again = stored
        .preview(WiringTarget::ExistingAction(records.action_id))
        .await
        .expect("the check answers");

    assert_eq!(
        again,
        OverlayWiringPreview::AlreadyWired {
            action_id: records.action_id
        }
    );
}

#[tokio::test]
async fn committing_a_plan_a_second_time_reports_it_wired_and_adds_nothing() {
    let stored = stored().await;
    let action = stored.seed_action(vec![]).await;
    let plan = stored.plan(WiringTarget::ExistingAction(action.id)).await;
    stored.wired(&plan).await;

    let second = stored
        .commit(&plan)
        .await
        .expect("the second commit answers");

    assert_eq!(
        second,
        OverlayWiringOutcome::AlreadyWired {
            action_id: action.id
        }
    );
    assert_eq!(stored.action(action.id).await.sub_actions.len(), 1);
}

#[tokio::test]
async fn a_trigger_linked_between_plan_and_commit_outdates_the_plan_and_writes_nothing() {
    let stored = stored().await;
    let action = stored.seed_action(vec![]).await;
    let plan = stored.plan(WiringTarget::ExistingAction(action.id)).await;
    let linked_meanwhile = stored.link(action.id, FOLLOW_KIND).await;

    let refused = stored.commit(&plan).await;

    assert!(matches!(
        refused,
        Err(RuntimeError::OverlayWiringRefused(
            OverlayWiringRefusal::PlanOutdated
        ))
    ));
    assert_eq!(
        (
            stored.action(action.id).await.sub_actions.len(),
            stored.linked(action.id).await,
        ),
        (0, vec![linked_meanwhile.id]),
    );
}

#[tokio::test]
async fn a_reused_trigger_unlinked_between_plan_and_commit_outdates_the_plan() {
    let stored = stored().await;
    let action = stored.seed_action(vec![]).await;
    let linked = stored.link(action.id, FOLLOW_KIND).await;
    let plan = stored.plan(WiringTarget::ExistingAction(action.id)).await;
    stored
        .provider
        .trigger_instance_repo()
        .unlink_action(action.id, linked.id)
        .await
        .expect("the link is removed");

    let refused = stored.commit(&plan).await;

    assert!(matches!(
        refused,
        Err(RuntimeError::OverlayWiringRefused(
            OverlayWiringRefusal::PlanOutdated
        ))
    ));
}

#[tokio::test]
async fn edits_to_the_action_between_plan_and_commit_survive_the_commit() {
    let stored = stored().await;
    let action = stored.seed_action(vec![]).await;
    let plan = stored.plan(WiringTarget::ExistingAction(action.id)).await;
    let edited = Action {
        name: "Renamed meanwhile".to_owned(),
        sub_actions: vec![wait_step()],
        ..action.clone()
    };
    stored
        .provider
        .action_repo()
        .save(&edited)
        .await
        .expect("the edit is stored");

    stored.wired(&plan).await;

    let landed = stored.action(action.id).await;
    assert_eq!(
        (landed.name.as_str(), kind_ids(&landed)),
        (
            "Renamed meanwhile",
            vec![WAIT_KIND_ID, OVERLAY_SEND_KIND_ID]
        ),
    );
}

#[tokio::test]
async fn a_queue_deleted_between_plan_and_commit_refuses_and_writes_no_action() {
    let stored = stored().await;
    let chosen = stored.seed_queue("Alerts").await;
    let plan = stored.plan(new_action("Alert", Some(chosen.id))).await;
    stored
        .provider
        .queue_repo()
        .delete(chosen.id)
        .await
        .expect("the queue is removed");

    let refused = stored.commit(&plan).await;

    assert!(matches!(
        refused,
        Err(RuntimeError::OverlayWiringRefused(
            OverlayWiringRefusal::QueueMissing { queue_id }
        )) if queue_id == chosen.id
    ));
    assert_eq!(
        stored
            .provider
            .action_repo()
            .get(plan.action().action().id)
            .await
            .expect("the actions read"),
        None,
    );
}

#[tokio::test]
async fn wiring_a_second_event_into_an_action_already_feeding_the_overlay_adds_no_second_send() {
    let stored = stored().await;
    let first = stored.plan(new_action("Alert", None)).await;
    let records = stored.wired(&first).await;

    let preview = stored
        .service
        .plan_overlay_wiring(
            OverlayWiringRequest {
                overlay: &stored.overlay,
                kind: &AlertOverlayKind,
                trigger: &StubTrigger(OTHER_KIND),
                target: WiringTarget::ExistingAction(records.action_id),
                content: OverlayConfig::new(),
            },
            &stored.steps,
        )
        .await
        .expect("plans");
    let OverlayWiringPreview::Ready(plan) = preview else {
        panic!("the second event was reported as already wired: {preview:?}");
    };
    assert!(plan.added_step().is_none(), "{plan:?}");
    let second = stored.wired(&plan).await;

    assert_eq!(
        kind_ids(&stored.action(records.action_id).await),
        [OVERLAY_SEND_KIND_ID]
    );
    assert!(
        second.trigger_created,
        "the second event's trigger is linked"
    );
}
