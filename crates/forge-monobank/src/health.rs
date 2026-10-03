use forge_platform_core::{BuiltinHealth, HealthDelta, HealthMetric, HealthStream, HealthValue};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use time::OffsetDateTime;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::provider::MonobankProvider;
use crate::status::PollStatus;

const LOCAL_TIME_FORMAT: &str = "%H:%M:%S";
const NOT_YET: &str = "-";

pub(crate) fn metrics(status: &PollStatus) -> [HealthMetric; 4] {
    let jar = status.jar.as_ref();
    [
        HealthMetric {
            label: "LAST POLL".to_owned(),
            value: HealthValue::Text {
                primary: local_time(status.last_poll_at),
                secondary: Some(status.phase_label().to_owned()),
            },
        },
        HealthMetric {
            label: "LAST ERROR".to_owned(),
            value: HealthValue::Status {
                label: status
                    .last_error
                    .map_or("None", |failure| failure.label())
                    .to_owned(),
                active: status.last_error.is_none(),
                detail: status.last_error.map(|_| local_time(status.last_error_at)),
            },
        },
        HealthMetric {
            label: "LAST DONATION".to_owned(),
            value: HealthValue::Text {
                primary: local_time(status.last_donation_at),
                secondary: None,
            },
        },
        HealthMetric {
            label: "JAR".to_owned(),
            value: HealthValue::Text {
                primary: jar
                    .and_then(|jar| jar.title.clone())
                    .unwrap_or_else(|| NOT_YET.to_owned()),
                secondary: jar
                    .and_then(|jar| jar.currency.as_ref())
                    .map(ToString::to_string),
            },
        },
    ]
}

pub(crate) fn deltas(status: &PollStatus) -> impl Iterator<Item = HealthDelta> {
    metrics(status)
        .into_iter()
        .zip(0u8..)
        .map(|(metric, index)| HealthDelta {
            index,
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
