use std::sync::Arc;

use forge_events::{DonationReceived, EventPublisher};
use forge_platform_core::DonationStream;
use forge_storage::DonationRepo;
use forge_types::{Donation, DonationOrigin, EventId, IntegrationId, Shared};
use futures_util::StreamExt;
use time::{Duration, OffsetDateTime};
use tracing::{debug, warn};

pub const DONATION_CATCH_UP: Duration = Duration::minutes(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Announce,
    RecordOnly,
}

pub struct DonationIngest {
    ledger: Arc<dyn DonationRepo>,
    publisher: Arc<dyn EventPublisher>,
    anonymous_donor: Shared<String>,
}

impl DonationIngest {
    pub fn new(
        ledger: Arc<dyn DonationRepo>,
        publisher: Arc<dyn EventPublisher>,
        anonymous_donor: Shared<String>,
    ) -> Self {
        Self {
            ledger,
            publisher,
            anonymous_donor,
        }
    }

    pub async fn recover_unannounced(&self) {
        let now = OffsetDateTime::now_utc();
        let horizon = now.saturating_sub(DONATION_CATCH_UP);
        match self.ledger.list_unannounced_occurred_since(horizon).await {
            Ok(pending) => {
                for stored in pending {
                    self.announce_recorded(&stored.donation, now).await;
                }
            }
            Err(error) => warn!(error = %error, "unannounced donations could not be read"),
        }
        if let Err(error) = self
            .ledger
            .mark_announced_all_occurred_before(horizon, now)
            .await
        {
            warn!(error = %error, "stale donations could not be marked announced");
        }
    }

    pub async fn drain(&self, provider: IntegrationId, mut stream: DonationStream) {
        let mut baseline = match self.ledger.has_donations_from(&provider).await {
            Ok(seen_before) => !seen_before,
            Err(error) => {
                warn!(provider = %provider, error = %error, "donation ledger unreadable; earlier donations are recorded without alerts");
                true
            }
        };
        while let Some(item) = stream.next().await {
            match item {
                Ok(donation) => self.ingest(donation, &mut baseline).await,
                Err(error) => {
                    debug!(provider = %provider, error = %error, "donation provider attempt failed");
                }
            }
        }
    }

    pub fn announce_test(&self, donation: &Donation, caused_by: Option<EventId>) -> bool {
        self.publish(donation, caused_by)
    }

    async fn ingest(&self, donation: Donation, baseline: &mut bool) {
        if donation.origin == DonationOrigin::Test {
            self.publish(&donation, None);
            return;
        }
        if donation.origin == DonationOrigin::Live {
            *baseline = false;
        }
        let now = OffsetDateTime::now_utc();
        match self.ledger.insert_if_new(&donation, now).await {
            Ok(true) => {}
            Ok(false) => return,
            Err(error) => {
                warn!(provider = %donation.provider, error = %error, "donation could not be recorded");
                return;
            }
        }
        match verdict(&donation, *baseline, now) {
            Verdict::Announce => self.announce_recorded(&donation, now).await,
            Verdict::RecordOnly => self.mark_announced(&donation, now).await,
        }
    }

    async fn announce_recorded(&self, donation: &Donation, now: OffsetDateTime) {
        let alert = Donation {
            origin: DonationOrigin::Live,
            ..donation.clone()
        };
        if self.publish(&alert, None) {
            self.mark_announced(donation, now).await;
        }
    }

    async fn mark_announced(&self, donation: &Donation, now: OffsetDateTime) {
        if let Err(error) = self
            .ledger
            .mark_announced(&donation.provider, &donation.donation_id, now)
            .await
        {
            warn!(provider = %donation.provider, error = %error, "donation could not be marked announced");
        }
    }

    fn publish(&self, donation: &Donation, caused_by: Option<EventId>) -> bool {
        let anonymous = self.anonymous_donor.load();
        let Some(payload) = DonationReceived::announce(donation, anonymous.as_str()) else {
            return false;
        };
        match payload.into_event() {
            Ok(mut event) => {
                event.caused_by = caused_by;
                self.publisher.publish(event);
                true
            }
            Err(error) => {
                warn!(provider = %donation.provider, error = %error, "donation event could not be built");
                false
            }
        }
    }
}

fn verdict(donation: &Donation, baseline: bool, now: OffsetDateTime) -> Verdict {
    let baseline_history = baseline && donation.origin == DonationOrigin::History;
    let within_catch_up = donation.occurred_at >= now.saturating_sub(DONATION_CATCH_UP);
    if within_catch_up && !baseline_history {
        Verdict::Announce
    } else {
        Verdict::RecordOnly
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_events::Event;
    use forge_platform_core::PlatformError;
    use forge_storage::{StorageError, StoredDonation};
    use forge_types::{CurrencyCode, Donor, MoneyAmount};

    use super::*;

    const PLACEHOLDER: &str = "Anonymous donor";

    #[derive(Default)]
    struct Ledger {
        rows: Mutex<Vec<StoredDonation>>,
        unreadable: bool,
        rejects_inserts: bool,
    }

    impl Ledger {
        fn holding(rows: Vec<StoredDonation>) -> Self {
            Self {
                rows: Mutex::new(rows),
                ..Self::default()
            }
        }

        fn announced(&self, donation_id: &str) -> Option<bool> {
            self.rows
                .lock()
                .unwrap()
                .iter()
                .find(|row| row.donation.donation_id == donation_id)
                .map(|row| row.announced_at.is_some())
        }

        fn ids(&self) -> Vec<String> {
            let rows = self.rows.lock().unwrap();
            rows.iter()
                .map(|row| row.donation.donation_id.clone())
                .collect()
        }
    }

    fn failure() -> StorageError {
        StorageError::Connection {
            reason: "disk gone".to_owned(),
        }
    }

    #[async_trait]
    impl DonationRepo for Ledger {
        async fn insert_if_new(
            &self,
            donation: &Donation,
            received_at: OffsetDateTime,
        ) -> Result<bool, StorageError> {
            if self.rejects_inserts {
                return Err(failure());
            }
            let mut rows = self.rows.lock().unwrap();
            if rows.iter().any(|row| {
                row.donation.provider == donation.provider
                    && row.donation.donation_id == donation.donation_id
            }) {
                return Ok(false);
            }
            rows.push(StoredDonation {
                donation: donation.clone(),
                received_at,
                announced_at: None,
            });
            Ok(true)
        }

        async fn mark_announced(
            &self,
            provider: &IntegrationId,
            donation_id: &str,
            announced_at: OffsetDateTime,
        ) -> Result<bool, StorageError> {
            let mut rows = self.rows.lock().unwrap();
            let row = rows.iter_mut().find(|row| {
                &row.donation.provider == provider && row.donation.donation_id == donation_id
            });
            Ok(row
                .map(|row| row.announced_at = Some(announced_at))
                .is_some())
        }

        async fn list_recent(&self, _limit: usize) -> Result<Vec<StoredDonation>, StorageError> {
            Ok(self.rows.lock().unwrap().clone())
        }

        async fn has_donations_from(&self, provider: &IntegrationId) -> Result<bool, StorageError> {
            if self.unreadable {
                return Err(failure());
            }
            let rows = self.rows.lock().unwrap();
            Ok(rows.iter().any(|row| &row.donation.provider == provider))
        }

        async fn list_unannounced_occurred_since(
            &self,
            not_before: OffsetDateTime,
        ) -> Result<Vec<StoredDonation>, StorageError> {
            let rows = self.rows.lock().unwrap();
            Ok(rows
                .iter()
                .filter(|row| row.announced_at.is_none() && row.donation.occurred_at >= not_before)
                .cloned()
                .collect())
        }

        async fn mark_announced_all_occurred_before(
            &self,
            not_before: OffsetDateTime,
            announced_at: OffsetDateTime,
        ) -> Result<u64, StorageError> {
            let mut rows = self.rows.lock().unwrap();
            let mut marked = 0;
            for row in rows
                .iter_mut()
                .filter(|row| row.announced_at.is_none() && row.donation.occurred_at < not_before)
            {
                row.announced_at = Some(announced_at);
                marked += 1;
            }
            Ok(marked)
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
        fn announced(&self) -> Vec<DonationReceived> {
            let events = self.0.lock().unwrap();
            events
                .iter()
                .map(|event| DonationReceived::from_event(event).unwrap())
                .collect()
        }

        fn announced_ids(&self) -> Vec<String> {
            self.announced()
                .into_iter()
                .map(|received| received.donation_id)
                .collect()
        }
    }

    fn provider() -> IntegrationId {
        IntegrationId::new("donatello")
    }

    fn donation(id: &str, origin: DonationOrigin, age: Duration) -> Donation {
        Donation {
            provider: provider(),
            donation_id: id.to_owned(),
            donor: Donor::named("Olena"),
            message: None,
            amount: MoneyAmount::from_micros(50_000_000, CurrencyCode::parse("UAH").unwrap()),
            occurred_at: OffsetDateTime::now_utc() - age,
            origin,
        }
    }

    fn fresh(id: &str, origin: DonationOrigin) -> Donation {
        donation(id, origin, Duration::minutes(1))
    }

    fn stored(donation: Donation) -> StoredDonation {
        StoredDonation {
            received_at: donation.occurred_at,
            donation,
            announced_at: None,
        }
    }

    fn already_announced(donation: Donation) -> StoredDonation {
        StoredDonation {
            announced_at: Some(donation.occurred_at),
            ..stored(donation)
        }
    }

    struct Rig {
        ledger: Arc<Ledger>,
        recorder: Arc<Recorder>,
        anonymous: Shared<String>,
        ingest: DonationIngest,
    }

    fn rig(ledger: Ledger) -> Rig {
        let ledger = Arc::new(ledger);
        let recorder = Arc::new(Recorder::default());
        let anonymous = Shared::new(PLACEHOLDER.to_owned());
        let ingest = DonationIngest::new(
            Arc::clone(&ledger) as Arc<dyn DonationRepo>,
            Arc::clone(&recorder) as Arc<dyn EventPublisher>,
            anonymous.clone(),
        );
        Rig {
            ledger,
            recorder,
            anonymous,
            ingest,
        }
    }

    fn seen_provider() -> Ledger {
        Ledger::holding(vec![already_announced(donation(
            "earlier",
            DonationOrigin::Live,
            Duration::days(3),
        ))])
    }

    fn stream_of(items: Vec<Result<Donation, PlatformError>>) -> DonationStream {
        Box::pin(futures_util::stream::iter(items))
    }

    async fn drain(rig: &Rig, donations: Vec<Donation>) {
        rig.ingest
            .drain(
                provider(),
                stream_of(donations.into_iter().map(Ok).collect()),
            )
            .await;
    }

    #[test]
    fn the_catch_up_window_includes_its_boundary_and_excludes_one_millisecond_past_it() {
        let now = OffsetDateTime::now_utc();
        for (age, expected) in [
            (DONATION_CATCH_UP, Verdict::Announce),
            (
                DONATION_CATCH_UP + Duration::milliseconds(1),
                Verdict::RecordOnly,
            ),
        ] {
            let late = Donation {
                occurred_at: now - age,
                ..fresh("late", DonationOrigin::Live)
            };
            assert_eq!(verdict(&late, false, now), expected, "age {age}");
        }
    }

    #[tokio::test]
    async fn history_from_a_provider_with_no_ledger_rows_is_recorded_without_an_alert() {
        let rig = rig(Ledger::default());

        drain(&rig, vec![fresh("h1", DonationOrigin::History)]).await;

        assert_eq!(
            (rig.recorder.announced_ids(), rig.ledger.announced("h1")),
            (Vec::<String>::new(), Some(true))
        );
    }

    #[tokio::test]
    async fn the_first_live_donation_lifts_the_baseline_for_later_history() {
        let rig = rig(Ledger::default());

        drain(
            &rig,
            vec![
                fresh("h1", DonationOrigin::History),
                fresh("l1", DonationOrigin::Live),
                fresh("h2", DonationOrigin::History),
            ],
        )
        .await;

        assert_eq!(rig.recorder.announced_ids(), ["l1", "h2"]);
    }

    #[tokio::test]
    async fn an_unreadable_ledger_treats_history_as_a_baseline() {
        let rig = rig(Ledger {
            unreadable: true,
            ..seen_provider()
        });

        drain(&rig, vec![fresh("h1", DonationOrigin::History)]).await;

        assert!(rig.recorder.announced_ids().is_empty());
    }

    #[tokio::test]
    async fn fresh_history_from_a_known_provider_is_announced_as_a_real_donation() {
        let rig = rig(seen_provider());

        drain(&rig, vec![fresh("h1", DonationOrigin::History)]).await;

        let announced = rig.recorder.announced();
        assert_eq!(
            announced
                .iter()
                .map(|received| (received.donation_id.as_str(), received.test))
                .collect::<Vec<_>>(),
            [("h1", false)]
        );
    }

    #[tokio::test]
    async fn an_announced_donation_is_marked_in_the_ledger() {
        let rig = rig(seen_provider());

        drain(&rig, vec![fresh("l1", DonationOrigin::Live)]).await;

        assert_eq!(rig.ledger.announced("l1"), Some(true));
    }

    #[tokio::test]
    async fn a_donation_already_in_the_ledger_is_neither_announced_nor_marked() {
        let rig = rig(Ledger::holding(vec![stored(fresh(
            "l1",
            DonationOrigin::Live,
        ))]));

        drain(&rig, vec![fresh("l1", DonationOrigin::Live)]).await;

        assert_eq!(
            (rig.recorder.announced_ids(), rig.ledger.announced("l1")),
            (Vec::<String>::new(), Some(false))
        );
    }

    #[tokio::test]
    async fn a_donation_the_ledger_cannot_record_is_not_announced() {
        let rig = rig(Ledger {
            rejects_inserts: true,
            ..seen_provider()
        });

        drain(&rig, vec![fresh("l1", DonationOrigin::Live)]).await;

        assert!(rig.recorder.announced_ids().is_empty());
    }

    #[tokio::test]
    async fn a_test_donation_is_announced_as_a_test_and_never_recorded() {
        let rig = rig(seen_provider());

        drain(&rig, vec![fresh("t1", DonationOrigin::Test)]).await;

        let flags: Vec<bool> = rig
            .recorder
            .announced()
            .iter()
            .map(|received| received.test)
            .collect();
        assert_eq!(
            (flags, rig.ledger.ids()),
            (vec![true], vec!["earlier".to_owned()])
        );
    }

    #[tokio::test]
    async fn a_failed_provider_attempt_does_not_end_the_drain() {
        let rig = rig(seen_provider());

        rig.ingest
            .drain(
                provider(),
                stream_of(vec![
                    Err(PlatformError::Network {
                        reason: "timeout".to_owned(),
                    }),
                    Ok(fresh("l1", DonationOrigin::Live)),
                ]),
            )
            .await;

        assert_eq!(rig.recorder.announced_ids(), ["l1"]);
    }

    #[tokio::test]
    async fn an_anonymous_donor_is_announced_under_the_current_placeholder() {
        let rig = rig(seen_provider());
        rig.anonymous.store("Анонім".to_owned());

        drain(
            &rig,
            vec![Donation {
                donor: Donor::Anonymous,
                ..fresh("l1", DonationOrigin::Live)
            }],
        )
        .await;

        let names: Vec<String> = rig
            .recorder
            .announced()
            .into_iter()
            .map(|received| received.donor_name)
            .collect();
        assert_eq!(names, ["Анонім"]);
    }

    fn recovery_ledger() -> Ledger {
        Ledger::holding(vec![
            stored(donation(
                "fresh",
                DonationOrigin::History,
                Duration::minutes(5),
            )),
            stored(donation(
                "stale",
                DonationOrigin::History,
                Duration::hours(2),
            )),
            already_announced(donation("done", DonationOrigin::Live, Duration::minutes(3))),
        ])
    }

    #[tokio::test]
    async fn recovery_announces_only_unannounced_donations_inside_the_catch_up_window() {
        let rig = rig(recovery_ledger());

        rig.ingest.recover_unannounced().await;

        let announced: Vec<(String, bool)> = rig
            .recorder
            .announced()
            .into_iter()
            .map(|received| (received.donation_id, received.test))
            .collect();
        assert_eq!(announced, [("fresh".to_owned(), false)]);
    }

    #[tokio::test]
    async fn recovery_leaves_nothing_unannounced_for_the_next_boot() {
        let rig = rig(recovery_ledger());

        rig.ingest.recover_unannounced().await;

        assert_eq!(
            ["fresh", "stale", "done"].map(|id| rig.ledger.announced(id)),
            [Some(true); 3]
        );
    }
}
