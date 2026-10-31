use forge_platform_core::{HealthMetric, HealthMetricId, HealthText, HealthValue, poll_metrics};
use forge_types::VerbatimText;

use crate::messages;
use crate::status::{PollFailure, PollStatus};

const JAR_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_JAR);

pub(crate) fn metrics(status: &PollStatus) -> [HealthMetric; 4] {
    let jar = status.subject.as_ref();
    let [last_poll, last_error, last_donation] =
        poll_metrics(status, &messages::POLL_VOCABULARY, PollFailure::message);
    [
        last_poll,
        last_error,
        last_donation,
        HealthMetric {
            id: JAR_METRIC,
            value: HealthValue::Text {
                primary: jar
                    .and_then(|jar| jar.title.clone())
                    .map_or_else(HealthText::placeholder, |title| {
                        VerbatimText::new(title).into()
                    }),
                secondary: jar
                    .and_then(|jar| jar.currency.as_ref())
                    .map(|currency| VerbatimText::new(currency.to_string()).into()),
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn metrics_declare_unique_catalogued_ids() {
        let status = crate::status::PollStatus::initial(std::time::Duration::ZERO);

        let ids: HashSet<_> = metrics(&status)
            .into_iter()
            .map(|metric| metric.id)
            .collect();

        assert_eq!(ids.len(), 4);
        for id in &ids {
            assert!(
                crate::messages::MONOBANK_MESSAGES
                    .codes
                    .contains(id.label().code()),
                "{id} is not catalogued"
            );
        }
    }
}
