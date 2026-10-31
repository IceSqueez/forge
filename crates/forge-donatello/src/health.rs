use forge_platform_core::{HealthMetric, HealthMetricId, HealthText, HealthValue, poll_metrics};

use crate::messages;
use crate::status::{PollFailure, PollStatus};

const POLL_INTERVAL_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_POLL_INTERVAL);

pub(crate) fn metrics(status: &PollStatus) -> [HealthMetric; 4] {
    let [last_poll, last_error, last_donation] =
        poll_metrics(status, &messages::POLL_VOCABULARY, PollFailure::message);
    [
        last_poll,
        last_error,
        last_donation,
        HealthMetric {
            id: POLL_INTERVAL_METRIC,
            value: HealthValue::Text {
                primary: HealthText::Span(status.poll_interval),
                secondary: None,
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::time::Duration;

    use super::*;

    #[test]
    fn metrics_declare_unique_catalogued_ids() {
        let status = crate::status::PollStatus::initial(Duration::from_secs(30));

        let ids: HashSet<_> = metrics(&status)
            .into_iter()
            .map(|metric| metric.id)
            .collect();

        assert_eq!(ids.len(), 4);
        for id in &ids {
            assert!(
                crate::messages::DONATELLO_MESSAGES
                    .codes
                    .contains(id.label().code()),
                "{id} is not catalogued"
            );
        }
    }
}
