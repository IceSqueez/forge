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
