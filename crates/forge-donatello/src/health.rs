use forge_platform_core::{
    BuiltinHealth, HealthDelta, HealthMetric, HealthMetricId, HealthStream, HealthText, HealthValue,
};
use forge_types::{MessageArg, MessageRef};
use time::OffsetDateTime;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::messages;
use crate::provider::DonatelloProvider;
use crate::status::{PollFailure, PollStatus};

const LAST_POLL_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_LAST_POLL);
const LAST_ERROR_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_LAST_ERROR);
const LAST_DONATION_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_LAST_DONATION);
const POLL_INTERVAL_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_POLL_INTERVAL);

pub(crate) fn metrics(status: &PollStatus) -> [HealthMetric; 4] {
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
            id: POLL_INTERVAL_METRIC,
            value: HealthValue::Text {
                primary: HealthText::Span(status.poll_interval),
                secondary: None,
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

impl BuiltinHealth for DonatelloProvider {
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
    use std::time::Duration;

    use super::*;

    #[test]
    fn metrics_declare_unique_catalogued_ids() {
        let status = crate::status::StatusBoard::new(Duration::from_secs(30)).snapshot();

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
