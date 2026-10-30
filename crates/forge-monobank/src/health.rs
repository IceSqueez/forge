use forge_platform_core::{
    BuiltinHealth, HealthDelta, HealthMetric, HealthMetricId, HealthStream, HealthText, HealthValue,
};
use forge_types::{MessageArg, MessageRef, VerbatimText};
use time::OffsetDateTime;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::messages;
use crate::provider::MonobankProvider;
use crate::status::{PollFailure, PollStatus};

const LAST_POLL_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_LAST_POLL);
const LAST_ERROR_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_LAST_ERROR);
const LAST_DONATION_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_LAST_DONATION);
const JAR_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_JAR);

pub(crate) fn metrics(status: &PollStatus) -> [HealthMetric; 4] {
    let jar = status.jar.as_ref();
    [
        HealthMetric {
            id: LAST_POLL_METRIC,
            value: HealthValue::Text {
                primary: time_text(status.last_poll_at),
                secondary: Some(status.phase.message().into()),
            },
        },
        HealthMetric {
            id: LAST_ERROR_METRIC,
            value: HealthValue::Status {
                label: status
                    .last_error
                    .map_or_else(
                        || MessageRef::new(messages::FAILURE_NONE),
                        PollFailure::message,
                    )
                    .into(),
                active: status.last_error.is_none(),
                detail: status.last_error.map(|_| time_text(status.last_error_at)),
            },
        },
        HealthMetric {
            id: LAST_DONATION_METRIC,
            value: HealthValue::Text {
                primary: time_text(status.last_donation_at),
                secondary: None,
            },
        },
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

pub(crate) fn deltas(status: &PollStatus) -> impl Iterator<Item = HealthDelta> {
    metrics(status).into_iter().map(|metric| HealthDelta {
        metric: metric.id,
        new_value: metric.value,
    })
}

fn time_text(instant: Option<OffsetDateTime>) -> HealthText {
    instant.map_or_else(HealthText::placeholder, |at| {
        MessageRef::new(messages::HEALTH_TIME)
            .with_arg("time", MessageArg::TimeOfDay(at))
            .into()
    })
}

impl BuiltinHealth for MonobankProvider {
    fn metrics(&self) -> [HealthMetric; 4] {
        metrics(&self.poll_status())
    }

    fn stream(&self) -> HealthStream {
        Box::pin(BroadcastStream::new(self.status.subscribe_health()).filter_map(Result::ok))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn metrics_declare_unique_catalogued_ids() {
        let status = crate::status::StatusBoard::new().snapshot();

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
