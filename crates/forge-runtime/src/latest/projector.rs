use std::sync::Arc;

use crate::bus::EventBus;
use crate::delivery::LATEST_VALUES;
use crate::latest::LatestValues;

pub fn spawn_latest_projector(bus: &Arc<EventBus>, values: LatestValues) {
    let mut subscription = bus.subscribe_critical(LATEST_VALUES);
    tokio::spawn(async move {
        while let Some(event) = subscription.recv().await {
            values.project(&event).await;
        }
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::time::Duration;

    use forge_events::{DonationReceived, Event, EventPublisher, LatestChanged};
    use forge_storage::{LatestValueRepo, MockLatestValueRepo};
    use forge_types::{
        CurrencyCode, DonorVisibility, IntegrationId, LATEST_DONATION_SLOT, LatestScope, Shared,
    };
    use time::OffsetDateTime;

    use super::*;
    use crate::bus::NullEventLogRepo;

    #[tokio::test]
    async fn bus_donation_reaches_the_slot_and_comes_back_as_a_change_event() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut repo = MockLatestValueRepo::new();
        repo.expect_list_slot().returning(|_| Ok(Vec::new()));
        repo.expect_upsert_if_newer().returning(|_| Ok(true));
        let values = LatestValues::load(
            Arc::new(repo) as Arc<dyn LatestValueRepo>,
            Arc::clone(&bus) as Arc<dyn EventPublisher>,
            Shared::new("Anonymous donor".to_owned()),
        )
        .await;
        spawn_latest_projector(&bus, values.clone());
        let mut watcher = bus.subscribe();
        let donation = DonationReceived {
            provider: IntegrationId::new("donatello"),
            donation_id: "dl-1".to_owned(),
            donor_name: "Olena".to_owned(),
            donor_visibility: DonorVisibility::Named,
            message: None,
            amount_micros: 12_500_000,
            currency: CurrencyCode::parse("UAH").unwrap(),
            occurred_at: OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap(),
            test: false,
        }
        .into_event()
        .unwrap();
        let donation_id = donation.id;

        bus.publish(donation);
        let changed: Event = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let event = watcher.recv().await.unwrap();
                if LatestChanged::from_event(&event).is_some() {
                    return event;
                }
            }
        })
        .await
        .expect("a latest.changed event");

        assert_eq!(changed.caused_by, Some(donation_id));
        assert!(
            values
                .latest(LATEST_DONATION_SLOT, LatestScope::Platform("donatello"))
                .is_some()
        );
    }
}
