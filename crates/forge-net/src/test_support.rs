use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;

use crate::rate_limit::{RateLimitError, RateLimitOutcome, RateLimitUsage, RateLimiter};

pub type AcquireAnswer = Result<RateLimitOutcome, RateLimitError>;

pub struct RecordingRateLimiter {
    standing: AcquireAnswer,
    usage: RateLimitUsage,
    record: Mutex<Record>,
}

#[derive(Default)]
struct Record {
    scripted: VecDeque<AcquireAnswer>,
    acquired_weights: Vec<u32>,
    observed_throttles: Vec<Duration>,
}

impl RecordingRateLimiter {
    pub fn granting() -> Self {
        Self::answering(Ok(RateLimitOutcome::Granted))
    }

    pub fn exhausted() -> Self {
        Self::answering(Ok(RateLimitOutcome::Exhausted))
    }

    pub fn failing() -> Self {
        Self::answering(Err(RateLimitError::Exhausted))
    }

    pub fn answering(standing: AcquireAnswer) -> Self {
        Self {
            standing,
            usage: RateLimitUsage::default(),
            record: Mutex::new(Record::default()),
        }
    }

    pub fn with_script(self, answers: impl IntoIterator<Item = AcquireAnswer>) -> Self {
        self.lock().scripted.extend(answers);
        self
    }

    pub fn with_usage(mut self, usage: RateLimitUsage) -> Self {
        self.usage = usage;
        self
    }

    pub fn acquired_weights(&self) -> Vec<u32> {
        self.lock().acquired_weights.clone()
    }

    pub fn acquire_count(&self) -> usize {
        self.lock().acquired_weights.len()
    }

    pub fn observed_throttles(&self) -> Vec<Duration> {
        self.lock().observed_throttles.clone()
    }

    fn lock(&self) -> MutexGuard<'_, Record> {
        self.record
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait]
impl RateLimiter for RecordingRateLimiter {
    async fn acquire(&self, weight: u32) -> AcquireAnswer {
        let mut record = self.lock();
        record.acquired_weights.push(weight);
        record.scripted.pop_front().unwrap_or(self.standing)
    }

    async fn observe_remote_throttle(&self, retry_after: Duration) {
        self.lock().observed_throttles.push(retry_after);
    }

    fn usage(&self) -> RateLimitUsage {
        self.usage
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn scripted_answers_are_served_in_order_before_the_standing_answer() {
        let rl = RecordingRateLimiter::exhausted().with_script([
            Ok(RateLimitOutcome::Granted),
            Err(RateLimitError::Exhausted),
        ]);
        let mut answers = Vec::new();
        for _ in 0..3 {
            answers.push(rl.acquire(1).await);
        }
        assert_eq!(
            answers,
            vec![
                Ok(RateLimitOutcome::Granted),
                Err(RateLimitError::Exhausted),
                Ok(RateLimitOutcome::Exhausted),
            ]
        );
    }

    #[tokio::test]
    async fn every_acquire_weight_is_recorded_in_call_order() {
        let rl = RecordingRateLimiter::granting();
        for weight in [3, 1, 2] {
            rl.acquire(weight).await.unwrap();
        }
        assert_eq!(rl.acquired_weights(), vec![3, 1, 2]);
    }

    #[tokio::test]
    async fn every_remote_throttle_is_recorded_in_call_order() {
        let rl = RecordingRateLimiter::granting();
        rl.observe_remote_throttle(Duration::from_secs(8)).await;
        rl.observe_remote_throttle(Duration::from_secs(1)).await;
        assert_eq!(
            rl.observed_throttles(),
            vec![Duration::from_secs(8), Duration::from_secs(1)]
        );
    }
}
