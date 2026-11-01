use std::sync::Arc;

use forge_net::TaskOwner;

use crate::bus::EventBus;
use crate::delivery::LATEST_VALUES;
use crate::latest::LatestValues;

pub fn spawn_latest_projector(bus: &Arc<EventBus>, values: LatestValues, tasks: &TaskOwner) {
    let mut subscription = bus.subscribe_critical(LATEST_VALUES);
    let stop = tasks.clone();
    tasks.spawn(async move {
        loop {
            tokio::select! {
                biased;
                () = stop.stopped() => break,
                received = subscription.recv() => match received {
                    Some(event) => values.project(&event).await,
                    None => return,
                },
            }
        }
        while let Some(event) = subscription.try_recv() {
            values.project(&event).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use forge_net::TaskShutdown;

    use forge_events::{
        DonationReceived, Event, EventPublisher, EventSource, LatestChanged, TypedEvent,
    };
    use forge_storage::{LatestValueRepo, MockLatestValueRepo};
    use forge_types::{
        CurrencyCode, DonorVisibility, IntegrationId, LATEST_DONATION_SLOT, LatestScope, Shared,
    };
    use time::OffsetDateTime;

    use super::*;
    use crate::NullEventLogRepo;

    async fn loaded_values(bus: &Arc<EventBus>) -> LatestValues {
        let mut repo = MockLatestValueRepo::new();
        repo.expect_list_slot().returning(|_| Ok(Vec::new()));
        repo.expect_upsert_if_newer().returning(|_| Ok(true));
        LatestValues::load(
            Arc::new(repo) as Arc<dyn LatestValueRepo>,
            Arc::clone(bus) as Arc<dyn EventPublisher>,
            Shared::new("Anonymous donor".to_owned()),
        )
        .await
    }

    fn donation() -> Event {
        DonationReceived {
            provider: IntegrationId::new("donatello"),
            donation_id: "dl-1".to_owned(),
            donor_name: "Olena".to_owned(),
            donor_visibility: DonorVisibility::Named,
            message: None,
            amount_micros: 12_500_000,
            currency: CurrencyCode::parse("UAH").unwrap(),
            occurred_at: OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap(),
            is_test: false,
        }
        .into_event(EventSource::Donation, None)
    }

    #[tokio::test(start_paused = true)]
    async fn stopping_projects_what_was_already_delivered_and_ends_with_the_bus_still_open() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let values = loaded_values(&bus).await;
        let tasks = TaskOwner::new();
        spawn_latest_projector(&bus, values.clone(), &tasks);
        bus.publish(donation());

        let stopped = tasks.shutdown(Duration::from_secs(1)).await;

        assert_eq!(stopped, TaskShutdown::Drained);
        assert!(
            values
                .latest(LATEST_DONATION_SLOT, LatestScope::Platform("donatello"))
                .is_some()
        );
    }

    #[tokio::test]
    async fn bus_donation_reaches_the_slot_and_comes_back_as_a_change_event() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let values = loaded_values(&bus).await;
        spawn_latest_projector(&bus, values.clone(), &TaskOwner::new());
        let mut watcher = bus.subscribe();
        let donation = donation();
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
