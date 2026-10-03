use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use forge_events::{Event, EventPublisher, LatestChanged};
use forge_storage::{LatestRecord, LatestValueRepo, StorageError};
use forge_types::{EventId, LatestScope, LatestValue, LatestValueReader, Shared, Variant};
use time::OffsetDateTime;

use crate::latest::slots::{
    FeedContext, LATEST_SLOTS, LatestSlotDeclaration, SlotReading, SlotRetention, slot_declaration,
};

const STORAGE_OP_TIMEOUT: Duration = Duration::from_secs(5);
const PERSIST_ATTEMPTS: u32 = 3;
const PERSIST_RETRY_DELAY: Duration = Duration::from_millis(500);
const TEST_MARKER: &str = "test";

type SlotValues = BTreeMap<String, LatestValue>;

#[derive(Clone)]
pub struct LatestValues {
    values: Arc<RwLock<HashMap<&'static str, SlotValues>>>,
    repo: Arc<dyn LatestValueRepo>,
    publisher: Arc<dyn EventPublisher>,
    anonymous_name: Shared<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LatestResetError {
    #[error("unknown latest slot '{0}'")]
    UnknownSlot(String),
    #[error("latest slot could not be reset: {0}")]
    Storage(#[from] StorageError),
}

impl LatestValues {
    pub async fn load(
        repo: Arc<dyn LatestValueRepo>,
        publisher: Arc<dyn EventPublisher>,
        anonymous_name: Shared<String>,
    ) -> Self {
        let mut values = HashMap::new();
        for declaration in LATEST_SLOTS {
            if declaration.retention == SlotRetention::SurvivesRestart {
                values.insert(
                    declaration.slot,
                    stored_values(repo.as_ref(), declaration).await,
                );
            }
        }
        Self {
            values: Arc::new(RwLock::new(values)),
            repo,
            publisher,
            anonymous_name,
        }
    }

    pub fn latest(&self, slot: &str, scope: LatestScope<'_>) -> Option<LatestValue> {
        let values = self.values.read().unwrap_or_else(PoisonError::into_inner);
        let slot_values = values.get(slot)?;
        match scope {
            LatestScope::Platform(platform) => slot_values.get(platform).cloned(),
            LatestScope::MostRecentAcrossPlatforms => most_recent(slot_values).cloned(),
        }
    }

    pub async fn reset(&self, slot: &str) -> Result<(), LatestResetError> {
        let declaration =
            slot_declaration(slot).ok_or_else(|| LatestResetError::UnknownSlot(slot.to_owned()))?;
        if declaration.retention == SlotRetention::SurvivesRestart {
            with_timeout(self.repo.reset_slot(declaration.slot)).await?;
        }
        let removed = {
            let mut values = self.values.write().unwrap_or_else(PoisonError::into_inner);
            values
                .remove(declaration.slot)
                .is_some_and(|slot_values| !slot_values.is_empty())
        };
        if removed {
            self.announce(
                LatestChanged {
                    slot: declaration.slot.to_owned(),
                    platform: None,
                    merged_changed: true,
                    cleared: true,
                },
                None,
            );
        }
        Ok(())
    }

    pub(crate) async fn project(&self, event: &Event) {
        if !admits(event) {
            return;
        }
        let anonymous_name = self.anonymous_name.load();
        let context = FeedContext {
            anonymous_name: anonymous_name.as_str(),
        };
        for declaration in LATEST_SLOTS {
            for feed in declaration
                .feeds
                .iter()
                .filter(|feed| feed.kind == event.kind)
            {
                if let Some(reading) = (feed.read)(event, &context) {
                    self.settle(declaration, reading, event.id).await;
                }
            }
        }
    }

    async fn settle(
        &self,
        declaration: &'static LatestSlotDeclaration,
        reading: SlotReading,
        cause: EventId,
    ) {
        match reading {
            SlotReading::Value(value) => self.accept(declaration, value, cause).await,
            SlotReading::Ended { platform } => {
                if declaration.retention == SlotRetention::LiveOnly {
                    self.end(declaration.slot, platform, cause);
                }
            }
        }
    }

    async fn accept(
        &self,
        declaration: &'static LatestSlotDeclaration,
        value: LatestValue,
        cause: EventId,
    ) {
        if !self.is_newer(declaration.slot, &value) {
            return;
        }
        if declaration.retention == SlotRetention::SurvivesRestart
            && !self.persist(declaration.slot, &value).await
        {
            return;
        }
        let platform = value.platform.clone();
        let merged_changed = {
            let mut values = self.values.write().unwrap_or_else(PoisonError::into_inner);
            let slot_values = values.entry(declaration.slot).or_default();
            let before = merged_identity(slot_values);
            slot_values.insert(platform.clone(), value);
            before != merged_identity(slot_values)
        };
        self.announce(
            LatestChanged {
                slot: declaration.slot.to_owned(),
                platform: Some(platform),
                merged_changed,
                cleared: false,
            },
            Some(cause),
        );
    }

    fn end(&self, slot: &'static str, platform: String, cause: EventId) {
        let merged_changed = {
            let mut values = self.values.write().unwrap_or_else(PoisonError::into_inner);
            let Some(slot_values) = values.get_mut(slot) else {
                return;
            };
            let before = merged_identity(slot_values);
            if slot_values.remove(&platform).is_none() {
                return;
            }
            before != merged_identity(slot_values)
        };
        self.announce(
            LatestChanged {
                slot: slot.to_owned(),
                platform: Some(platform),
                merged_changed,
                cleared: true,
            },
            Some(cause),
        );
    }

    fn is_newer(&self, slot: &str, value: &LatestValue) -> bool {
        let values = self.values.read().unwrap_or_else(PoisonError::into_inner);
        values
            .get(slot)
            .and_then(|slot_values| slot_values.get(&value.platform))
            .is_none_or(|stored| value.occurred_at > stored.occurred_at)
    }

    async fn persist(&self, slot: &str, value: &LatestValue) -> bool {
        let record = LatestRecord {
            slot: slot.to_owned(),
            platform: value.platform.clone(),
            payload: value.to_variant().to_json(),
            occurred_at: value.occurred_at,
            updated_at: OffsetDateTime::now_utc(),
        };
        let mut attempt = 1;
        loop {
            match with_timeout(self.repo.upsert_if_newer(&record)).await {
                Ok(stored) => return stored,
                Err(error) if attempt >= PERSIST_ATTEMPTS => {
                    tracing::error!(
                        slot,
                        platform = %value.platform,
                        error = %error,
                        "latest value could not be saved; it shows until restart"
                    );
                    return true;
                }
                Err(error) => {
                    tracing::warn!(slot, attempt, error = %error, "saving a latest value failed; retrying");
                    attempt += 1;
                    tokio::time::sleep(PERSIST_RETRY_DELAY).await;
                }
            }
        }
    }

    fn announce(&self, change: LatestChanged, cause: Option<EventId>) {
        match change.into_event(cause) {
            Ok(event) => self.publisher.publish(event),
            Err(error) => tracing::warn!(error = %error, "latest change event not encodable"),
        }
    }
}

impl LatestValueReader for LatestValues {
    fn latest(&self, slot: &str, scope: LatestScope<'_>) -> Option<LatestValue> {
        LatestValues::latest(self, slot, scope)
    }
}

fn admits(event: &Event) -> bool {
    let test_marked = event
        .payload
        .get(TEST_MARKER)
        .and_then(|marker| marker.as_bool())
        .unwrap_or(false);
    !event.replay && !test_marked
}

fn most_recent(slot_values: &SlotValues) -> Option<&LatestValue> {
    slot_values.values().max_by_key(|value| value.occurred_at)
}

fn merged_identity(slot_values: &SlotValues) -> Option<(String, OffsetDateTime)> {
    most_recent(slot_values).map(|value| (value.platform.clone(), value.occurred_at))
}

async fn with_timeout<T>(
    operation: impl Future<Output = Result<T, StorageError>>,
) -> Result<T, StorageError> {
    tokio::time::timeout(STORAGE_OP_TIMEOUT, operation)
        .await
        .unwrap_or_else(|_| {
            Err(StorageError::Connection {
                reason: "latest value storage timed out".to_owned(),
            })
        })
}

async fn stored_values(
    repo: &dyn LatestValueRepo,
    declaration: &LatestSlotDeclaration,
) -> SlotValues {
    let records = match with_timeout(repo.list_slot(declaration.slot)).await {
        Ok(records) => records,
        Err(error) => {
            tracing::warn!(slot = declaration.slot, error = %error, "stored latest values unreadable; slot starts empty");
            return SlotValues::new();
        }
    };
    records
        .into_iter()
        .filter_map(|record| match Variant::from_json(record.payload) {
            Ok(Variant::Object(fields)) => Some((
                record.platform.clone(),
                LatestValue::new(record.platform, record.occurred_at, fields),
            )),
            _ => {
                tracing::warn!(
                    slot = declaration.slot,
                    platform = %record.platform,
                    "stored latest value is malformed; skipped"
                );
                None
            }
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_events::{DonationReceived, EventSource, NowPlaying, PlaybackState};
    use forge_types::{
        CurrencyCode, DonorVisibility, IntegrationId, LATEST_DONATION_SLOT, NOW_PLAYING_SLOT,
        latest_fields,
    };
    use serde_json::json;
    use tracing::Level;

    use super::*;
    use crate::latest::feeds::CHEER_KIND;
    use crate::test_support::log_capture::capture;

    const ANONYMOUS: &str = "Anonymous donor";
    const BASE_UNIX: i64 = 1_790_000_000;

    #[derive(Default)]
    struct Store {
        rows: Mutex<Vec<LatestRecord>>,
        upserts: Mutex<Vec<LatestRecord>>,
        upsert_replies: Mutex<VecDeque<Result<bool, StorageError>>>,
        resets: Mutex<Vec<String>>,
        list_fails: bool,
        reset_fails: bool,
    }

    fn storage_failure() -> StorageError {
        StorageError::Connection {
            reason: "disk unplugged".to_owned(),
        }
    }

    impl Store {
        fn holding(rows: Vec<LatestRecord>) -> Self {
            Self {
                rows: Mutex::new(rows),
                ..Self::default()
            }
        }

        fn replying(replies: Vec<Result<bool, StorageError>>) -> Self {
            Self {
                upsert_replies: Mutex::new(replies.into()),
                ..Self::default()
            }
        }

        fn upsert_count(&self) -> usize {
            self.upserts.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl LatestValueRepo for Store {
        async fn upsert_if_newer(&self, record: &LatestRecord) -> Result<bool, StorageError> {
            self.upserts.lock().unwrap().push(record.clone());
            self.upsert_replies
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(true))
        }

        async fn list_slot(&self, slot: &str) -> Result<Vec<LatestRecord>, StorageError> {
            if self.list_fails {
                return Err(storage_failure());
            }
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .filter(|row| row.slot == slot)
                .cloned()
                .collect())
        }

        async fn reset_slot(&self, slot: &str) -> Result<u64, StorageError> {
            if self.reset_fails {
                return Err(storage_failure());
            }
            self.resets.lock().unwrap().push(slot.to_owned());
            Ok(1)
        }
    }

    #[derive(Default)]
    struct Recorder(Mutex<Vec<Event>>);

    impl EventPublisher for Recorder {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    impl Recorder {
        fn events(&self) -> Vec<Event> {
            self.0.lock().unwrap().clone()
        }

        fn changes(&self) -> Vec<LatestChanged> {
            self.events()
                .iter()
                .map(|event| LatestChanged::from_event(event).expect("only latest.changed"))
                .collect()
        }
    }

    async fn rig(store: Store) -> (LatestValues, Arc<Store>, Arc<Recorder>) {
        let store = Arc::new(store);
        let recorder = Arc::new(Recorder::default());
        let values = LatestValues::load(
            Arc::clone(&store) as Arc<dyn LatestValueRepo>,
            Arc::clone(&recorder) as Arc<dyn EventPublisher>,
            Shared::new(ANONYMOUS.to_owned()),
        )
        .await;
        (values, store, recorder)
    }

    fn at(offset_secs: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(BASE_UNIX + offset_secs).unwrap()
    }

    fn donation(provider: &str, donor: &str, offset_secs: i64, test: bool) -> Event {
        donation_at(provider, donor, at(offset_secs), test)
    }

    fn donation_at(provider: &str, donor: &str, occurred_at: OffsetDateTime, test: bool) -> Event {
        DonationReceived {
            provider: IntegrationId::new(provider),
            donation_id: format!("{provider}-{donor}"),
            donor_name: donor.to_owned(),
            donor_visibility: DonorVisibility::Named,
            message: Some("a private message".to_owned()),
            amount_micros: 12_500_000,
            currency: CurrencyCode::parse("UAH").unwrap(),
            occurred_at,
            test,
        }
        .into_event()
        .unwrap()
    }

    fn cheer(login: &str, offset_secs: i64) -> Event {
        let mut event = Event::new(
            EventSource::Twitch,
            CHEER_KIND,
            json!({
                "bits": 500,
                "is_anonymous": false,
                "message": "cheer500",
                "user": { "id": "1", "login": login, "display_name": login },
            }),
        );
        event.timestamp = at(offset_secs);
        event
    }

    fn now_playing(producer: &str, state: PlaybackState, title: &str) -> Event {
        NowPlaying {
            producer: IntegrationId::new(producer),
            state,
            title: title.to_owned(),
            artist: "Artist".to_owned(),
            album: None,
            art: None,
            occurred_at: OffsetDateTime::now_utc(),
        }
        .into_event(EventSource::Server)
        .unwrap()
    }

    fn user_name(value: Option<LatestValue>) -> Option<String> {
        value.and_then(|value| match value.fields.get(latest_fields::USER_NAME) {
            Some(Variant::String(name)) => Some(name.clone()),
            _ => None,
        })
    }

    fn merged_donor(values: &LatestValues) -> Option<String> {
        user_name(values.latest(LATEST_DONATION_SLOT, LatestScope::MostRecentAcrossPlatforms))
    }

    fn stored_row(slot: &str, platform: &str, payload: serde_json::Value) -> LatestRecord {
        LatestRecord {
            slot: slot.to_owned(),
            platform: platform.to_owned(),
            payload,
            occurred_at: at(0),
            updated_at: at(0),
        }
    }

    #[tokio::test]
    async fn accepted_donation_is_stored_and_announced_as_caused_by_its_event() {
        let (values, store, recorder) = rig(Store::default()).await;
        let event = donation("donatello", "Olena", 10, false);

        values.project(&event).await;

        let announced = recorder.events();
        assert_eq!(merged_donor(&values).as_deref(), Some("Olena"));
        assert_eq!(store.upsert_count(), 1);
        assert_eq!(announced.len(), 1);
        assert_eq!(announced[0].caused_by, Some(event.id));
        assert_eq!(
            LatestChanged::from_event(&announced[0]),
            Some(LatestChanged {
                slot: LATEST_DONATION_SLOT.to_owned(),
                platform: Some("donatello".to_owned()),
                merged_changed: true,
                cleared: false,
            })
        );
    }

    #[tokio::test]
    async fn replayed_feeding_event_is_ignored() {
        let (values, store, recorder) = rig(Store::default()).await;
        let mut replayed = cheer("generous_one", 10);
        replayed.replay = true;

        values.project(&replayed).await;

        assert_eq!(merged_donor(&values), None);
        assert_eq!(store.upsert_count(), 0);
        assert!(recorder.events().is_empty());
    }

    #[tokio::test]
    async fn test_marked_payload_never_overwrites_the_slot() {
        let mut marked_cheer = cheer("tester", 30);
        marked_cheer.payload["test"] = json!(true);
        for marked in [donation("donatello", "Tester", 30, true), marked_cheer] {
            let (values, store, recorder) = rig(Store::default()).await;
            values
                .project(&donation("donatello", "Olena", 10, false))
                .await;

            values.project(&marked).await;

            assert_eq!(merged_donor(&values).as_deref(), Some("Olena"));
            assert_eq!(store.upsert_count(), 1, "{}", marked.kind);
            assert_eq!(recorder.events().len(), 1, "{}", marked.kind);
        }
    }

    #[tokio::test]
    async fn value_not_strictly_newer_than_the_record_is_rejected() {
        let first = at(10);
        let sub_millisecond_later = first + time::Duration::nanoseconds(400_000);
        for (label, occurred_at) in [
            ("older", at(9)),
            ("equal", first),
            ("same millisecond", sub_millisecond_later),
        ] {
            let (values, store, recorder) = rig(Store::default()).await;
            values
                .project(&donation_at("donatello", "Olena", first, false))
                .await;

            values
                .project(&donation_at("donatello", "Taras", occurred_at, false))
                .await;

            assert_eq!(merged_donor(&values).as_deref(), Some("Olena"), "{label}");
            assert_eq!(store.upsert_count(), 1, "{label}");
            assert_eq!(recorder.events().len(), 1, "{label}");
        }
    }

    #[tokio::test]
    async fn value_one_millisecond_newer_replaces_the_record() {
        let (values, _store, recorder) = rig(Store::default()).await;
        let first = at(10);
        values
            .project(&donation_at("donatello", "Olena", first, false))
            .await;

        values
            .project(&donation_at(
                "donatello",
                "Taras",
                first + time::Duration::milliseconds(1),
                false,
            ))
            .await;

        assert_eq!(merged_donor(&values).as_deref(), Some("Taras"));
        assert_eq!(recorder.events().len(), 2);
    }

    #[tokio::test]
    async fn storage_holding_a_newer_value_changes_nothing() {
        let (values, store, recorder) = rig(Store::replying(vec![Ok(false)])).await;

        values
            .project(&donation("donatello", "Olena", 10, false))
            .await;

        assert_eq!(store.upsert_count(), 1);
        assert_eq!(merged_donor(&values), None);
        assert!(recorder.events().is_empty());
    }

    fn paused_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap()
    }

    fn project_on_paused_clock(values: &LatestValues, event: &Event) {
        paused_runtime().block_on(values.project(event));
    }

    fn rig_blocking(store: Store) -> (LatestValues, Arc<Store>, Arc<Recorder>) {
        paused_runtime().block_on(rig(store))
    }

    fn failing(times: usize) -> Store {
        Store::replying((0..times).map(|_| Err(storage_failure())).collect())
    }

    #[test]
    fn value_still_shows_and_is_announced_when_every_save_attempt_fails() {
        let (values, store, recorder) = rig_blocking(failing(3));

        project_on_paused_clock(&values, &donation("donatello", "Olena", 10, false));

        assert_eq!(store.upsert_count(), 3);
        assert_eq!(merged_donor(&values).as_deref(), Some("Olena"));
        assert_eq!(
            recorder.changes(),
            [LatestChanged {
                slot: LATEST_DONATION_SLOT.to_owned(),
                platform: Some("donatello".to_owned()),
                merged_changed: true,
                cleared: false,
            }]
        );
    }

    #[test]
    fn exhausted_save_attempts_log_one_error_without_the_donor_name() {
        let (values, _store, _recorder) = rig_blocking(failing(3));

        let lines = capture(Level::WARN, || {
            project_on_paused_clock(&values, &donation("donatello", "Olena", 10, false));
        });

        let errors: Vec<_> = lines
            .iter()
            .filter(|line| line.level == Level::ERROR)
            .collect();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field("platform"), "donatello");
        assert!(!lines.iter().any(|line| line.mentions("Olena")));
    }

    #[test]
    fn save_succeeding_on_the_last_attempt_logs_no_error() {
        let (values, store, _recorder) = rig_blocking(Store::replying(vec![
            Err(storage_failure()),
            Err(storage_failure()),
            Ok(true),
        ]));

        let lines = capture(Level::WARN, || {
            project_on_paused_clock(&values, &donation("donatello", "Olena", 10, false));
        });

        assert_eq!(store.upsert_count(), 3);
        assert!(!lines.iter().any(|line| line.level == Level::ERROR));
    }

    #[tokio::test]
    async fn merged_change_flag_tracks_the_most_recent_platform() {
        let (values, _store, recorder) = rig(Store::default()).await;

        values
            .project(&donation("donatello", "Olena", 20, false))
            .await;
        values
            .project(&donation("monobank", "Taras", 10, false))
            .await;
        values.project(&cheer("generous_one", 30)).await;
        values
            .project(&donation("monobank", "Ivan", 25, false))
            .await;

        let flags: Vec<(Option<String>, bool)> = recorder
            .changes()
            .into_iter()
            .map(|change| (change.platform, change.merged_changed))
            .collect();
        assert_eq!(
            flags,
            [
                (Some("donatello".to_owned()), true),
                (Some("monobank".to_owned()), false),
                (Some("twitch".to_owned()), true),
                (Some("monobank".to_owned()), false),
            ]
        );
    }

    #[tokio::test]
    async fn platform_scope_reads_that_platform_while_merged_reads_the_newest() {
        let (values, _store, _recorder) = rig(Store::default()).await;
        values
            .project(&donation("donatello", "Olena", 20, false))
            .await;
        values
            .project(&donation("monobank", "Taras", 10, false))
            .await;

        let merged = merged_donor(&values);
        let monobank =
            user_name(values.latest(LATEST_DONATION_SLOT, LatestScope::Platform("monobank")));

        assert_eq!(merged.as_deref(), Some("Olena"));
        assert_eq!(monobank.as_deref(), Some("Taras"));
    }

    fn now_playing_title(values: &LatestValues, scope: LatestScope<'_>) -> Option<String> {
        values.latest(NOW_PLAYING_SLOT, scope).and_then(|value| {
            match value.fields.get(latest_fields::TITLE) {
                Some(Variant::String(title)) => Some(title.clone()),
                _ => None,
            }
        })
    }

    #[tokio::test]
    async fn stopped_playback_clears_only_its_own_producer() {
        let (values, _store, recorder) = rig(Store::default()).await;
        values
            .project(&now_playing("spotify", PlaybackState::Playing, "Song A"))
            .await;
        values
            .project(&now_playing("companion", PlaybackState::Playing, "Song B"))
            .await;

        values
            .project(&now_playing("spotify", PlaybackState::Stopped, ""))
            .await;

        assert_eq!(
            now_playing_title(&values, LatestScope::Platform("spotify")),
            None
        );
        assert_eq!(
            now_playing_title(&values, LatestScope::MostRecentAcrossPlatforms).as_deref(),
            Some("Song B")
        );
        let last = recorder.changes().pop().unwrap();
        assert_eq!(last.platform.as_deref(), Some("spotify"));
        assert!(last.cleared);
    }

    #[tokio::test]
    async fn stopped_playback_without_a_record_publishes_nothing() {
        let (values, _store, recorder) = rig(Store::default()).await;
        values
            .project(&now_playing("companion", PlaybackState::Playing, "Song B"))
            .await;

        values
            .project(&now_playing("spotify", PlaybackState::Stopped, ""))
            .await;

        assert_eq!(recorder.events().len(), 1);
    }

    #[tokio::test]
    async fn now_playing_is_never_written_to_storage() {
        let (values, store, recorder) = rig(Store::default()).await;

        values
            .project(&now_playing("spotify", PlaybackState::Playing, "Song A"))
            .await;

        assert_eq!(store.upsert_count(), 0);
        assert_eq!(recorder.events().len(), 1);
    }

    #[tokio::test]
    async fn reset_of_an_unknown_slot_is_rejected_without_touching_storage() {
        let (values, store, _recorder) = rig(Store::default()).await;

        let result = values.reset("no_such_slot").await;

        assert!(
            matches!(result, Err(LatestResetError::UnknownSlot(slot)) if slot == "no_such_slot")
        );
        assert!(store.resets.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn reset_of_an_empty_slot_publishes_nothing() {
        let (values, store, recorder) = rig(Store::default()).await;

        values.reset(LATEST_DONATION_SLOT).await.unwrap();

        assert_eq!(*store.resets.lock().unwrap(), [LATEST_DONATION_SLOT]);
        assert!(recorder.events().is_empty());
    }

    #[tokio::test]
    async fn reset_clears_every_platform_and_announces_an_uncaused_whole_slot_change() {
        let (values, _store, recorder) = rig(Store::default()).await;
        values
            .project(&donation("donatello", "Olena", 20, false))
            .await;
        values
            .project(&donation("monobank", "Taras", 10, false))
            .await;

        values.reset(LATEST_DONATION_SLOT).await.unwrap();

        let announced = recorder.events().pop().unwrap();
        assert_eq!(merged_donor(&values), None);
        assert_eq!(announced.caused_by, None);
        assert_eq!(
            LatestChanged::from_event(&announced),
            Some(LatestChanged {
                slot: LATEST_DONATION_SLOT.to_owned(),
                platform: None,
                merged_changed: true,
                cleared: true,
            })
        );
    }

    #[tokio::test]
    async fn reset_that_storage_refuses_keeps_the_value() {
        let (values, _store, recorder) = rig(Store {
            reset_fails: true,
            ..Store::default()
        })
        .await;
        values
            .project(&donation("donatello", "Olena", 20, false))
            .await;

        let result = values.reset(LATEST_DONATION_SLOT).await;

        assert!(matches!(result, Err(LatestResetError::Storage(_))));
        assert_eq!(merged_donor(&values).as_deref(), Some("Olena"));
        assert_eq!(recorder.events().len(), 1);
    }

    #[tokio::test]
    async fn load_restores_stored_rows_and_skips_malformed_ones() {
        let (values, _store, _recorder) = rig(Store::holding(vec![
            stored_row(
                LATEST_DONATION_SLOT,
                "donatello",
                json!({ "user_name": "Olena" }),
            ),
            stored_row(LATEST_DONATION_SLOT, "monobank", json!("not an object")),
        ]))
        .await;

        let donatello =
            user_name(values.latest(LATEST_DONATION_SLOT, LatestScope::Platform("donatello")));
        let monobank = values.latest(LATEST_DONATION_SLOT, LatestScope::Platform("monobank"));

        assert_eq!(donatello.as_deref(), Some("Olena"));
        assert_eq!(monobank, None);
    }

    #[tokio::test]
    async fn restored_value_rejects_an_older_late_arrival() {
        let (values, store, recorder) = rig(Store::holding(vec![stored_row(
            LATEST_DONATION_SLOT,
            "donatello",
            json!({ "user_name": "Olena" }),
        )]))
        .await;

        values
            .project(&donation("donatello", "Taras", -60, false))
            .await;

        assert_eq!(merged_donor(&values).as_deref(), Some("Olena"));
        assert_eq!(store.upsert_count(), 0);
        assert!(recorder.events().is_empty());
    }

    #[tokio::test]
    async fn unreadable_storage_starts_the_slot_empty() {
        let (values, _store, _recorder) = rig(Store {
            list_fails: true,
            ..Store::default()
        })
        .await;

        assert_eq!(merged_donor(&values), None);
    }

    #[tokio::test]
    async fn live_only_slot_is_empty_after_restart_even_with_a_stored_row() {
        let (values, _store, _recorder) = rig(Store::holding(vec![stored_row(
            NOW_PLAYING_SLOT,
            "spotify",
            json!({ "title": "Song A" }),
        )]))
        .await;

        assert_eq!(
            now_playing_title(&values, LatestScope::MostRecentAcrossPlatforms),
            None
        );
    }
}
