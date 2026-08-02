#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{ActionId, EventId, ExecutionContext, ExecutionMetadata, ExecutionOutcome};
use std::collections::BTreeMap;
use time::OffsetDateTime;

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn setup() -> SqliteBackend {
    SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open")
}

fn make_ctx(action_id: ActionId, event_id: EventId) -> ExecutionContext {
    ExecutionContext {
        action_id,
        metadata: ExecutionMetadata::Trigger {
            event_id,
            trigger_kind: None,
        },
        arg_stack_snapshot: BTreeMap::new(),
        started_at: OffsetDateTime::now_utc(),
        completed_at: Some(OffsetDateTime::now_utc()),
        telemetry: vec![],
        outcome: ExecutionOutcome::Success,
    }
}

#[tokio::test]
async fn save_then_recent_for_action_roundtrips() {
    let backend = setup().await;
    let action_id = ActionId::new();
    let event_id = EventId::new();
    let ctx = make_ctx(action_id, event_id);

    backend.history_repo().save(&ctx).await.expect("save");

    let records = backend
        .history_repo()
        .recent_for_action(action_id, 10)
        .await
        .expect("recent_for_action");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].action_id, action_id);
    assert!(
        matches!(&records[0].metadata, ExecutionMetadata::Trigger { event_id: eid, .. } if *eid == event_id)
    );
    assert_eq!(records[0].outcome, ExecutionOutcome::Success);
}

#[tokio::test]
async fn recent_for_action_returns_empty_for_unknown_action() {
    let backend = setup().await;
    let records = backend
        .history_repo()
        .recent_for_action(ActionId::new(), 10)
        .await
        .expect("recent_for_action");
    assert!(records.is_empty());
}

#[tokio::test]
async fn recent_for_action_respects_limit() {
    let backend = setup().await;
    let action_id = ActionId::new();
    for _ in 0..5 {
        backend
            .history_repo()
            .save(&make_ctx(action_id, EventId::new()))
            .await
            .expect("save");
    }
    let records = backend
        .history_repo()
        .recent_for_action(action_id, 3)
        .await
        .expect("recent_for_action");
    assert_eq!(records.len(), 3);
}

#[tokio::test]
async fn recent_for_action_scoped_to_action() {
    let backend = setup().await;
    let action_a = ActionId::new();
    let action_b = ActionId::new();

    backend
        .history_repo()
        .save(&make_ctx(action_a, EventId::new()))
        .await
        .expect("save a");
    backend
        .history_repo()
        .save(&make_ctx(action_a, EventId::new()))
        .await
        .expect("save a2");
    backend
        .history_repo()
        .save(&make_ctx(action_b, EventId::new()))
        .await
        .expect("save b");

    let for_a = backend
        .history_repo()
        .recent_for_action(action_a, 100)
        .await
        .expect("recent_for_action a");
    assert_eq!(for_a.len(), 2);
    assert!(for_a.iter().all(|c| c.action_id == action_a));
}

fn make_ctx_at(
    action_id: ActionId,
    event_id: EventId,
    started_at: OffsetDateTime,
) -> ExecutionContext {
    ExecutionContext {
        action_id,
        metadata: ExecutionMetadata::Trigger {
            event_id,
            trigger_kind: None,
        },
        arg_stack_snapshot: BTreeMap::new(),
        started_at,
        completed_at: Some(started_at),
        telemetry: vec![],
        outcome: ExecutionOutcome::Success,
    }
}

async fn surviving_event_ids(backend: &SqliteBackend, action_id: ActionId) -> Vec<EventId> {
    backend
        .history_repo()
        .recent_for_action(action_id, 100)
        .await
        .expect("recent_for_action")
        .into_iter()
        .filter_map(|c| match c.metadata {
            ExecutionMetadata::Trigger { event_id, .. } => Some(event_id),
            ExecutionMetadata::QuickAction { .. } => None,
        })
        .collect()
}

#[tokio::test]
async fn prune_before_deletes_older_entries_and_query_excludes_them() {
    let backend = setup().await;
    let action_id = ActionId::new();
    let now = OffsetDateTime::now_utc();

    let old = EventId::new();
    let recent = EventId::new();
    backend
        .history_repo()
        .save(&make_ctx_at(action_id, old, now - time::Duration::days(10)))
        .await
        .expect("save old");
    backend
        .history_repo()
        .save(&make_ctx_at(
            action_id,
            recent,
            now - time::Duration::days(1),
        ))
        .await
        .expect("save recent");

    let removed = backend
        .history_repo()
        .prune_before(now - time::Duration::days(7))
        .await
        .expect("prune");

    assert_eq!(
        removed, 1,
        "only the 10-day-old entry predates the 7-day cutoff"
    );
    assert_eq!(
        surviving_event_ids(&backend, action_id).await,
        vec![recent],
        "the pruned entry no longer appears in recent_for_action",
    );
}

#[tokio::test]
async fn prune_before_uses_millisecond_precision_at_cutoff_boundary() {
    let backend = setup().await;
    let action_id = ActionId::new();
    let base = OffsetDateTime::now_utc();

    let before = EventId::new();
    let at_cutoff = EventId::new();
    let after = EventId::new();
    backend
        .history_repo()
        .save(&make_ctx_at(
            action_id,
            before,
            base - time::Duration::seconds(1),
        ))
        .await
        .expect("save before");
    backend
        .history_repo()
        .save(&make_ctx_at(action_id, at_cutoff, base))
        .await
        .expect("save at cutoff");
    backend
        .history_repo()
        .save(&make_ctx_at(
            action_id,
            after,
            base + time::Duration::seconds(1),
        ))
        .await
        .expect("save after");

    let removed = backend
        .history_repo()
        .prune_before(base)
        .await
        .expect("prune");

    assert_eq!(
        removed, 1,
        "only the entry one second before the cutoff is pruned"
    );
    let survivors = surviving_event_ids(&backend, action_id).await;
    assert!(!survivors.contains(&before), "cutoff-1s entry pruned");
    assert!(
        survivors.contains(&at_cutoff),
        "entry exactly at cutoff kept (strict `<`)"
    );
    assert!(survivors.contains(&after), "cutoff+1s entry kept");
}

fn fixed_instant(offset_secs: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_700_000_000 + offset_secs).expect("valid timestamp")
}

fn make_quick_ctx_at(
    action_id: ActionId,
    builtin_id: &str,
    label: &str,
    started_at: OffsetDateTime,
) -> ExecutionContext {
    ExecutionContext {
        action_id,
        metadata: ExecutionMetadata::QuickAction {
            builtin_id: builtin_id.to_owned(),
            label: label.to_owned(),
        },
        arg_stack_snapshot: BTreeMap::new(),
        started_at,
        completed_at: Some(started_at),
        telemetry: vec![],
        outcome: ExecutionOutcome::Success,
    }
}

fn run_labels(records: &[ExecutionContext]) -> Vec<String> {
    records
        .iter()
        .map(|c| match &c.metadata {
            ExecutionMetadata::QuickAction { builtin_id, label } => format!("{builtin_id}/{label}"),
            ExecutionMetadata::Trigger { .. } => "trigger".to_owned(),
        })
        .collect()
}

#[tokio::test]
async fn recent_for_builtin_returns_only_that_builtins_quick_runs_newest_first() {
    let backend = setup().await;
    let repo = backend.history_repo();

    repo.save(&make_ctx_at(
        ActionId::new(),
        EventId::new(),
        fixed_instant(0),
    ))
    .await
    .expect("save trigger run");
    repo.save(&make_quick_ctx_at(
        ActionId::new(),
        "twitch",
        "Send message",
        fixed_instant(10),
    ))
    .await
    .expect("save twitch older");
    repo.save(&make_quick_ctx_at(
        ActionId::new(),
        "obs",
        "Send message",
        fixed_instant(20),
    ))
    .await
    .expect("save obs");
    repo.save(&make_quick_ctx_at(
        ActionId::new(),
        "twitch",
        "Run ad",
        fixed_instant(30),
    ))
    .await
    .expect("save twitch newer");

    let records = repo
        .recent_for_builtin("twitch", 100)
        .await
        .expect("recent_for_builtin");

    assert_eq!(
        run_labels(&records),
        vec!["twitch/Run ad", "twitch/Send message"],
        "only twitch quick runs, newest first"
    );
}

#[tokio::test]
async fn recent_for_builtin_limit_caps_result_to_the_newest_rows() {
    let backend = setup().await;
    let repo = backend.history_repo();
    for (label, secs) in [("a", 10), ("b", 20), ("c", 30)] {
        repo.save(&make_quick_ctx_at(
            ActionId::new(),
            "twitch",
            label,
            fixed_instant(secs),
        ))
        .await
        .expect("save quick run");
    }

    for (limit, expected) in [
        (0u32, vec![]),
        (1, vec!["twitch/c"]),
        (2, vec!["twitch/c", "twitch/b"]),
        (3, vec!["twitch/c", "twitch/b", "twitch/a"]),
        (4, vec!["twitch/c", "twitch/b", "twitch/a"]),
    ] {
        let records = repo
            .recent_for_builtin("twitch", limit)
            .await
            .expect("recent_for_builtin");
        assert_eq!(run_labels(&records), expected, "limit {limit} of 3 rows");
    }
}

#[tokio::test]
async fn recent_for_builtin_returns_empty_for_unknown_builtin() {
    let backend = setup().await;
    backend
        .history_repo()
        .save(&make_quick_ctx_at(
            ActionId::new(),
            "twitch",
            "Run ad",
            fixed_instant(0),
        ))
        .await
        .expect("save quick run");

    let records = backend
        .history_repo()
        .recent_for_builtin("obs", 10)
        .await
        .expect("recent_for_builtin");
    assert!(records.is_empty());
}

#[tokio::test]
async fn stats_summary_omits_actions_whose_only_runs_are_quick_actions() {
    let backend = setup().await;
    let repo = backend.history_repo();
    let triggered = ActionId::new();
    let quick_only = ActionId::new();

    repo.save(&make_ctx_at(triggered, EventId::new(), fixed_instant(10)))
        .await
        .expect("save trigger run");
    repo.save(&make_quick_ctx_at(
        quick_only,
        "twitch",
        "Run ad",
        fixed_instant(20),
    ))
    .await
    .expect("save quick run");

    let stats = repo
        .stats_summary(fixed_instant(0))
        .await
        .expect("stats_summary");

    assert!(
        stats.contains_key(&triggered),
        "trigger run still aggregated"
    );
    assert!(
        !stats.contains_key(&quick_only),
        "quick run must not form a phantom singleton group"
    );
    assert_eq!(stats.len(), 1);
}

#[tokio::test]
async fn stats_summary_excludes_quick_runs_from_an_actions_counts_and_last_run() {
    let backend = setup().await;
    let repo = backend.history_repo();
    let action_id = ActionId::new();

    repo.save(&make_ctx_at(
        action_id,
        EventId::new(),
        fixed_instant(-3_600),
    ))
    .await
    .expect("save trigger run before window");
    for secs in [60, 120] {
        repo.save(&make_ctx_at(action_id, EventId::new(), fixed_instant(secs)))
            .await
            .expect("save trigger run in window");
    }
    repo.save(&make_quick_ctx_at(
        action_id,
        "twitch",
        "Run ad",
        fixed_instant(600),
    ))
    .await
    .expect("save quick run");

    let stats = repo
        .stats_summary(fixed_instant(0))
        .await
        .expect("stats_summary");
    let entry = stats.get(&action_id).expect("action present");

    assert_eq!(entry.runs_24h, 2, "the quick run is not counted");
    assert_eq!(
        entry.last_ran_at,
        fixed_instant(120),
        "last run is the newest trigger run, not the newer quick run"
    );
}

#[tokio::test]
async fn save_failed_outcome_roundtrips() {
    let backend = setup().await;
    let action_id = ActionId::new();
    let mut ctx = make_ctx(action_id, EventId::new());
    ctx.outcome = ExecutionOutcome::Failed("rhai error".to_owned());

    backend.history_repo().save(&ctx).await.expect("save");

    let records = backend
        .history_repo()
        .recent_for_action(action_id, 1)
        .await
        .expect("recent_for_action");
    assert_eq!(records.len(), 1);
    assert!(matches!(&records[0].outcome, ExecutionOutcome::Failed(msg) if msg == "rhai error"));
}
