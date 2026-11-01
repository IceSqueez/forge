#![expect(clippy::unwrap_used, reason = "test fixtures assert directly")]
mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use common::{Store, at, saved_action, store};
use forge_events::{Event, EventSource};
use forge_storage::{
    BanLedgerEntry, BanLedgerKey, BanLedgerRepo, BanOrigin, ChatHistoryRepo, ChatModerationTarget,
    DataProvider, DonationRepo, EventLogRepo, HistoryRepo, LatestRecord, LatestValueRepo,
    MissedRunPolicy, ScheduledRunId, ScheduledRunOutcome, ScheduledRunRepo, ScheduledRunSpec,
    StorageError, UserGlobalKey, UserGlobalsRepo, ViewerRepo,
};
use forge_types::{
    ActionId, CurrencyCode, Donation, DonationOrigin, Donor, EventId, ExecutionContext,
    ExecutionMetadata, ExecutionOutcome, IntegrationId, ModerationMarks, MoneyAmount, PlatformId,
    UnifiedChatRow, UserRemoval, Variant,
};
use serde_json::json;
use time::Duration;

const BROADCASTER: &str = "broadcaster";
const VIEWER: &str = "viewer";
const COUNTER: &str = "deaths";
const SLOT: &str = "follower";
const PROVIDER: &str = "monobank";
const NO_SUCH_MESSAGE: &str = "absent";

fn chats(store: &Store) -> Arc<dyn ChatHistoryRepo> {
    store.backend.chat_history_repo()
}

fn events(store: &Store) -> Arc<dyn EventLogRepo> {
    store.backend.event_log_repo()
}

fn runs(store: &Store) -> Arc<dyn ScheduledRunRepo> {
    store.backend.scheduled_run_repo()
}

fn history(store: &Store) -> Arc<dyn HistoryRepo> {
    store.backend.history_repo()
}

fn donations(store: &Store) -> Arc<dyn DonationRepo> {
    store.backend.donation_repo()
}

fn bans(store: &Store) -> Arc<dyn BanLedgerRepo> {
    store.backend.ban_ledger_repo()
}

fn latest(store: &Store) -> Arc<dyn LatestValueRepo> {
    store.backend.latest_value_repo()
}

fn viewers(store: &Store) -> Arc<dyn ViewerRepo> {
    store.backend.viewer_repo()
}

fn user_globals(store: &Store) -> Arc<dyn UserGlobalsRepo> {
    store.backend.user_globals_repo()
}

fn chat_row(id: &str, source: PlatformId, author_id: &str) -> UnifiedChatRow {
    UnifiedChatRow {
        id: id.to_owned(),
        event_id: EventId::new(),
        source,
        received_at: at(0),
        author: format!("name of {author_id}"),
        author_id: Some(author_id.to_owned()),
        author_color: None,
        body_segments: vec![],
        badges: vec![],
        is_event: false,
        event_detail: None,
        moderation: ModerationMarks::default(),
    }
}

async fn chat_ids(store: &Store) -> Vec<String> {
    chats(store)
        .list_recent(usize::MAX >> 1)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.id)
        .collect()
}

fn logged_event() -> Event {
    Event::new(EventSource::Core, "test.kind", json!({}))
}

fn run_context(action_id: ActionId, started_offset: i64) -> ExecutionContext {
    ExecutionContext {
        action_id,
        metadata: ExecutionMetadata::Trigger {
            event_id: EventId::new(),
            trigger_kind: None,
        },
        arg_stack_snapshot: BTreeMap::new(),
        started_at: at(started_offset),
        completed_at: Some(at(started_offset)),
        telemetry: vec![],
        outcome: ExecutionOutcome::Success,
    }
}

fn run_spec(target: ActionId, due_offset: i64, key: Option<&str>) -> ScheduledRunSpec {
    ScheduledRunSpec {
        target_action_id: target,
        due_at: at(due_offset),
        key: key.map(str::to_owned),
        missed_run_policy: MissedRunPolicy::RunLateOnce,
        args: BTreeMap::new(),
        scheduled_by_action: None,
        scheduled_by_run: None,
        trigger_event_id: None,
        replay: false,
        scheduled_at: at(0),
        label: "follow-up".to_owned(),
    }
}

fn donation(id: &str, occurred_offset: i64) -> Donation {
    Donation {
        provider: IntegrationId::new(PROVIDER),
        donation_id: id.to_owned(),
        donor: Donor::Anonymous,
        message: None,
        amount: MoneyAmount::from_micros(1_000_000, CurrencyCode::parse("UAH").unwrap()),
        occurred_at: at(occurred_offset),
        origin: DonationOrigin::Live,
    }
}

fn ban_key(viewer_id: &str) -> BanLedgerKey {
    BanLedgerKey {
        platform: PlatformId::Kick,
        channel_id: BROADCASTER.to_owned(),
        viewer_id: viewer_id.to_owned(),
    }
}

fn ban(viewer_id: &str, banned_offset: i64, term: Option<Duration>) -> BanLedgerEntry {
    BanLedgerEntry {
        key: ban_key(viewer_id),
        viewer_name: viewer_id.to_owned(),
        reason: None,
        moderator: None,
        banned_at: at(banned_offset),
        expires_at: term.map(|term| at(banned_offset) + term),
        platform_ban_id: None,
        origin: BanOrigin::IssuedByForge,
    }
}

fn latest_record(platform: &str, occurred_offset: i64, value: i64) -> LatestRecord {
    LatestRecord {
        slot: SLOT.to_owned(),
        platform: platform.to_owned(),
        payload: json!({ "value": value }),
        occurred_at: at(occurred_offset),
        updated_at: at(occurred_offset),
    }
}

fn counter_key(user_id: &str, name: &str) -> UserGlobalKey {
    UserGlobalKey {
        broadcaster_id: BROADCASTER.to_owned(),
        user_id: user_id.to_owned(),
        name: name.to_owned(),
    }
}

#[tokio::test]
async fn a_duplicate_chat_id_is_dropped_by_the_single_and_the_batch_path() {
    let store = store().await;
    let row = chat_row("one", PlatformId::Twitch, VIEWER);

    chats(&store).append(&row).await.unwrap();
    chats(&store).append(&row).await.unwrap();
    chats(&store)
        .append_batch(&[row.clone(), chat_row("two", PlatformId::Twitch, VIEWER)])
        .await
        .unwrap();

    assert_eq!(chat_ids(&store).await, vec!["two", "one"]);
}

#[tokio::test]
async fn a_duplicate_event_id_is_dropped_by_the_single_and_the_batch_path() {
    let store = store().await;
    let event = logged_event();
    let other = logged_event();

    events(&store).insert(&event).await.unwrap();
    events(&store).insert(&event).await.unwrap();
    events(&store)
        .insert_batch(&[Arc::new(event.clone()), Arc::new(other.clone())])
        .await
        .unwrap();

    let logged: Vec<EventId> = events(&store)
        .recent(10)
        .await
        .unwrap()
        .into_iter()
        .map(|event| event.id)
        .collect();
    assert_eq!(logged, vec![other.id, event.id]);
}

#[tokio::test]
async fn an_unknown_event_reads_as_nothing() {
    let store = store().await;

    assert!(events(&store).get(EventId::new()).await.unwrap().is_none());
}

#[tokio::test]
async fn moderating_chat_that_is_not_stored_touches_no_row() {
    let store = store().await;
    chats(&store)
        .append(&chat_row("one", PlatformId::Twitch, VIEWER))
        .await
        .unwrap();

    let deleted = chats(&store)
        .mark_message_deleted(PlatformId::Twitch, NO_SUCH_MESSAGE)
        .await
        .unwrap();
    let removed = chats(&store)
        .mark_user_messages_moderated(
            &ChatModerationTarget {
                source: PlatformId::Kick,
                author_id: Some(VIEWER.to_owned()),
                author_name: VIEWER.to_owned(),
            },
            UserRemoval::Ban,
        )
        .await
        .unwrap();

    assert_eq!((deleted, removed), (0, 0));
}

#[tokio::test]
async fn clearing_a_platform_marks_only_its_rows_deleted() {
    let store = store().await;
    chats(&store)
        .append_batch(&[
            chat_row("twitch", PlatformId::Twitch, VIEWER),
            chat_row("kick", PlatformId::Kick, VIEWER),
        ])
        .await
        .unwrap();

    assert_eq!(
        chats(&store)
            .clear_platform(PlatformId::Twitch)
            .await
            .unwrap(),
        1
    );

    let deleted: Vec<(String, bool)> = chats(&store)
        .list_recent(10)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.id, row.moderation.deleted))
        .collect();
    assert_eq!(
        deleted,
        vec![("kick".to_owned(), false), ("twitch".to_owned(), true)]
    );
}

#[tokio::test]
async fn run_history_reads_newest_first_within_the_limit_and_the_action() {
    let store = store().await;
    let watched = saved_action(&store, "watched").await;
    let other = saved_action(&store, "other").await;
    history(&store)
        .save_batch(&[
            run_context(watched, 0),
            run_context(other, 1),
            run_context(watched, 2),
        ])
        .await
        .unwrap();

    let recent = history(&store).recent_for_action(watched, 1).await.unwrap();

    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].started_at, at(2));
    assert_eq!(history(&store).recent(10).await.unwrap().len(), 3);
}

#[tokio::test]
async fn a_second_insert_of_one_donation_is_refused_and_keeps_the_first_row() {
    let store = store().await;
    let repo = donations(&store);

    assert!(
        repo.insert_if_new(&donation("one", 0), at(0))
            .await
            .unwrap()
    );
    assert!(
        !repo
            .insert_if_new(&donation("one", 5), at(5))
            .await
            .unwrap()
    );

    let stored = repo.list_recent(10).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].donation.occurred_at, at(0));
}

#[tokio::test]
async fn announcing_is_a_one_way_mark_and_a_missing_donation_reports_nothing() {
    let store = store().await;
    let repo = donations(&store);
    let provider = IntegrationId::new(PROVIDER);
    repo.insert_if_new(&donation("one", 0), at(0))
        .await
        .unwrap();

    assert!(
        !repo
            .mark_announced(&provider, "absent", at(1))
            .await
            .unwrap()
    );
    assert!(repo.mark_announced(&provider, "one", at(1)).await.unwrap());
    assert!(!repo.mark_announced(&provider, "one", at(2)).await.unwrap());
    assert!(
        repo.list_unannounced_occurred_since(at(-1))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn removing_a_ban_that_is_not_held_reports_nothing() {
    let store = store().await;

    assert!(!bans(&store).remove(&ban_key(VIEWER)).await.unwrap());
}

#[tokio::test]
async fn an_expired_ban_is_gone_from_every_read_at_the_exact_expiry() {
    let store = store().await;
    let repo = bans(&store);
    let term = Duration::seconds(10);
    repo.upsert(&ban(VIEWER, 0, Some(term)), at(0))
        .await
        .unwrap();

    assert!(repo.get(&ban_key(VIEWER), at(9)).await.unwrap().is_some());
    assert!(repo.get(&ban_key(VIEWER), at(10)).await.unwrap().is_none());
    assert!(
        repo.list_active(PlatformId::Kick, BROADCASTER, at(10), 10)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn active_bans_list_newest_first_within_the_limit() {
    let store = store().await;
    let repo = bans(&store);
    repo.upsert(&ban("older", 0, None), at(0)).await.unwrap();
    repo.upsert(&ban("newer", 5, None), at(5)).await.unwrap();

    let listed = repo
        .list_active(PlatformId::Kick, BROADCASTER, at(6), 1)
        .await
        .unwrap();

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].key.viewer_id, "newer");
}

#[tokio::test]
async fn a_latest_value_never_moves_backwards_in_time() {
    let store = store().await;
    let repo = latest(&store);

    assert!(
        repo.upsert_if_newer(&latest_record("twitch", 5, 1))
            .await
            .unwrap()
    );
    assert!(
        !repo
            .upsert_if_newer(&latest_record("twitch", 4, 2))
            .await
            .unwrap()
    );

    assert_eq!(
        repo.list_slot(SLOT).await.unwrap()[0].payload,
        json!({ "value": 1 })
    );
}

#[tokio::test]
async fn latest_values_are_kept_per_platform_and_reset_as_one_slot() {
    let store = store().await;
    let repo = latest(&store);
    repo.upsert_if_newer(&latest_record("twitch", 1, 1))
        .await
        .unwrap();
    repo.upsert_if_newer(&latest_record("kick", 2, 2))
        .await
        .unwrap();

    let listed: Vec<String> = repo
        .list_slot(SLOT)
        .await
        .unwrap()
        .into_iter()
        .map(|record| record.platform)
        .collect();

    assert_eq!(listed, vec!["kick", "twitch"]);
    assert_eq!(repo.reset_slot(SLOT).await.unwrap(), 2);
    assert_eq!(repo.reset_slot(SLOT).await.unwrap(), 0);
}

#[tokio::test]
async fn an_unknown_viewer_reads_as_nothing_and_takes_no_greeting() {
    let store = store().await;
    let repo = viewers(&store);

    assert!(
        repo.get(PlatformId::Twitch, VIEWER)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !repo
            .set_custom_greeting(PlatformId::Twitch, VIEWER, true)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn a_returning_viewer_stays_one_row_per_platform() {
    let store = store().await;
    let repo = viewers(&store);

    repo.record_message(PlatformId::Twitch, VIEWER, "first")
        .await
        .unwrap();
    repo.record_message(PlatformId::Twitch, VIEWER, "renamed")
        .await
        .unwrap();
    repo.record_message(PlatformId::Kick, VIEWER, "other")
        .await
        .unwrap();

    assert_eq!(repo.count().await.unwrap(), 2);
    assert_eq!(
        repo.get(PlatformId::Twitch, VIEWER)
            .await
            .unwrap()
            .map(|viewer| viewer.username),
        Some("renamed".to_owned())
    );
}

#[tokio::test]
async fn deleting_an_absent_user_global_reports_nothing() {
    let store = store().await;

    assert!(
        !user_globals(&store)
            .delete(&counter_key(VIEWER, COUNTER))
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn user_global_lists_are_sorted_by_user_then_name() {
    let store = store().await;
    let repo = user_globals(&store);
    for (user, name) in [("b", "x"), ("a", "z"), ("a", "y")] {
        repo.set(&counter_key(user, name), Variant::Int(1))
            .await
            .unwrap();
    }

    let for_user: Vec<String> = repo
        .list_for_user(BROADCASTER, "a")
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    let for_broadcaster: Vec<(String, String)> = repo
        .list_for_broadcaster(BROADCASTER)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| (entry.user_id, entry.name))
        .collect();

    assert_eq!(for_user, vec!["y", "z"]);
    assert_eq!(
        for_broadcaster,
        vec![
            ("a".to_owned(), "y".to_owned()),
            ("a".to_owned(), "z".to_owned()),
            ("b".to_owned(), "x".to_owned()),
        ]
    );
}

#[tokio::test]
async fn incrementing_a_user_global_saturates_at_the_integer_bounds() {
    let store = store().await;
    let repo = user_globals(&store);

    for (start, amount, expected) in [
        (i64::MAX, 1, i64::MAX),
        (i64::MAX - 1, 1, i64::MAX),
        (i64::MIN, -1, i64::MIN),
    ] {
        let key = counter_key(VIEWER, &format!("{start}"));
        repo.set(&key, Variant::Int(start)).await.unwrap();

        assert_eq!(
            repo.incr(&key, amount).await.unwrap(),
            Variant::Int(expected)
        );
    }
}

#[tokio::test]
async fn incrementing_a_non_numeric_user_global_is_refused_and_keeps_the_value() {
    let store = store().await;
    let repo = user_globals(&store);
    let key = counter_key(VIEWER, COUNTER);
    let text = Variant::String("text".to_owned());
    repo.set(&key, text.clone()).await.unwrap();

    assert!(repo.incr(&key, 1).await.is_err());

    assert_eq!(repo.get(&key).await.unwrap(), Some(text));
}

#[tokio::test]
async fn every_scheduled_run_write_on_a_missing_id_reports_that_nothing_matched() {
    let store = store().await;
    let repo = runs(&store);
    let missing = ScheduledRunId::new(i64::MAX);

    assert!(repo.get(missing).await.unwrap().is_none());
    assert!(repo.claim(missing, at(0)).await.unwrap().is_none());
    assert!(!repo.cancel(missing, at(0)).await.unwrap());
    assert!(!repo.cancel_by_key("absent", at(0)).await.unwrap());
    assert!(
        !repo
            .settle(missing, ScheduledRunOutcome::Dispatched, None, at(0))
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn a_scheduled_run_can_be_claimed_once() {
    let store = store().await;
    let repo = runs(&store);
    let target = saved_action(&store, "target").await;
    let placed = repo.schedule(&run_spec(target, 1, None)).await.unwrap();

    assert!(repo.claim(placed.id, at(1)).await.unwrap().is_some());
    assert!(repo.claim(placed.id, at(2)).await.unwrap().is_none());
}

#[tokio::test]
async fn a_cancelled_run_is_no_longer_pending_or_claimable() {
    let store = store().await;
    let repo = runs(&store);
    let target = saved_action(&store, "target").await;
    let placed = repo.schedule(&run_spec(target, 1, None)).await.unwrap();

    assert!(repo.cancel(placed.id, at(0)).await.unwrap());

    assert!(!repo.cancel(placed.id, at(0)).await.unwrap());
    assert_eq!(repo.count_pending().await.unwrap(), 0);
    assert!(repo.claim(placed.id, at(1)).await.unwrap().is_none());
}

#[tokio::test]
async fn rescheduling_a_key_supersedes_the_pending_run_it_replaces() {
    let store = store().await;
    let repo = runs(&store);
    let target = saved_action(&store, "target").await;
    let first = repo
        .schedule(&run_spec(target, 1, Some("key")))
        .await
        .unwrap();

    let second = repo
        .schedule(&run_spec(target, 2, Some("key")))
        .await
        .unwrap();

    assert_eq!(second.superseded, Some(first.id));
    assert_eq!(repo.count_pending_for_action(target).await.unwrap(), 1);
}

#[tokio::test]
async fn due_runs_are_the_pending_ones_at_or_before_now() {
    let store = store().await;
    let repo = runs(&store);
    let target = saved_action(&store, "target").await;
    let due = repo.schedule(&run_spec(target, 1, None)).await.unwrap();
    repo.schedule(&run_spec(target, 2, None)).await.unwrap();

    let listed: Vec<ScheduledRunId> = repo
        .list_due(at(1))
        .await
        .unwrap()
        .into_iter()
        .map(|run| run.id)
        .collect();

    assert_eq!(listed, vec![due.id]);
    assert_eq!(repo.next_due().await.unwrap(), Some(at(1)));
}

#[tokio::test]
async fn an_unknown_user_global_has_no_type_to_mismatch() {
    let store = store().await;
    let repo = user_globals(&store);

    let result = repo.incr(&counter_key(VIEWER, COUNTER), 3).await;

    assert!(!matches!(result, Err(StorageError::TypeMismatch { .. })));
    assert_eq!(result.unwrap(), Variant::Int(3));
}
