use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use forge_platform_core::{
    BuiltinHealth, HealthDelta, HealthMetric, HealthMetricId, HealthStream, HealthText, HealthValue,
};

use crate::client::DiscordClient;
use crate::vocabulary::messages;

const LATENCY_WINDOW: usize = 20;
const ERROR_WINDOW: Duration = Duration::from_secs(3600);

pub(crate) const LATENCY_METRIC: HealthMetricId =
    HealthMetricId::new(messages::HEALTH_WEBHOOK_LATENCY);
pub(crate) const RATE_LIMIT_METRIC: HealthMetricId =
    HealthMetricId::new(messages::HEALTH_RATE_LIMIT);
pub(crate) const LAST_SEND_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_LAST_SEND);
pub(crate) const ERRORS_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_ERRORS);

#[derive(Default)]
pub(crate) struct DiscordHealthSnapshot {
    pub latencies_ms: VecDeque<u64>,
    pub rate_limit_remaining: u64,
    pub rate_limit_total: u64,
    pub rate_limit_reset_hint: Option<Duration>,
    pub last_send_ok: Option<bool>,
    pub error_timestamps: VecDeque<Instant>,
}

pub(crate) fn make_health_state() -> (
    broadcast::Sender<HealthDelta>,
    Arc<Mutex<DiscordHealthSnapshot>>,
) {
    let (tx, _) = broadcast::channel(forge_platform_core::HEALTH_CHANNEL_CAPACITY);
    (tx, Arc::new(Mutex::new(DiscordHealthSnapshot::default())))
}

pub(crate) fn update_on_send(
    snap: &mut DiscordHealthSnapshot,
    latency_ms: u64,
    ok: bool,
    rate_remaining: u64,
    rate_total: u64,
    rate_reset_hint: Option<Duration>,
) -> Vec<HealthDelta> {
    let mut deltas = Vec::new();

    snap.latencies_ms.push_back(latency_ms);
    if snap.latencies_ms.len() > LATENCY_WINDOW {
        snap.latencies_ms.pop_front();
    }

    deltas.push(HealthDelta {
        metric: LATENCY_METRIC,
        new_value: latency_value(&snap.latencies_ms),
    });

    let prev_remaining = snap.rate_limit_remaining;
    let prev_total = snap.rate_limit_total;
    snap.rate_limit_remaining = rate_remaining;
    snap.rate_limit_total = rate_total;
    snap.rate_limit_reset_hint = rate_reset_hint;
    if prev_remaining != rate_remaining || prev_total != rate_total {
        deltas.push(HealthDelta {
            metric: RATE_LIMIT_METRIC,
            new_value: budget_value(rate_remaining, rate_total, rate_reset_hint),
        });
    }

    let prev_ok = snap.last_send_ok;
    snap.last_send_ok = Some(ok);
    if prev_ok != Some(ok) {
        deltas.push(HealthDelta {
            metric: LAST_SEND_METRIC,
            new_value: last_send_value(Some(ok)),
        });
    }

    if !ok {
        snap.error_timestamps.push_back(Instant::now());
    }
    let count = age_out_errors(&mut snap.error_timestamps);
    deltas.push(HealthDelta {
        metric: ERRORS_METRIC,
        new_value: errors_value(count),
    });

    deltas
}

fn latency_value(latencies_ms: &VecDeque<u64>) -> HealthValue {
    match compute_p50(latencies_ms) {
        Some(p50) => HealthValue::Text {
            primary: messages::health_latency_millis(p50).into(),
            secondary: Some(messages::health_latency_samples(latencies_ms.len()).into()),
        },
        None => HealthValue::Text {
            primary: HealthText::placeholder(),
            secondary: None,
        },
    }
}

fn budget_value(remaining: u64, total: u64, reset_hint: Option<Duration>) -> HealthValue {
    HealthValue::Ratio {
        used: total.saturating_sub(remaining),
        total,
        reset_hint: reset_hint.map(|span| messages::health_reset_in(span).into()),
    }
}

fn last_send_value(last_send_ok: Option<bool>) -> HealthValue {
    HealthValue::Status {
        label: messages::health_send(last_send_ok).into(),
        active: last_send_ok == Some(true),
        detail: None,
    }
}

fn errors_value(count: usize) -> HealthValue {
    HealthValue::Text {
        primary: HealthText::Count(count as u64),
        secondary: Some(messages::health_errors_window().into()),
    }
}

pub(crate) fn record_missing_webhook(snap: &mut DiscordHealthSnapshot) -> Vec<HealthDelta> {
    let mut deltas = Vec::new();

    let prev_ok = snap.last_send_ok;
    snap.last_send_ok = Some(false);
    if prev_ok != Some(false) {
        deltas.push(HealthDelta {
            metric: LAST_SEND_METRIC,
            new_value: last_send_value(Some(false)),
        });
    }

    snap.error_timestamps.push_back(Instant::now());
    let count = age_out_errors(&mut snap.error_timestamps);
    deltas.push(HealthDelta {
        metric: ERRORS_METRIC,
        new_value: errors_value(count),
    });

    deltas
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DiscordSendHealth {
    pub latency_p50_ms: Option<u64>,
    pub latency_samples: usize,
    pub rate_limit_used: u64,
    pub rate_limit_total: u64,
    pub last_send_ok: Option<bool>,
    pub errors_last_hour: usize,
}

impl DiscordClient {
    pub fn send_health(&self) -> DiscordSendHealth {
        let mut snap = self.health_state.lock().unwrap_or_else(|p| p.into_inner());
        DiscordSendHealth {
            latency_p50_ms: compute_p50(&snap.latencies_ms),
            latency_samples: snap.latencies_ms.len(),
            rate_limit_used: snap
                .rate_limit_total
                .saturating_sub(snap.rate_limit_remaining),
            rate_limit_total: snap.rate_limit_total,
            last_send_ok: snap.last_send_ok,
            errors_last_hour: age_out_errors(&mut snap.error_timestamps),
        }
    }
}

fn age_out_errors(timestamps: &mut VecDeque<Instant>) -> usize {
    if let Some(cutoff) = Instant::now().checked_sub(ERROR_WINDOW) {
        while timestamps.front().is_some_and(|t| *t < cutoff) {
            timestamps.pop_front();
        }
    }
    timestamps.len()
}

fn compute_p50(latencies: &VecDeque<u64>) -> Option<u64> {
    if latencies.is_empty() {
        return None;
    }
    let mut sorted: Vec<u64> = latencies.iter().copied().collect();
    sorted.sort_unstable();
    Some(sorted[sorted.len() / 2])
}

impl BuiltinHealth for DiscordClient {
    fn metrics(&self) -> [HealthMetric; 4] {
        let mut snap = self.health_state.lock().unwrap_or_else(|p| p.into_inner());

        let latency = latency_value(&snap.latencies_ms);
        let budget = budget_value(
            snap.rate_limit_remaining,
            snap.rate_limit_total,
            snap.rate_limit_reset_hint,
        );
        let last_send = last_send_value(snap.last_send_ok);
        let errors = errors_value(age_out_errors(&mut snap.error_timestamps));

        [
            HealthMetric {
                id: LATENCY_METRIC,
                value: latency,
            },
            HealthMetric {
                id: RATE_LIMIT_METRIC,
                value: budget,
            },
            HealthMetric {
                id: LAST_SEND_METRIC,
                value: last_send,
            },
            HealthMetric {
                id: ERRORS_METRIC,
                value: errors,
            },
        ]
    }

    fn stream(&self) -> HealthStream {
        let rx = self.health_tx.subscribe();
        Box::pin(BroadcastStream::new(rx).filter_map(|r| r.ok()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::DiscordClient;
    use crate::client::tests::MockCreds;

    fn record(client: &DiscordClient, latency_ms: u64, ok: bool, remaining: u64, total: u64) {
        let mut snap = client.health_state.lock().unwrap();
        update_on_send(&mut snap, latency_ms, ok, remaining, total, None);
    }

    #[test]
    fn update_on_send_reports_latency_and_last_result_deltas() {
        let (_tx, snap) = make_health_state();

        let deltas = update_on_send(&mut snap.lock().unwrap(), 50, true, 4, 5, None);

        assert!(
            deltas.iter().any(|d| d.metric == LATENCY_METRIC),
            "missing latency delta"
        );
        assert!(
            deltas.iter().any(|d| d.metric == LAST_SEND_METRIC),
            "missing last-send delta"
        );
    }

    #[test]
    fn metrics_declare_unique_catalogued_ids() {
        let client = DiscordClient::new_for_test_with_creds(MockCreds::new().creds());

        let ids: std::collections::HashSet<_> = client
            .metrics()
            .into_iter()
            .map(|metric| metric.id)
            .collect();

        assert_eq!(ids.len(), 4);
        for id in &ids {
            assert!(
                crate::vocabulary::messages::DISCORD_MESSAGES
                    .codes
                    .contains(id.label().code()),
                "{id} is not catalogued"
            );
        }
    }

    #[test]
    fn every_emitted_delta_targets_a_declared_metric() {
        let client = DiscordClient::new_for_test_with_creds(MockCreds::new().creds());
        let declared: Vec<_> = client
            .metrics()
            .into_iter()
            .map(|metric| metric.id)
            .collect();
        let mut snap = DiscordHealthSnapshot::default();

        let mut deltas = update_on_send(&mut snap, 10, true, 4, 5, None);
        deltas.extend(update_on_send(
            &mut snap,
            20,
            false,
            3,
            5,
            Some(Duration::from_secs(9)),
        ));
        deltas.extend(record_missing_webhook(&mut DiscordHealthSnapshot::default()));

        for delta in &deltas {
            assert!(
                declared.contains(&delta.metric),
                "undeclared {}",
                delta.metric
            );
        }
    }

    fn value_of(deltas: &[HealthDelta], metric: HealthMetricId) -> HealthValue {
        deltas
            .iter()
            .find(|delta| delta.metric == metric)
            .unwrap_or_else(|| panic!("no delta for {metric}"))
            .new_value
            .clone()
    }

    #[test]
    fn update_on_send_rate_limit_delta_carries_the_reset_hint_as_a_span() {
        let (_tx, snap) = make_health_state();
        let span = Duration::from_secs(12);

        let deltas = update_on_send(&mut snap.lock().unwrap(), 10, true, 3, 5, Some(span));

        assert_eq!(
            value_of(&deltas, RATE_LIMIT_METRIC),
            HealthValue::Ratio {
                used: 2,
                total: 5,
                reset_hint: Some(messages::health_reset_in(span).into()),
            }
        );
    }

    #[test]
    fn the_latency_tile_reads_a_placeholder_until_the_first_send_then_the_p50_of_its_samples() {
        let mut samples = VecDeque::new();
        assert_eq!(
            latency_value(&samples),
            HealthValue::Text {
                primary: HealthText::placeholder(),
                secondary: None,
            }
        );

        samples.extend([10, 30, 20]);
        assert_eq!(
            latency_value(&samples),
            HealthValue::Text {
                primary: messages::health_latency_millis(20).into(),
                secondary: Some(messages::health_latency_samples(3).into()),
            }
        );
    }

    #[test]
    fn the_last_send_tile_lights_only_after_a_successful_send() {
        for (last_send_ok, active) in [(None, false), (Some(false), false), (Some(true), true)] {
            assert_eq!(
                last_send_value(last_send_ok),
                HealthValue::Status {
                    label: messages::health_send(last_send_ok).into(),
                    active,
                    detail: None,
                },
                "{last_send_ok:?}"
            );
        }
    }

    #[test]
    fn a_missing_webhook_fails_the_last_send_tile_and_counts_as_an_error() {
        let deltas = record_missing_webhook(&mut DiscordHealthSnapshot::default());

        assert_eq!(
            value_of(&deltas, LAST_SEND_METRIC),
            last_send_value(Some(false))
        );
        assert_eq!(value_of(&deltas, ERRORS_METRIC), errors_value(1));
    }

    #[test]
    fn the_errors_tile_is_a_count_over_the_last_hour() {
        assert_eq!(
            errors_value(4),
            HealthValue::Text {
                primary: HealthText::Count(4),
                secondary: Some(messages::health_errors_window().into()),
            }
        );
    }

    #[test]
    fn update_on_send_records_a_timestamp_only_for_a_failed_send() {
        let (_tx, snap) = make_health_state();
        let mut snap = snap.lock().unwrap();

        update_on_send(&mut snap, 10, true, 4, 5, None);
        assert!(snap.error_timestamps.is_empty());

        update_on_send(&mut snap, 100, false, 3, 5, None);
        assert_eq!(snap.error_timestamps.len(), 1);
    }

    #[test]
    fn compute_p50_returns_the_upper_median_of_the_sample_window() {
        for (samples, expected) in [
            (vec![], None),
            (vec![42], Some(42)),
            (vec![10, 30, 20, 50, 40], Some(30)),
            (vec![40, 10, 30, 20], Some(30)),
        ] {
            let queue: VecDeque<u64> = samples.iter().copied().collect();
            assert_eq!(compute_p50(&queue), expected, "samples {samples:?}");
        }
    }

    #[test]
    fn send_health_before_any_send_reports_no_samples() {
        let client = DiscordClient::new_for_test_with_creds(MockCreds::new().creds());

        let health = client.send_health();

        assert_eq!(health.latency_p50_ms, None);
        assert_eq!(health.latency_samples, 0);
        assert_eq!(health.last_send_ok, None);
        assert_eq!(health.errors_last_hour, 0);
    }

    #[test]
    fn send_health_projects_latency_budget_and_last_result_of_recorded_sends() {
        let client = DiscordClient::new_for_test_with_creds(MockCreds::new().creds());
        record(&client, 10, true, 4, 5);
        record(&client, 90, true, 3, 5);
        record(&client, 50, false, 2, 5);

        let health = client.send_health();

        assert_eq!(health.latency_p50_ms, Some(50));
        assert_eq!(health.latency_samples, 3);
        assert_eq!(health.rate_limit_used, 3);
        assert_eq!(health.rate_limit_total, 5);
        assert_eq!(health.last_send_ok, Some(false));
        assert_eq!(health.errors_last_hour, 1);
    }

    #[test]
    fn send_health_rate_limit_used_stays_zero_when_remaining_exceeds_total() {
        let client = DiscordClient::new_for_test_with_creds(MockCreds::new().creds());
        record(&client, 10, true, 9, 5);

        assert_eq!(client.send_health().rate_limit_used, 0);
    }
}
