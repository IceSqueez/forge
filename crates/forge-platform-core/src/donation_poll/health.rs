use forge_types::{MessageArg, MessageCode, MessageRef};
use time::OffsetDateTime;

use super::status::{PollPhaseCodes, PollStatus};
use crate::builtin::{HealthMetric, HealthMetricId, HealthText, HealthValue};

#[derive(Debug, Clone)]
pub struct PollVocabulary {
    pub health_last_poll: MessageCode,
    pub health_last_error: MessageCode,
    pub health_last_donation: MessageCode,
    pub health_time: MessageCode,
    pub failure_none: MessageCode,
    pub phases: PollPhaseCodes,
}

pub fn poll_metrics<S, F: Copy>(
    status: &PollStatus<S, F>,
    vocabulary: &PollVocabulary,
    failure_message: impl Fn(F) -> MessageRef,
) -> [HealthMetric; 3] {
    let time_text = |instant: Option<OffsetDateTime>| {
        instant.map_or_else(HealthText::placeholder, |at| {
            MessageRef::new(vocabulary.health_time.clone())
                .with_arg("time", MessageArg::TimeOfDay(at))
                .into()
        })
    };
    [
        HealthMetric {
            id: HealthMetricId::new(vocabulary.health_last_poll.clone()),
            value: HealthValue::Text {
                primary: time_text(status.last_poll_at),
                secondary: Some(status.phase.message(&vocabulary.phases).into()),
            },
        },
        HealthMetric {
            id: HealthMetricId::new(vocabulary.health_last_error.clone()),
            value: HealthValue::Status {
                label: status
                    .last_error
                    .map_or_else(
                        || MessageRef::new(vocabulary.failure_none.clone()),
                        failure_message,
                    )
                    .into(),
                active: status.last_error.is_none(),
                detail: status.last_error.map(|_| time_text(status.last_error_at)),
            },
        },
        HealthMetric {
            id: HealthMetricId::new(vocabulary.health_last_donation.clone()),
            value: HealthValue::Text {
                primary: time_text(status.last_donation_at),
                secondary: None,
            },
        },
    ]
}
