use forge_platform_core::{
    BuiltinHealth, HealthDelta, HealthMetric, HealthMetricId, HealthStream, HealthText, HealthValue,
};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use time::OffsetDateTime;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::messages;
use crate::provider::MonobankProvider;
use crate::status::PollStatus;

const LOCAL_TIME_FORMAT: &str = "%H:%M:%S";
const NOT_YET: &str = "-";

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
            id: JAR_METRIC,
            value: HealthValue::Text {
                primary: HealthText::unlocalized(
                    jar.and_then(|jar| jar.title.clone())
                        .unwrap_or_else(|| NOT_YET.to_owned()),
                ),
                secondary: jar
                    .and_then(|jar| jar.currency.as_ref())
                    .map(ToString::to_string)
                    .map(HealthText::unlocalized),
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
