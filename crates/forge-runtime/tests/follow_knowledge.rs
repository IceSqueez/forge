#![expect(clippy::unwrap_used, reason = "test fixtures")]

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_platform_core::{FollowLookup, FollowStatus};
use forge_registry::{
    ActorRole, ActorSlot, CanonicalVariable, ChatEventKind, EventFilter, KindPlatformContract,
    TriggerCatalogEntry, TriggerCategory, TriggerKindDescriptor, TriggerRegistry,
};
use forge_runtime::{
    EventBus, FollowKnowledgeChanges, FollowKnowledgeHandle, FollowKnowledgeParts,
    NullEventLogRepo, spawn_follow_knowledge,
};
use forge_storage::{
    DataProvider, EventLogRepo, FollowKnowledgeEntry, FollowKnowledgeKey, FollowKnowledgeRepo,
    KnownFollow, MockFollowKnowledgeRepo, StorageError,
};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{ArgStack, EventId, PlatformId, TriggerConfig, Variant};
use futures_util::FutureExt;
use tempfile::TempDir;
use time::OffsetDateTime;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

const FOLLOW_TRIGGER_ID: &str = "stub.follow";
const FOLLOW_KIND: &str = "stub.channel.follow";
const CHAT_TRIGGER_ID: &str = "stub.chat";
const CHAT_KIND: &str = "stub.channel.chat";
const USER_ID_KEY: &str = "user_id";
const FOLLOWED_AT_KEY: &str = "followed_at";
const VIEWER: &str = "viewer-1";
const OTHER_VIEWER: &str = "viewer-2";
const SENTINEL_VIEWER: &str = "sentinel";
const FOLLOWED_AT_UNIX_SECS: i64 = 1_700_000_000;
const LATER_FOLLOWED_AT_UNIX_SECS: i64 = 1_750_000_000;
const CONCURRENT_RESOLVES: usize = 8;
const HANG_GUARD: Duration = Duration::from_secs(60);

struct FollowTrigger;

impl TriggerCatalogEntry for FollowTrigger {
    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Follow"
    }

    fn summary(&self) -> &str {
        "Fires on a follow"
    }

    fn search_text(&self) -> &str {
        "follow"
    }

    fn icon_name(&self) -> &str {
        "follow"
    }
}

impl TriggerKindDescriptor for FollowTrigger {
    fn id(&self) -> &str {
        FOLLOW_TRIGGER_ID
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: None,
            kind_prefix: Some(FOLLOW_KIND.to_owned()),
        }
    }

    fn chat_event(&self) -> Option<ChatEventKind> {
        Some(ChatEventKind::Follow)
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn follow_moment(&self, event: &Event) -> Option<OffsetDateTime> {
        event.payload[FOLLOWED_AT_KEY]
            .as_i64()
            .map(|secs| OffsetDateTime::from_unix_timestamp(secs).unwrap())
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        principal_args(event)
    }
}

struct ChatTrigger;

impl TriggerCatalogEntry for ChatTrigger {
    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat"
    }

    fn summary(&self) -> &str {
        "Fires on a chat message"
    }

    fn search_text(&self) -> &str {
        "chat"
    }

    fn icon_name(&self) -> &str {
        "chat"
    }
}

impl TriggerKindDescriptor for ChatTrigger {
    fn id(&self) -> &str {
        CHAT_TRIGGER_ID
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: None,
            kind_prefix: Some(CHAT_KIND.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        principal_args(event)
    }
}

fn principal_args(event: &Event) -> ArgStack {
    let args = ArgStack::new();
    match event.payload[USER_ID_KEY].as_str() {
        Some(id) => args.set(
            CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id)
                .name()
                .to_owned(),
            Variant::String(id.to_owned()),
        ),
        None => args,
    }
}

fn triggers() -> Arc<TriggerRegistry> {
    let mut registry = TriggerRegistry::new();
    registry.register(Box::new(FollowTrigger)).unwrap();
    registry.register(Box::new(ChatTrigger)).unwrap();
    Arc::new(registry)
}

fn unix(secs: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(secs).unwrap()
}

fn whole_second_now() -> OffsetDateTime {
    OffsetDateTime::now_utc().replace_nanosecond(0).unwrap()
}

fn follow_event(
    source: EventSource,
    viewer_id: &str,
    followed_at: Option<i64>,
    timestamp: OffsetDateTime,
) -> Event {
    let mut payload = serde_json::json!({ USER_ID_KEY: viewer_id });
    if let Some(secs) = followed_at {
        payload[FOLLOWED_AT_KEY] = serde_json::json!(secs);
    }
    let mut event = Event::new(source, FOLLOW_KIND, payload);
    event.timestamp = timestamp;
    event
}

fn twitch_follow(viewer_id: &str, followed_at: i64, timestamp: OffsetDateTime) -> Event {
    follow_event(EventSource::Twitch, viewer_id, Some(followed_at), timestamp)
}

fn entry(
    platform: PlatformId,
    viewer_id: &str,
    follow: KnownFollow,
    observed_at: OffsetDateTime,
    lifetime: time::Duration,
) -> FollowKnowledgeEntry {
    FollowKnowledgeEntry {
        key: FollowKnowledgeKey {
            platform,
            viewer_id: viewer_id.to_owned(),
        },
        follow,
        observed_at,
        fresh_until: observed_at + lifetime,
    }
}

async fn guarded<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(HANG_GUARD, future).await.unwrap()
}

async fn writer_idle(handle: &FollowKnowledgeHandle) {
    guarded(handle.clear_platform(PlatformId::YouTube)).await;
}

async fn next_change(changes: &mut FollowKnowledgeChanges) {
    assert!(guarded(changes.changed()).await);
}

fn change_pending(changes: &mut FollowKnowledgeChanges) -> bool {
    changes.changed().now_or_never().is_some()
}

struct Sqlite {
    backend: SqliteBackend,
    _media: TempDir,
}

impl Sqlite {
    async fn open() -> Self {
        let media = tempfile::tempdir().unwrap();
        let backend =
            SqliteBackend::open_for_test(":memory:", [0x5a; 32], media.path().join("media"), None)
                .await
                .unwrap();
        Self {
            backend,
            _media: media,
        }
    }

    fn repo(&self) -> Arc<dyn FollowKnowledgeRepo> {
        self.backend.follow_knowledge_repo()
    }

    fn event_log(&self) -> Arc<dyn EventLogRepo> {
        self.backend.event_log_repo()
    }

    async fn log(&self, event: &Event) {
        self.event_log().insert(event).await.unwrap();
    }

    fn spawn(&self) -> Rig {
        spawn_rig(self.repo(), self.event_log())
    }
}

struct Rig {
    bus: Arc<EventBus>,
    handle: FollowKnowledgeHandle,
}

fn spawn_rig(repo: Arc<dyn FollowKnowledgeRepo>, event_log: Arc<dyn EventLogRepo>) -> Rig {
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let handle = spawn_follow_knowledge(FollowKnowledgeParts {
        bus: Arc::clone(&bus),
        repo,
        event_log,
        triggers: triggers(),
    });
    Rig { bus, handle }
}

#[derive(Default)]
struct MemoryFollows {
    entries: Mutex<HashMap<FollowKnowledgeKey, FollowKnowledgeEntry>>,
    seeded: Mutex<bool>,
}

#[async_trait]
impl FollowKnowledgeRepo for MemoryFollows {
    async fn get(
        &self,
        key: &FollowKnowledgeKey,
    ) -> Result<Option<FollowKnowledgeEntry>, StorageError> {
        Ok(self.entries.lock().unwrap().get(key).cloned())
    }

    async fn upsert(&self, entry: &FollowKnowledgeEntry) -> Result<(), StorageError> {
        self.entries
            .lock()
            .unwrap()
            .insert(entry.key.clone(), entry.clone());
        Ok(())
    }

    async fn seed(&self, entries: &[FollowKnowledgeEntry]) -> Result<(), StorageError> {
        for entry in entries {
            self.upsert(entry).await?;
        }
        *self.seeded.lock().unwrap() = true;
        Ok(())
    }

    async fn is_seeded(&self) -> Result<bool, StorageError> {
        Ok(*self.seeded.lock().unwrap())
    }

    async fn prune_expired(&self, now: OffsetDateTime) -> Result<u64, StorageError> {
        let mut entries = self.entries.lock().unwrap();
        let before = entries.len();
        entries.retain(|_, entry| entry.fresh_until > now);
        Ok(u64::try_from(before - entries.len()).unwrap())
    }

    async fn clear_platform(&self, platform: PlatformId) -> Result<u64, StorageError> {
        let mut entries = self.entries.lock().unwrap();
        let before = entries.len();
        entries.retain(|key, _| key.platform != platform);
        Ok(u64::try_from(before - entries.len()).unwrap())
    }
}

struct UnreadableEventLog;

fn unreadable() -> StorageError {
    StorageError::Connection {
        reason: "event log unreadable".to_owned(),
    }
}

#[async_trait]
impl EventLogRepo for UnreadableEventLog {
    async fn insert(&self, _event: &Event) -> Result<(), StorageError> {
        Err(unreadable())
    }

    async fn get(&self, _id: EventId) -> Result<Option<Event>, StorageError> {
        Err(unreadable())
    }

    async fn recent(&self, _limit: usize) -> Result<Vec<Event>, StorageError> {
        Err(unreadable())
    }

    async fn recent_since(
        &self,
        _limit: usize,
        _since: Option<EventId>,
    ) -> Result<Vec<Event>, StorageError> {
        Err(unreadable())
    }

    async fn events_of_kinds_since(
        &self,
        _kinds: &[String],
        _since: OffsetDateTime,
    ) -> Result<Vec<Event>, StorageError> {
        Err(unreadable())
    }
}

struct GatedLookup {
    answer: FollowStatus,
    entered: Notify,
    release: Notify,
    calls: AtomicUsize,
}

impl GatedLookup {
    fn answering(answer: FollowStatus) -> Arc<Self> {
        Arc::new(Self {
            answer,
            entered: Notify::new(),
            release: Notify::new(),
            calls: AtomicUsize::new(0),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl FollowLookup for GatedLookup {
    async fn follow_status(&self, _viewer_id: &str) -> FollowStatus {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        self.release.notified().await;
        self.answer
    }
}

struct InstantLookup {
    answer: FollowStatus,
    calls: AtomicUsize,
}

impl InstantLookup {
    fn answering(answer: FollowStatus) -> Self {
        Self {
            answer,
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl FollowLookup for InstantLookup {
    async fn follow_status(&self, _viewer_id: &str) -> FollowStatus {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.answer
    }
}

fn spawn_resolve(
    handle: &FollowKnowledgeHandle,
    lookup: Arc<dyn FollowLookup>,
) -> JoinHandle<FollowStatus> {
    let handle = handle.clone();
    tokio::spawn(async move {
        handle
            .resolve(PlatformId::Twitch, VIEWER, lookup.as_ref())
            .await
    })
}

#[tokio::test]
async fn peek_returns_a_fresh_status() {
    let store = Sqlite::open().await;
    let observed_at = whole_second_now() - time::Duration::days(6);
    store
        .repo()
        .upsert(&entry(
            PlatformId::Twitch,
            VIEWER,
            KnownFollow::FollowingSince(unix(FOLLOWED_AT_UNIX_SECS)),
            observed_at,
            time::Duration::days(7),
        ))
        .await
        .unwrap();
    let rig = store.spawn();

    assert_eq!(
        rig.handle.peek(PlatformId::Twitch, VIEWER).await,
        Some(FollowStatus::FollowedSince(unix(FOLLOWED_AT_UNIX_SECS)))
    );
}

#[tokio::test]
async fn peek_hides_an_expired_status() {
    let store = Sqlite::open().await;
    let rig = store.spawn();
    writer_idle(&rig.handle).await;
    let observed_at = whole_second_now() - time::Duration::hours(2);
    store
        .repo()
        .upsert(&entry(
            PlatformId::Twitch,
            VIEWER,
            KnownFollow::NotFollowing,
            observed_at,
            time::Duration::hours(1),
        ))
        .await
        .unwrap();

    assert_eq!(rig.handle.peek(PlatformId::Twitch, VIEWER).await, None);
}

#[tokio::test]
async fn resolve_answers_a_fresh_status_without_a_lookup() {
    let store = Sqlite::open().await;
    store
        .repo()
        .upsert(&entry(
            PlatformId::Twitch,
            VIEWER,
            KnownFollow::Hidden,
            whole_second_now() - time::Duration::days(1),
            time::Duration::days(7),
        ))
        .await
        .unwrap();
    let rig = store.spawn();
    let lookup = InstantLookup::answering(FollowStatus::NotFollowing);

    let answer = rig
        .handle
        .resolve(PlatformId::Twitch, VIEWER, &lookup)
        .await;

    assert_eq!((answer, lookup.calls()), (FollowStatus::Hidden, 0));
}

#[tokio::test]
async fn resolve_looks_up_an_unknown_viewer_once_and_remembers_the_answer() {
    let store = Sqlite::open().await;
    let rig = store.spawn();
    let lookup = InstantLookup::answering(FollowStatus::NotFollowing);

    let first = rig
        .handle
        .resolve(PlatformId::Twitch, VIEWER, &lookup)
        .await;
    let second = rig
        .handle
        .resolve(PlatformId::Twitch, VIEWER, &lookup)
        .await;

    assert_eq!(
        (first, second, lookup.calls()),
        (FollowStatus::NotFollowing, FollowStatus::NotFollowing, 1)
    );
}

#[tokio::test]
async fn resolve_replaces_an_expired_status_with_a_fresh_lookup() {
    let store = Sqlite::open().await;
    let rig = store.spawn();
    writer_idle(&rig.handle).await;
    store
        .repo()
        .upsert(&entry(
            PlatformId::Twitch,
            VIEWER,
            KnownFollow::NotFollowing,
            whole_second_now() - time::Duration::hours(2),
            time::Duration::hours(1),
        ))
        .await
        .unwrap();
    let followed = FollowStatus::FollowedSince(unix(FOLLOWED_AT_UNIX_SECS));
    let lookup = InstantLookup::answering(followed);

    rig.handle
        .resolve(PlatformId::Twitch, VIEWER, &lookup)
        .await;

    assert_eq!(
        rig.handle.peek(PlatformId::Twitch, VIEWER).await,
        Some(followed)
    );
}

#[tokio::test]
async fn resolve_returns_an_unavailable_answer_without_remembering_it() {
    let store = Sqlite::open().await;
    let rig = store.spawn();
    let lookup = InstantLookup::answering(FollowStatus::Unavailable);

    let answer = rig
        .handle
        .resolve(PlatformId::Twitch, VIEWER, &lookup)
        .await;

    assert_eq!(
        (answer, rig.handle.peek(PlatformId::Twitch, VIEWER).await),
        (FollowStatus::Unavailable, None)
    );
}

#[tokio::test(start_paused = true)]
async fn resolve_gives_up_on_a_lookup_that_never_answers() {
    let rig = spawn_rig(
        Arc::new(MemoryFollows::default()),
        Arc::new(NullEventLogRepo),
    );
    let silent = GatedLookup::answering(FollowStatus::NotFollowing);

    let resolving = spawn_resolve(&rig.handle, silent.clone());

    assert_eq!(guarded(resolving).await.unwrap(), FollowStatus::Unavailable);
}

#[tokio::test]
async fn concurrent_resolves_of_one_viewer_share_a_single_lookup() {
    let rig = spawn_rig(
        Arc::new(MemoryFollows::default()),
        Arc::new(NullEventLogRepo),
    );
    let followed = FollowStatus::FollowedSince(unix(FOLLOWED_AT_UNIX_SECS));
    let lookup = GatedLookup::answering(followed);

    let resolving: Vec<_> = (0..CONCURRENT_RESOLVES)
        .map(|_| spawn_resolve(&rig.handle, lookup.clone()))
        .collect();
    guarded(lookup.entered.notified()).await;
    tokio::task::yield_now().await;
    lookup.release.notify_one();
    let mut answers = Vec::new();
    for task in resolving {
        answers.push(guarded(task).await.unwrap());
    }

    assert_eq!(
        (answers, lookup.calls()),
        (vec![followed; CONCURRENT_RESOLVES], 1)
    );
}

#[tokio::test]
async fn a_waiting_resolve_takes_over_when_the_leading_one_is_cancelled() {
    let rig = spawn_rig(
        Arc::new(MemoryFollows::default()),
        Arc::new(NullEventLogRepo),
    );
    let stuck = GatedLookup::answering(FollowStatus::Hidden);
    let leader = spawn_resolve(&rig.handle, stuck.clone());
    guarded(stuck.entered.notified()).await;
    let answering = Arc::new(InstantLookup::answering(FollowStatus::NotFollowing));
    let waiter = spawn_resolve(&rig.handle, answering.clone());
    tokio::task::yield_now().await;

    leader.abort();

    assert_eq!(guarded(waiter).await.unwrap(), FollowStatus::NotFollowing);
}

#[tokio::test]
async fn a_follow_event_newer_than_a_pending_lookup_outlives_its_answer() {
    let store = Sqlite::open().await;
    let rig = store.spawn();
    let mut changes = rig.handle.changes();
    let lookup = GatedLookup::answering(FollowStatus::NotFollowing);
    let resolving = spawn_resolve(&rig.handle, lookup.clone());
    guarded(lookup.entered.notified()).await;
    rig.bus.publish(twitch_follow(
        VIEWER,
        FOLLOWED_AT_UNIX_SECS,
        whole_second_now() + time::Duration::seconds(2),
    ));
    next_change(&mut changes).await;

    lookup.release.notify_one();

    assert_eq!(
        guarded(resolving).await.unwrap(),
        FollowStatus::FollowedSince(unix(FOLLOWED_AT_UNIX_SECS))
    );
}

#[tokio::test]
async fn a_lookup_answer_that_loses_to_a_newer_follow_signals_no_change() {
    let store = Sqlite::open().await;
    let rig = store.spawn();
    let mut changes = rig.handle.changes();
    let lookup = GatedLookup::answering(FollowStatus::NotFollowing);
    let resolving = spawn_resolve(&rig.handle, lookup.clone());
    guarded(lookup.entered.notified()).await;
    rig.bus.publish(twitch_follow(
        VIEWER,
        FOLLOWED_AT_UNIX_SECS,
        whole_second_now() + time::Duration::seconds(2),
    ));
    next_change(&mut changes).await;

    lookup.release.notify_one();
    guarded(resolving).await.unwrap();

    assert!(!change_pending(&mut changes));
}

#[tokio::test]
async fn a_lookup_issued_after_a_stored_follow_event_replaces_it() {
    let store = Sqlite::open().await;
    let rig = store.spawn();
    let mut changes = rig.handle.changes();
    let lookup = GatedLookup::answering(FollowStatus::NotFollowing);
    let resolving = spawn_resolve(&rig.handle, lookup.clone());
    guarded(lookup.entered.notified()).await;
    rig.bus.publish(twitch_follow(
        VIEWER,
        FOLLOWED_AT_UNIX_SECS,
        whole_second_now() - time::Duration::hours(1),
    ));
    next_change(&mut changes).await;

    lookup.release.notify_one();
    guarded(resolving).await.unwrap();

    assert_eq!(
        rig.handle.peek(PlatformId::Twitch, VIEWER).await,
        Some(FollowStatus::NotFollowing)
    );
}

#[tokio::test]
async fn a_follow_event_records_the_follow_moment_it_reports() {
    let store = Sqlite::open().await;
    let rig = store.spawn();
    let mut changes = rig.handle.changes();

    rig.bus.publish(twitch_follow(
        VIEWER,
        FOLLOWED_AT_UNIX_SECS,
        whole_second_now(),
    ));
    next_change(&mut changes).await;

    assert_eq!(
        rig.handle.peek(PlatformId::Twitch, VIEWER).await,
        Some(FollowStatus::FollowedSince(unix(FOLLOWED_AT_UNIX_SECS)))
    );
}

#[tokio::test]
async fn a_follow_event_without_a_follow_moment_records_its_own_timestamp() {
    let store = Sqlite::open().await;
    let rig = store.spawn();
    let mut changes = rig.handle.changes();
    let timestamp = whole_second_now();

    rig.bus
        .publish(follow_event(EventSource::Kick, VIEWER, None, timestamp));
    next_change(&mut changes).await;

    assert_eq!(
        rig.handle.peek(PlatformId::Kick, VIEWER).await,
        Some(FollowStatus::FollowedSince(timestamp))
    );
}

#[tokio::test]
async fn a_follow_event_replaces_a_remembered_not_following() {
    let store = Sqlite::open().await;
    store
        .repo()
        .upsert(&entry(
            PlatformId::Twitch,
            VIEWER,
            KnownFollow::NotFollowing,
            whole_second_now() - time::Duration::minutes(10),
            time::Duration::hours(1),
        ))
        .await
        .unwrap();
    let rig = store.spawn();
    let mut changes = rig.handle.changes();

    rig.bus.publish(twitch_follow(
        VIEWER,
        FOLLOWED_AT_UNIX_SECS,
        whole_second_now(),
    ));
    next_change(&mut changes).await;

    assert_eq!(
        rig.handle.peek(PlatformId::Twitch, VIEWER).await,
        Some(FollowStatus::FollowedSince(unix(FOLLOWED_AT_UNIX_SECS)))
    );
}

#[tokio::test]
async fn events_that_are_not_live_platform_follows_are_not_remembered() {
    let store = Sqlite::open().await;
    let rig = store.spawn();
    let mut changes = rig.handle.changes();
    let now = whole_second_now();
    let mut replayed = twitch_follow("replayed", FOLLOWED_AT_UNIX_SECS, now);
    replayed.replay = true;
    let mut chat = twitch_follow("chatter", FOLLOWED_AT_UNIX_SECS, now);
    chat.kind = CHAT_KIND.to_owned();
    let core = follow_event(EventSource::Core, "core", Some(FOLLOWED_AT_UNIX_SECS), now);
    let mut anonymous = twitch_follow("", FOLLOWED_AT_UNIX_SECS, now);
    anonymous.payload = serde_json::json!({ FOLLOWED_AT_KEY: FOLLOWED_AT_UNIX_SECS });
    let unnamed = twitch_follow("", FOLLOWED_AT_UNIX_SECS, now);

    for event in [replayed, chat, core, anonymous, unnamed] {
        rig.bus.publish(event);
    }
    rig.bus
        .publish(twitch_follow(SENTINEL_VIEWER, FOLLOWED_AT_UNIX_SECS, now));
    next_change(&mut changes).await;

    for viewer_id in ["replayed", "chatter", "core", ""] {
        assert_eq!(
            rig.handle.peek(PlatformId::Twitch, viewer_id).await,
            None,
            "{viewer_id:?} must not be remembered"
        );
    }
}

#[tokio::test]
async fn startup_prunes_expired_entries_before_checking_the_seed() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut repo = MockFollowKnowledgeRepo::new();
    let pruned = Arc::clone(&calls);
    repo.expect_prune_expired().returning(move |_| {
        pruned.lock().unwrap().push("prune");
        Ok(0)
    });
    let checked = Arc::clone(&calls);
    repo.expect_is_seeded().returning(move || {
        checked.lock().unwrap().push("seed check");
        Ok(true)
    });
    repo.expect_clear_platform().returning(|_| Ok(0));
    let rig = spawn_rig(Arc::new(repo), Arc::new(NullEventLogRepo));

    writer_idle(&rig.handle).await;

    assert_eq!(*calls.lock().unwrap(), ["prune", "seed check"]);
}

#[tokio::test]
async fn startup_does_not_seed_an_already_seeded_store() {
    let store = Sqlite::open().await;
    store.repo().seed(&[]).await.unwrap();
    store
        .log(&twitch_follow(
            VIEWER,
            FOLLOWED_AT_UNIX_SECS,
            whole_second_now() - time::Duration::days(1),
        ))
        .await;
    let rig = store.spawn();

    writer_idle(&rig.handle).await;

    assert_eq!(rig.handle.peek(PlatformId::Twitch, VIEWER).await, None);
}

#[tokio::test]
async fn seeding_remembers_the_newest_logged_follow_of_each_viewer() {
    let store = Sqlite::open().await;
    let now = whole_second_now();
    store
        .log(&twitch_follow(
            VIEWER,
            LATER_FOLLOWED_AT_UNIX_SECS,
            now - time::Duration::days(1),
        ))
        .await;
    store
        .log(&twitch_follow(
            VIEWER,
            FOLLOWED_AT_UNIX_SECS,
            now - time::Duration::days(3),
        ))
        .await;
    let rig = store.spawn();

    writer_idle(&rig.handle).await;

    assert_eq!(
        rig.handle.peek(PlatformId::Twitch, VIEWER).await,
        Some(FollowStatus::FollowedSince(unix(
            LATER_FOLLOWED_AT_UNIX_SECS
        )))
    );
}

#[tokio::test]
async fn seeding_skips_follows_logged_more_than_a_week_ago() {
    let store = Sqlite::open().await;
    let now = whole_second_now();
    store
        .log(&twitch_follow(
            VIEWER,
            FOLLOWED_AT_UNIX_SECS,
            now - time::Duration::days(8),
        ))
        .await;
    store
        .log(&twitch_follow(
            OTHER_VIEWER,
            FOLLOWED_AT_UNIX_SECS,
            now - time::Duration::days(6),
        ))
        .await;
    let rig = store.spawn();

    writer_idle(&rig.handle).await;

    assert_eq!(
        (
            rig.handle.peek(PlatformId::Twitch, VIEWER).await,
            rig.handle.peek(PlatformId::Twitch, OTHER_VIEWER).await,
        ),
        (
            None,
            Some(FollowStatus::FollowedSince(unix(FOLLOWED_AT_UNIX_SECS)))
        )
    );
}

#[tokio::test]
async fn seeding_an_empty_event_log_still_marks_the_store_seeded() {
    let store = Sqlite::open().await;
    let rig = store.spawn();

    writer_idle(&rig.handle).await;

    assert!(store.repo().is_seeded().await.unwrap());
}

#[tokio::test]
async fn an_unreadable_event_log_leaves_the_store_unseeded() {
    let store = Sqlite::open().await;
    let rig = spawn_rig(store.repo(), Arc::new(UnreadableEventLog));

    writer_idle(&rig.handle).await;

    assert!(!store.repo().is_seeded().await.unwrap());
}

#[tokio::test]
async fn clearing_a_platform_forgets_only_that_platform() {
    let store = Sqlite::open().await;
    for platform in [PlatformId::Twitch, PlatformId::Kick] {
        store
            .repo()
            .upsert(&entry(
                platform,
                VIEWER,
                KnownFollow::Hidden,
                whole_second_now(),
                time::Duration::days(7),
            ))
            .await
            .unwrap();
    }
    let rig = store.spawn();

    rig.handle.clear_platform(PlatformId::Twitch).await;

    assert_eq!(
        (
            rig.handle.peek(PlatformId::Twitch, VIEWER).await,
            rig.handle.peek(PlatformId::Kick, VIEWER).await,
        ),
        (None, Some(FollowStatus::Hidden))
    );
}

#[derive(Default)]
struct HeldFollows {
    inner: MemoryFollows,
    clear_entered: Notify,
    clear_release: Notify,
    hold_upserts: AtomicBool,
    upsert_entered: Notify,
    upserts: AtomicUsize,
}

impl HeldFollows {
    fn remembered(&self, platform: PlatformId) -> Vec<String> {
        let mut viewers: Vec<String> = self
            .inner
            .entries
            .lock()
            .unwrap()
            .keys()
            .filter(|key| key.platform == platform)
            .map(|key| key.viewer_id.clone())
            .collect();
        viewers.sort();
        viewers
    }

    fn upserts(&self) -> usize {
        self.upserts.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl FollowKnowledgeRepo for HeldFollows {
    async fn get(
        &self,
        key: &FollowKnowledgeKey,
    ) -> Result<Option<FollowKnowledgeEntry>, StorageError> {
        self.inner.get(key).await
    }

    async fn upsert(&self, entry: &FollowKnowledgeEntry) -> Result<(), StorageError> {
        self.upserts.fetch_add(1, Ordering::SeqCst);
        self.upsert_entered.notify_one();
        if self.hold_upserts.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        self.inner.upsert(entry).await
    }

    async fn seed(&self, entries: &[FollowKnowledgeEntry]) -> Result<(), StorageError> {
        self.inner.seed(entries).await
    }

    async fn is_seeded(&self) -> Result<bool, StorageError> {
        self.inner.is_seeded().await
    }

    async fn prune_expired(&self, now: OffsetDateTime) -> Result<u64, StorageError> {
        self.inner.prune_expired(now).await
    }

    async fn clear_platform(&self, platform: PlatformId) -> Result<u64, StorageError> {
        self.clear_entered.notify_one();
        self.clear_release.notified().await;
        self.inner.clear_platform(platform).await
    }
}

#[derive(Default)]
struct StuckSeedRead {
    entered: Notify,
}

#[async_trait]
impl EventLogRepo for StuckSeedRead {
    async fn insert(&self, _event: &Event) -> Result<(), StorageError> {
        Ok(())
    }

    async fn get(&self, _id: EventId) -> Result<Option<Event>, StorageError> {
        Ok(None)
    }

    async fn recent(&self, _limit: usize) -> Result<Vec<Event>, StorageError> {
        Ok(Vec::new())
    }

    async fn recent_since(
        &self,
        _limit: usize,
        _since: Option<EventId>,
    ) -> Result<Vec<Event>, StorageError> {
        Ok(Vec::new())
    }

    async fn events_of_kinds_since(
        &self,
        _kinds: &[String],
        _since: OffsetDateTime,
    ) -> Result<Vec<Event>, StorageError> {
        self.entered.notify_one();
        std::future::pending().await
    }
}

fn spawn_clear(handle: &FollowKnowledgeHandle) -> JoinHandle<()> {
    let handle = handle.clone();
    tokio::spawn(async move { handle.clear_platform(PlatformId::Twitch).await })
}

async fn flush_completes(bus: &EventBus) -> bool {
    tokio::time::timeout(HANG_GUARD, bus.await_flush())
        .await
        .is_ok()
}

fn publish_follows_of(bus: &EventBus, viewers: &[&str]) {
    let now = whole_second_now();
    for viewer_id in viewers {
        bus.publish(twitch_follow(viewer_id, FOLLOWED_AT_UNIX_SECS, now));
    }
}

#[tokio::test(start_paused = true)]
async fn shutting_down_an_idle_writer_completes_the_flush() {
    let rig = spawn_rig(
        Arc::new(MemoryFollows::default()),
        Arc::new(NullEventLogRepo),
    );
    writer_idle(&rig.handle).await;

    rig.bus.shutdown();

    assert!(flush_completes(&rig.bus).await);
}

#[tokio::test(start_paused = true)]
async fn shutting_down_during_a_stuck_startup_seed_completes_the_flush_at_once() {
    let event_log = Arc::new(StuckSeedRead::default());
    let rig = spawn_rig(Arc::new(MemoryFollows::default()), event_log.clone());
    guarded(event_log.entered.notified()).await;
    let stopped_at = tokio::time::Instant::now();

    rig.bus.shutdown();
    guarded(rig.bus.await_flush()).await;

    assert_eq!(stopped_at.elapsed(), Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn follow_events_queued_at_shutdown_are_written_before_the_flush_completes() {
    let repo = Arc::new(HeldFollows::default());
    let rig = spawn_rig(repo.clone(), Arc::new(NullEventLogRepo));
    let _clearing = spawn_clear(&rig.handle);
    guarded(repo.clear_entered.notified()).await;
    publish_follows_of(&rig.bus, &[VIEWER, OTHER_VIEWER]);

    rig.bus.shutdown();
    repo.clear_release.notify_one();
    guarded(rig.bus.await_flush()).await;

    assert_eq!(repo.remembered(PlatformId::Twitch), [VIEWER, OTHER_VIEWER]);
}

#[tokio::test(start_paused = true)]
async fn abandoning_the_flush_stops_draining_queued_follow_events() {
    let repo = Arc::new(HeldFollows::default());
    let rig = spawn_rig(repo.clone(), Arc::new(NullEventLogRepo));
    let _clearing = spawn_clear(&rig.handle);
    guarded(repo.clear_entered.notified()).await;
    publish_follows_of(&rig.bus, &[VIEWER, OTHER_VIEWER]);
    rig.bus.shutdown();
    repo.hold_upserts.store(true, Ordering::SeqCst);
    repo.clear_release.notify_one();
    guarded(repo.upsert_entered.notified()).await;

    guarded(rig.bus.abandon_flush()).await;

    assert_eq!(repo.upserts(), 1);
}

#[tokio::test(start_paused = true)]
async fn clearing_after_the_writer_has_flushed_returns() {
    let rig = spawn_rig(
        Arc::new(MemoryFollows::default()),
        Arc::new(NullEventLogRepo),
    );
    writer_idle(&rig.handle).await;
    rig.bus.shutdown();
    guarded(rig.bus.await_flush()).await;

    let cleared =
        tokio::time::timeout(HANG_GUARD, rig.handle.clear_platform(PlatformId::Twitch)).await;

    assert!(cleared.is_ok());
}

#[tokio::test(start_paused = true)]
async fn a_clear_queued_behind_a_stuck_startup_returns_as_soon_as_the_bus_shuts_down() {
    let event_log = Arc::new(StuckSeedRead::default());
    let rig = spawn_rig(Arc::new(MemoryFollows::default()), event_log.clone());
    guarded(event_log.entered.notified()).await;
    let clearing = spawn_clear(&rig.handle);
    tokio::task::yield_now().await;
    let stopped_at = tokio::time::Instant::now();

    rig.bus.shutdown();
    guarded(clearing).await.unwrap();

    assert_eq!(stopped_at.elapsed(), Duration::ZERO);
}
