use forge_platform_core::{
    BuiltinHealth, HealthDelta, HealthMetric, HealthMetricId, HealthStream, HealthText, HealthValue,
};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use time::OffsetDateTime;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::messages;
use crate::provider::DonatelloProvider;
use crate::status::PollStatus;

const LOCAL_TIME_FORMAT: &str = "%H:%M:%S";
const NOT_YET: &str = "-";

const LAST_POLL_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_LAST_POLL);
const LAST_ERROR_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_LAST_ERROR);
const LAST_DONATION_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_LAST_DONATION);
const POLL_INTERVAL_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_POLL_INTERVAL);

pub(crate) fn metrics(status: &PollStatus) -> [HealthMetric; 4] {
    [
        HealthMetric {
            id: LAST_POLL_METRIC,
            value: HealthValue::Text {
                primary: HealthText::unlocalized(local_time(status.last_poll_at)),
                secondary: Some(HealthText::unlocalized(status.phase_label().to_owned())),
            },
        },
        HealthMetric {
            id: LAST_ERROR_METRIC,
            value: HealthValue::Status {
                label: HealthText::unlocalized(
                    status
                        .last_error
                        .map_or("None", |failure| failure.label())
                        .to_owned(),
                ),
                active: status.last_error.is_none(),
                detail: status
                    .last_error
                    .map(|_| local_time(status.last_error_at))
                    .map(HealthText::unlocalized),
            },
        },
        HealthMetric {
            id: LAST_DONATION_METRIC,
            value: HealthValue::Text {
                primary: HealthText::unlocalized(local_time(status.last_donation_at)),
                secondary: None,
            },
        },
        HealthMetric {
            id: POLL_INTERVAL_METRIC,
            value: HealthValue::Text {
                primary: HealthText::unlocalized(format!("{}s", status.poll_interval.as_secs())),
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

fn local_time(instant: Option<OffsetDateTime>) -> String {
    instant
        .and_then(|instant| Timestamp::from_second(instant.unix_timestamp()).ok())
        .map_or_else(
            || NOT_YET.to_owned(),
            |timestamp| {
                timestamp
                    .to_zoned(TimeZone::system())
                    .strftime(LOCAL_TIME_FORMAT)
                    .to_string()
            },
        )
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
    fn metrics_declare_unique_ids() {
        let status = crate::status::StatusBoard::new(Duration::from_secs(30)).snapshot();

        let ids: HashSet<_> = metrics(&status)
            .into_iter()
            .map(|metric| metric.id)
            .collect();

        assert_eq!(ids.len(), 4);
    }
}
