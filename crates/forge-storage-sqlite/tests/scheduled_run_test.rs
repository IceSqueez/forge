#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use forge_storage::{
    DataProvider, MissedRunPolicy, ScheduledRun, ScheduledRunId, ScheduledRunOutcome,
    ScheduledRunRepo, ScheduledRunSpec, ScheduledRunState, StorageError,
};
use forge_storage_sqlite::{SqliteBackend, SqlitePools, apply_migrations, connect_pools};
use forge_types::{Action, ActionId, EventId, Variant};
use time::OffsetDateTime;

mod common;
use common::{Sandboxed, TEST_KEY};

const BASE_UNIX_SECS: i64 = 1_791_115_200;

async fn setup() -> Sandboxed<SqliteBackend> {
    common::sandboxed_backend("sqlite::memory:", TEST_KEY).await
}

fn at_ms(offset_ms: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(BASE_UNIX_SECS).unwrap()
        + time::Duration::milliseconds(offset_ms)
}

fn at_min(offset_min: i64) -> OffsetDateTime {
    at_ms(offset_min * 60_000)
}

fn spec(target: ActionId, due_min: i64, key: Option<&str>) -> ScheduledRunSpec {
    ScheduledRunSpec {
        target_action_id: target,
        due_at: at_min(due_min),
        key: key.map(str::to_owned),
        missed_run_policy: MissedRunPolicy::RunLateOnce,
        args: BTreeMap::new(),
        scheduled_by_action: None,
        scheduled_by_run: None,
        trigger_event_id: None,
        scheduled_at: at_min(0),
        label: "follow-up".to_owned(),
    }
}

async fn place(repo: &Arc<dyn ScheduledRunRepo>, spec: &ScheduledRunSpec) -> ScheduledRunId {
    repo.schedule(spec).await.unwrap().id
}

async fn fetch(repo: &Arc<dyn ScheduledRunRepo>, id: ScheduledRunId) -> ScheduledRun {
    repo.get(id).await.unwrap().expect("the run exists")
}

async fn pending_ids(repo: &Arc<dyn ScheduledRunRepo>) -> Vec<ScheduledRunId> {
    repo.list_pending()
        .await
        .unwrap()
        .into_iter()
        .map(|run| run.id)
        .collect()
}

async fn resolved(
    repo: &Arc<dyn ScheduledRunRepo>,
    outcome: ScheduledRunOutcome,
    resolved_ms: i64,
) -> ScheduledRunId {
    let id = place(repo, &spec(ActionId::new(), 1, None)).await;
    assert!(
        repo.settle(id, outcome, None, at_ms(resolved_ms))
            .await
            .unwrap()
    );
    id
}

#[tokio::test]
async fn scheduling_with_a_key_cancels_the_pending_run_holding_that_key() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let target = ActionId::new();
    let first = place(&repo, &spec(target, 10, Some("vip:alice"))).await;
    let mut replacement = spec(target, 20, Some("vip:alice"));
    replacement.scheduled_at = at_min(5);

    let placement = repo.schedule(&replacement).await.unwrap();

    let old = fetch(&repo, first).await;
    assert_eq!(
        (
            placement.superseded,
            old.state,
            old.outcome_reason.as_deref(),
            old.resolved_at,
            pending_ids(&repo).await,
        ),
        (
            Some(first),
            ScheduledRunState::Cancelled,
            Some("superseded"),
            Some(at_min(5)),
            vec![placement.id],
        )
    );
}

#[tokio::test]
async fn a_key_held_only_by_resolved_runs_does_not_supersede() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let target = ActionId::new();
    let claimed = place(&repo, &spec(target, 10, Some("vip:alice"))).await;
    repo.claim(claimed, at_min(10)).await.unwrap();

    let placement = repo
        .schedule(&spec(target, 20, Some("vip:alice")))
        .await
        .unwrap();

    assert_eq!(
        (placement.superseded, fetch(&repo, claimed).await.state),
        (None, ScheduledRunState::Dispatched)
    );
}

#[tokio::test]
async fn runs_without_a_shared_key_never_supersede_each_other() {
    for (first_key, second_key) in [
        (None, None),
        (Some("a"), Some("b")),
        (Some("a"), None),
        (None, Some("a")),
    ] {
        let backend = setup().await;
        let repo = backend.scheduled_run_repo();
        let target = ActionId::new();
        place(&repo, &spec(target, 10, first_key)).await;

        let placement = repo.schedule(&spec(target, 10, second_key)).await.unwrap();

        assert_eq!(
            (placement.superseded, repo.count_pending().await.unwrap()),
            (None, 2),
            "{first_key:?} then {second_key:?}"
        );
    }
}

#[tokio::test]
async fn a_rejected_schedule_leaves_the_keyed_pending_run_in_place() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let target = ActionId::new();
    let held = place(&repo, &spec(target, 10, Some("vip:alice"))).await;
    let mut oversized = spec(target, 20, Some("vip:alice"));
    oversized.missed_run_policy = MissedRunPolicy::SkipIfLateBy(Duration::MAX);

    let result = repo.schedule(&oversized).await;

    assert!(
        matches!(
            &result,
            Err(StorageError::ValidationFailed { field, .. }) if field == "missed_run_policy"
        ),
        "{result:?}"
    );
    assert_eq!(pending_ids(&repo).await, vec![held]);
}

#[tokio::test]
async fn concurrent_claims_on_one_run_yield_exactly_one_winner() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let id = place(&repo, &spec(ActionId::new(), 10, None)).await;

    let (left, right) = tokio::join!(repo.claim(id, at_min(10)), repo.claim(id, at_min(10)));

    let winners = [left.unwrap(), right.unwrap()]
        .into_iter()
        .flatten()
        .map(|run| (run.id, run.state, run.resolved_at))
        .collect::<Vec<_>>();
    assert_eq!(
        winners,
        vec![(id, ScheduledRunState::Dispatched, Some(at_min(10)))]
    );
}

#[tokio::test]
async fn claim_takes_a_pending_run_before_it_is_due() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let id = place(&repo, &spec(ActionId::new(), 60, None)).await;

    let claimed = repo.claim(id, at_min(1)).await.unwrap();

    assert_eq!(
        claimed.map(|run| run.state),
        Some(ScheduledRunState::Dispatched)
    );
}

#[tokio::test]
async fn claim_refuses_runs_that_are_not_pending_and_leaves_them_unchanged() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let dispatched = place(&repo, &spec(ActionId::new(), 1, None)).await;
    repo.claim(dispatched, at_min(1)).await.unwrap();
    let cancelled = place(&repo, &spec(ActionId::new(), 1, None)).await;
    repo.cancel(cancelled, at_min(1)).await.unwrap();
    let skipped = resolved(&repo, ScheduledRunOutcome::Skipped, 0).await;

    for (id, state) in [
        (dispatched, ScheduledRunState::Dispatched),
        (cancelled, ScheduledRunState::Cancelled),
        (skipped, ScheduledRunState::Skipped),
    ] {
        let before = fetch(&repo, id).await;
        assert_eq!(repo.claim(id, at_min(30)).await.unwrap(), None, "{state:?}");
        assert_eq!(fetch(&repo, id).await, before, "{state:?}");
    }
    assert_eq!(
        repo.claim(ScheduledRunId::new(i64::MAX), at_min(30))
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn pending_runs_list_by_due_time_then_insertion_order_without_resolved_runs() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let target = ActionId::new();
    let late = place(&repo, &spec(target, 30, None)).await;
    let tie_first = place(&repo, &spec(target, 10, None)).await;
    let middle = place(&repo, &spec(target, 20, None)).await;
    let tie_second = place(&repo, &spec(target, 10, None)).await;
    let gone = place(&repo, &spec(target, 5, None)).await;
    repo.cancel(gone, at_min(1)).await.unwrap();

    assert_eq!(
        pending_ids(&repo).await,
        vec![tie_first, tie_second, middle, late]
    );
}

#[tokio::test]
async fn list_due_includes_a_run_due_exactly_now_and_nothing_later() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let target = ActionId::new();
    let mut on_time = spec(target, 0, None);
    on_time.due_at = at_ms(1_000);
    let mut one_ms_later = spec(target, 0, None);
    one_ms_later.due_at = at_ms(1_001);
    let on_time = place(&repo, &on_time).await;
    place(&repo, &one_ms_later).await;

    for (now, expected) in [(at_ms(999), vec![]), (at_ms(1_000), vec![on_time])] {
        let due: Vec<_> = repo
            .list_due(now)
            .await
            .unwrap()
            .into_iter()
            .map(|run| run.id)
            .collect();
        assert_eq!(due, expected, "now = {now}");
    }
}

#[tokio::test]
async fn list_due_skips_runs_that_are_due_but_resolved() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let claimed = place(&repo, &spec(ActionId::new(), 1, None)).await;
    repo.claim(claimed, at_min(1)).await.unwrap();
    let pending = place(&repo, &spec(ActionId::new(), 2, None)).await;

    let due: Vec<_> = repo
        .list_due(at_min(5))
        .await
        .unwrap()
        .into_iter()
        .map(|run| run.id)
        .collect();

    assert_eq!(due, vec![pending]);
}

#[tokio::test]
async fn next_due_is_none_without_pending_runs() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let claimed = place(&repo, &spec(ActionId::new(), 1, None)).await;
    repo.claim(claimed, at_min(1)).await.unwrap();

    assert_eq!(repo.next_due().await.unwrap(), None);
}

#[tokio::test]
async fn next_due_is_the_earliest_pending_due_time_ignoring_resolved_runs() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let target = ActionId::new();
    let earliest = place(&repo, &spec(target, 1, None)).await;
    repo.cancel(earliest, at_min(0)).await.unwrap();
    place(&repo, &spec(target, 30, None)).await;
    place(&repo, &spec(target, 15, None)).await;

    assert_eq!(repo.next_due().await.unwrap(), Some(at_min(15)));
}

#[tokio::test]
async fn cancel_moves_a_pending_run_to_cancelled() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let id = place(&repo, &spec(ActionId::new(), 10, None)).await;

    let cancelled = repo.cancel(id, at_min(3)).await.unwrap();

    let run = fetch(&repo, id).await;
    assert_eq!(
        (
            cancelled,
            run.state,
            run.outcome_reason.as_deref(),
            run.resolved_at
        ),
        (
            true,
            ScheduledRunState::Cancelled,
            Some("cancelled"),
            Some(at_min(3))
        )
    );
}

#[tokio::test]
async fn cancel_refuses_runs_that_are_not_pending() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let dispatched = place(&repo, &spec(ActionId::new(), 1, None)).await;
    repo.claim(dispatched, at_min(1)).await.unwrap();
    let cancelled = place(&repo, &spec(ActionId::new(), 1, None)).await;
    repo.cancel(cancelled, at_min(1)).await.unwrap();
    let failed = resolved(&repo, ScheduledRunOutcome::Failed, 0).await;

    for id in [dispatched, cancelled, failed] {
        let before = fetch(&repo, id).await;
        assert!(!repo.cancel(id, at_min(30)).await.unwrap(), "{id:?}");
        assert_eq!(fetch(&repo, id).await, before);
    }
    assert!(
        !repo
            .cancel(ScheduledRunId::new(i64::MAX), at_min(30))
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn cancel_by_key_cancels_the_pending_run_and_spares_resolved_runs_with_that_key() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let target = ActionId::new();
    let dispatched = place(&repo, &spec(target, 1, Some("vip:alice"))).await;
    repo.claim(dispatched, at_min(1)).await.unwrap();
    let pending = place(&repo, &spec(target, 10, Some("vip:alice"))).await;

    let cancelled = repo.cancel_by_key("vip:alice", at_min(2)).await.unwrap();

    assert_eq!(
        (
            cancelled,
            fetch(&repo, pending).await.state,
            fetch(&repo, dispatched).await.state,
        ),
        (
            true,
            ScheduledRunState::Cancelled,
            ScheduledRunState::Dispatched
        )
    );
}

#[tokio::test]
async fn cancel_by_key_reports_false_when_no_pending_run_holds_the_key() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let target = ActionId::new();
    let dispatched = place(&repo, &spec(target, 1, Some("vip:alice"))).await;
    repo.claim(dispatched, at_min(1)).await.unwrap();
    place(&repo, &spec(target, 10, None)).await;

    for key in ["vip:alice", "vip:bob", ""] {
        assert!(
            !repo.cancel_by_key(key, at_min(2)).await.unwrap(),
            "{key:?}"
        );
    }
    assert_eq!(repo.count_pending().await.unwrap(), 1);
}

#[tokio::test]
async fn settle_records_each_outcome_with_its_reason() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    for (outcome, state) in [
        (
            ScheduledRunOutcome::Dispatched,
            ScheduledRunState::Dispatched,
        ),
        (ScheduledRunOutcome::Cancelled, ScheduledRunState::Cancelled),
        (ScheduledRunOutcome::Skipped, ScheduledRunState::Skipped),
        (ScheduledRunOutcome::Failed, ScheduledRunState::Failed),
    ] {
        for reason in [None, Some("action disabled".to_owned())] {
            let id = place(&repo, &spec(ActionId::new(), 1, None)).await;

            let settled = repo
                .settle(id, outcome, reason.clone(), at_min(7))
                .await
                .unwrap();

            let run = fetch(&repo, id).await;
            assert_eq!(
                (settled, run.state, run.outcome_reason, run.resolved_at),
                (true, state, reason, Some(at_min(7))),
                "{outcome:?}"
            );
        }
    }
}

#[tokio::test]
async fn settle_turns_a_claimed_run_into_a_failure() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let id = place(&repo, &spec(ActionId::new(), 1, None)).await;
    repo.claim(id, at_min(1)).await.unwrap();

    let settled = repo
        .settle(
            id,
            ScheduledRunOutcome::Failed,
            Some("queue refused".to_owned()),
            at_min(2),
        )
        .await
        .unwrap();

    let run = fetch(&repo, id).await;
    assert_eq!(
        (settled, run.state, run.outcome_reason.as_deref()),
        (true, ScheduledRunState::Failed, Some("queue refused"))
    );
}

#[tokio::test]
async fn settle_refuses_terminal_and_missing_runs() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let cancelled = place(&repo, &spec(ActionId::new(), 1, None)).await;
    repo.cancel(cancelled, at_min(1)).await.unwrap();
    let skipped = resolved(&repo, ScheduledRunOutcome::Skipped, 0).await;
    let failed = resolved(&repo, ScheduledRunOutcome::Failed, 0).await;

    for id in [cancelled, skipped, failed] {
        let before = fetch(&repo, id).await;
        let settled = repo
            .settle(
                id,
                ScheduledRunOutcome::Dispatched,
                Some("late".to_owned()),
                at_min(9),
            )
            .await
            .unwrap();
        assert!(!settled, "{:?}", before.state);
        assert_eq!(fetch(&repo, id).await, before);
    }
    assert!(
        !repo
            .settle(
                ScheduledRunId::new(i64::MAX),
                ScheduledRunOutcome::Failed,
                None,
                at_min(9),
            )
            .await
            .unwrap()
    );
}

fn every_variant_kind() -> BTreeMap<String, Variant> {
    let nested = BTreeMap::from([("inner".to_owned(), Variant::Int(-1))]);
    BTreeMap::from([
        ("int".to_owned(), Variant::Int(i64::MIN)),
        ("float".to_owned(), Variant::Float(-0.125)),
        ("bool".to_owned(), Variant::Bool(true)),
        ("string".to_owned(), Variant::String("привіт 🎉".to_owned())),
        (
            "datetime".to_owned(),
            Variant::Datetime(at_ms(0) + time::Duration::nanoseconds(123_456_789)),
        ),
        (
            "array".to_owned(),
            Variant::Array(vec![Variant::Int(1), Variant::String(String::new())]),
        ),
        ("object".to_owned(), Variant::Object(nested)),
    ])
}

#[tokio::test]
async fn a_scheduled_run_reads_back_exactly_as_it_was_specified() {
    let full = ScheduledRunSpec {
        target_action_id: ActionId::new(),
        due_at: at_ms(86_400_123),
        key: Some("vip:алиса".to_owned()),
        missed_run_policy: MissedRunPolicy::SkipIfLateBy(Duration::from_millis(90_123)),
        args: every_variant_kind(),
        scheduled_by_action: Some(ActionId::new()),
        scheduled_by_run: Some("run-42".to_owned()),
        trigger_event_id: Some(EventId::new()),
        scheduled_at: at_ms(7),
        label: "Remove VIP from алиса".to_owned(),
    };
    let mut zero_threshold = spec(ActionId::new(), 1, None);
    zero_threshold.missed_run_policy = MissedRunPolicy::SkipIfLateBy(Duration::ZERO);
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();

    for spec in [full, spec(ActionId::new(), 1, None), zero_threshold] {
        let id = place(&repo, &spec).await;
        assert_eq!(
            fetch(&repo, id).await,
            ScheduledRun {
                id,
                spec,
                state: ScheduledRunState::Pending,
                outcome_reason: None,
                resolved_at: None,
            }
        );
    }
}

#[tokio::test]
async fn a_run_with_a_non_finite_float_argument_cannot_make_the_pending_list_unreadable() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    place(&repo, &spec(ActionId::new(), 1, None)).await;
    let mut poisoned = spec(ActionId::new(), 2, None);
    poisoned.args = BTreeMap::from([("ratio".to_owned(), Variant::Float(f64::NAN))]);

    let scheduled = repo.schedule(&poisoned).await;
    let listed = repo.list_pending().await;

    assert!(
        scheduled.is_err() || listed.is_ok(),
        "a stored run broke every pending read: {listed:?}"
    );
}

#[tokio::test]
async fn pending_counts_cover_only_pending_runs_per_target() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let busy = ActionId::new();
    let quiet = ActionId::new();
    place(&repo, &spec(busy, 1, None)).await;
    place(&repo, &spec(busy, 2, None)).await;
    let cancelled = place(&repo, &spec(busy, 3, None)).await;
    repo.cancel(cancelled, at_min(0)).await.unwrap();
    place(&repo, &spec(quiet, 1, None)).await;

    assert_eq!(
        (
            repo.count_pending().await.unwrap(),
            repo.count_pending_for_action(busy).await.unwrap(),
            repo.count_pending_for_action(quiet).await.unwrap(),
            repo.count_pending_for_action(ActionId::new())
                .await
                .unwrap(),
        ),
        (3, 2, 1, 0)
    );
}

#[tokio::test]
async fn recent_resolved_runs_list_newest_resolution_first_within_the_limit() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let oldest = resolved(&repo, ScheduledRunOutcome::Failed, 1_000).await;
    let tie_first = resolved(&repo, ScheduledRunOutcome::Skipped, 3_000).await;
    let middle = resolved(&repo, ScheduledRunOutcome::Dispatched, 2_000).await;
    let tie_second = resolved(&repo, ScheduledRunOutcome::Cancelled, 3_000).await;
    place(&repo, &spec(ActionId::new(), 1, None)).await;

    for (limit, expected) in [
        (0, vec![]),
        (3, vec![tie_second, tie_first, middle]),
        (usize::MAX, vec![tie_second, tie_first, middle, oldest]),
    ] {
        let listed: Vec<_> = repo
            .list_recent_resolved(limit)
            .await
            .unwrap()
            .into_iter()
            .map(|run| run.id)
            .collect();
        assert_eq!(listed, expected, "limit {limit}");
    }
}

#[tokio::test]
async fn prune_removes_only_resolved_runs_strictly_before_the_cutoff() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let before = resolved(&repo, ScheduledRunOutcome::Failed, 4_999).await;
    let on_cutoff = resolved(&repo, ScheduledRunOutcome::Skipped, 5_000).await;
    let after = resolved(&repo, ScheduledRunOutcome::Dispatched, 5_001).await;
    let mut long_overdue = spec(ActionId::new(), 0, None);
    long_overdue.due_at = at_ms(-1_000_000);
    long_overdue.scheduled_at = at_ms(-2_000_000);
    let pending = place(&repo, &long_overdue).await;

    let pruned = repo.prune_resolved_before(at_ms(5_000)).await.unwrap();

    let mut survivors = Vec::new();
    for id in [before, on_cutoff, after, pending] {
        survivors.push(repo.get(id).await.unwrap().is_some());
    }
    assert_eq!((pruned, survivors), (1, vec![false, true, true, true]));
}

fn action_for(queue_id: forge_types::QueueId, name: &str) -> Action {
    Action {
        id: ActionId::new(),
        name: name.to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: forge_types::ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![],
    }
}

async fn saved_actions(backend: &SqliteBackend) -> (ActionId, ActionId) {
    let queue_id = backend.queue_repo().list().await.unwrap()[0].id;
    let doomed = action_for(queue_id, "doomed");
    let kept = action_for(queue_id, "kept");
    backend.action_repo().save(&doomed).await.unwrap();
    backend.action_repo().save(&kept).await.unwrap();
    (doomed.id, kept.id)
}

#[tokio::test]
async fn deleting_an_action_drops_its_pending_runs_and_keeps_its_history_and_other_targets() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let (doomed, kept) = saved_actions(&backend).await;
    let doomed_pending = place(&repo, &spec(doomed, 10, Some("vip:alice"))).await;
    let doomed_dispatched = place(&repo, &spec(doomed, 1, None)).await;
    repo.claim(doomed_dispatched, at_min(1)).await.unwrap();
    let kept_pending = place(&repo, &spec(kept, 10, None)).await;

    assert!(backend.action_repo().delete(doomed).await.unwrap());

    assert_eq!(
        (
            repo.get(doomed_pending).await.unwrap(),
            fetch(&repo, doomed_dispatched).await.state,
            pending_ids(&repo).await,
            repo.count_pending_for_action(doomed).await.unwrap(),
        ),
        (None, ScheduledRunState::Dispatched, vec![kept_pending], 0)
    );
}

#[tokio::test]
async fn archiving_an_action_keeps_its_pending_runs() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let (archived, _) = saved_actions(&backend).await;
    let pending = place(&repo, &spec(archived, 10, None)).await;

    assert!(backend.action_repo().archive(archived).await.unwrap());

    assert_eq!(pending_ids(&repo).await, vec![pending]);
}

#[tokio::test]
async fn deleting_an_action_advances_the_schedule_revision_only_when_a_row_went() {
    let backend = setup().await;
    let (doomed, _) = saved_actions(&backend).await;
    let revision = backend.scheduled_run_revision();

    for (id, expect_advance) in [(ActionId::new(), false), (doomed, true)] {
        let before = revision.current();
        backend.action_repo().delete(id).await.unwrap();
        assert_eq!(revision.current() > before, expect_advance, "{id:?}");
    }
}

macro_rules! bumps {
    ($backend:expr, $label:literal, $op:expr, $schedule:expr) => {{
        let schedule = $backend.scheduled_run_revision();
        let catalog = $backend.catalog_revision();
        let before = (schedule.current(), catalog.current());
        $op.await.expect($label);
        assert_eq!(
            (schedule.current() > before.0, catalog.current()),
            ($schedule, before.1),
            "{}",
            $label,
        );
    }};
}

#[tokio::test]
async fn every_schedule_write_advances_the_schedule_revision_and_never_the_catalog_one() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let target = ActionId::new();
    let keyed = spec(target, 10, Some("vip:alice"));
    let plain = spec(target, 10, None);

    bumps!(backend, "schedule", repo.schedule(&keyed), true);
    bumps!(backend, "schedule replace", repo.schedule(&keyed), true);
    bumps!(
        backend,
        "cancel_by_key",
        repo.cancel_by_key("vip:alice", at_min(1)),
        true
    );
    let claimed = place(&repo, &plain).await;
    bumps!(backend, "claim", repo.claim(claimed, at_min(1)), true);
    bumps!(
        backend,
        "settle",
        repo.settle(claimed, ScheduledRunOutcome::Failed, None, at_min(2)),
        true
    );
    let cancelled = place(&repo, &plain).await;
    bumps!(backend, "cancel", repo.cancel(cancelled, at_min(1)), true);
    bumps!(
        backend,
        "prune",
        repo.prune_resolved_before(at_min(60)),
        true
    );
}

#[tokio::test]
async fn schedule_reads_leave_both_revisions_unchanged() {
    let backend = setup().await;
    let repo = backend.scheduled_run_repo();
    let target = ActionId::new();
    let id = place(&repo, &spec(target, 10, None)).await;

    bumps!(backend, "get", repo.get(id), false);
    bumps!(backend, "list_pending", repo.list_pending(), false);
    bumps!(backend, "list_due", repo.list_due(at_min(60)), false);
    bumps!(backend, "next_due", repo.next_due(), false);
    bumps!(
        backend,
        "list_recent_resolved",
        repo.list_recent_resolved(10),
        false
    );
    bumps!(backend, "count_pending", repo.count_pending(), false);
    bumps!(
        backend,
        "count_pending_for_action",
        repo.count_pending_for_action(target),
        false
    );
}

async fn migrated_pools() -> (SqlitePools, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", dir.path().join("forge.db").display());
    let pools = connect_pools(&url).await.unwrap();
    apply_migrations(pools.writer()).await.unwrap();
    (pools, dir)
}

const INSERT_ROW: &str = "INSERT INTO scheduled_runs (
        target_action_id, due_at, key, missed_policy, skip_late_ms, args,
        scheduled_at, label, state, resolved_at
    ) VALUES ('a', 0, ?, ?, ?, '{}', 0, 'l', ?, ?)";

async fn insert_raw(
    pools: &SqlitePools,
    key: Option<&str>,
    policy: &str,
    skip_late_ms: Option<i64>,
    state: &str,
    resolved_at: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query(INSERT_ROW)
        .bind(key)
        .bind(policy)
        .bind(skip_late_ms)
        .bind(state)
        .bind(resolved_at)
        .execute(pools.writer())
        .await
        .map(|_| ())
}

#[tokio::test]
async fn the_schema_rejects_rows_whose_state_and_policy_columns_disagree() {
    let (pools, _dir) = migrated_pools().await;
    assert!(
        insert_raw(&pools, None, "run_late_once", None, "pending", None)
            .await
            .is_ok()
    );

    for (policy, skip_late_ms, state, resolved_at) in [
        ("run_late_once", None, "pending", Some(1)),
        ("run_late_once", None, "dispatched", None),
        ("skip_if_late_by", None, "pending", None),
        ("run_late_once", Some(5), "pending", None),
        ("run_later", None, "pending", None),
        ("run_late_once", None, "running", Some(1)),
    ] {
        let result = insert_raw(&pools, None, policy, skip_late_ms, state, resolved_at).await;
        assert!(
            result.is_err(),
            "{policy} {skip_late_ms:?} {state} {resolved_at:?}"
        );
    }
}

#[tokio::test]
async fn the_schema_allows_one_pending_run_per_key_and_any_number_of_resolved_ones() {
    let (pools, _dir) = migrated_pools().await;
    let row = |state, resolved_at| {
        insert_raw(&pools, Some("k"), "run_late_once", None, state, resolved_at)
    };
    row("dispatched", Some(1)).await.unwrap();
    row("cancelled", Some(1)).await.unwrap();
    row("pending", None).await.unwrap();

    assert!(row("pending", None).await.is_err());
}
