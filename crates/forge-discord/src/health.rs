use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use forge_platform_core::{BuiltinHealth, HealthDelta, HealthMetric, HealthStream, HealthValue};

use crate::client::DiscordClient;

const LATENCY_WINDOW: usize = 20;

#[derive(Default)]
pub(crate) struct DiscordHealthSnapshot {
    pub latencies_ms: VecDeque<u64>,
    pub rate_limit_remaining: u64,
    pub rate_limit_total: u64,
    pub rate_limit_reset_hint: Option<String>,
    pub last_send_ok: Option<bool>,
    pub error_timestamps: VecDeque<Instant>,
}

pub(crate) fn make_health_state() -> (
    broadcast::Sender<HealthDelta>,
    Arc<Mutex<DiscordHealthSnapshot>>,
) {
    let (tx, _) = broadcast::channel(16);
    (tx, Arc::new(Mutex::new(DiscordHealthSnapshot::default())))
}

pub(crate) fn update_on_send(
    snap: &mut DiscordHealthSnapshot,
    latency_ms: u64,
    ok: bool,
    rate_remaining: u64,
    rate_total: u64,
    rate_reset_hint: Option<String>,
) -> Vec<HealthDelta> {
    let mut deltas = Vec::new();

    snap.latencies_ms.push_back(latency_ms);
    if snap.latencies_ms.len() > LATENCY_WINDOW {
        snap.latencies_ms.pop_front();
    }

    let p50 = compute_p50(&snap.latencies_ms);
    deltas.push(HealthDelta {
        index: 0,
        new_value: HealthValue::Text {
            primary: p50
                .map(|v| format!("{v}ms"))
                .unwrap_or_else(|| "\u{2014}".to_owned()),
            secondary: Some(format!("p50 / last {}", snap.latencies_ms.len())),
        },
    });

    let prev_remaining = snap.rate_limit_remaining;
    let prev_total = snap.rate_limit_total;
    snap.rate_limit_remaining = rate_remaining;
    snap.rate_limit_total = rate_total;
    snap.rate_limit_reset_hint = rate_reset_hint.clone();
    if prev_remaining != rate_remaining || prev_total != rate_total {
        deltas.push(HealthDelta {
            index: 1,
            new_value: HealthValue::Ratio {
                used: rate_total.saturating_sub(rate_remaining),
                total: rate_total,
                reset_hint: rate_reset_hint.map(|s| format!("resets in {s}s")),
            },
        });
    }

    let prev_ok = snap.last_send_ok;
    snap.last_send_ok = Some(ok);
    if prev_ok != Some(ok) {
        let label = if ok {
            "OK".to_owned()
        } else {
            "Failed".to_owned()
        };
        deltas.push(HealthDelta {
            index: 2,
            new_value: HealthValue::Status {
                label,
                active: ok,
                detail: None,
            },
        });
    }

    if !ok {
        snap.error_timestamps.push_back(Instant::now());
    }
    let count = age_out_errors(&mut snap.error_timestamps);
    deltas.push(HealthDelta {
        index: 3,
        new_value: HealthValue::Text {
            primary: count.to_string(),
            secondary: Some("last 60 min".to_owned()),
        },
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
    /// Same four measurements `BuiltinHealth::metrics` renders, typed instead of pre-formatted.
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
    // Windows can start the monotonic clock near zero; `now - 3600s` would underflow there.
    if let Some(cutoff) = Instant::now().checked_sub(Duration::from_secs(3600)) {
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

        let p50 = compute_p50(&snap.latencies_ms);
        let latency_value = HealthValue::Text {
            primary: p50
                .map(|v| format!("{v}ms"))
                .unwrap_or_else(|| "\u{2014}".to_owned()),
            secondary: if snap.latencies_ms.is_empty() {
                None
            } else {
                Some(format!("p50 / last {}", snap.latencies_ms.len()))
            },
        };

        let budget_used = snap
            .rate_limit_total
            .saturating_sub(snap.rate_limit_remaining);
        let budget_value = HealthValue::Ratio {
            used: budget_used,
            total: snap.rate_limit_total,
            reset_hint: snap
                .rate_limit_reset_hint
                .as_deref()
                .map(|s| format!("resets in {s}")),
        };

        let last_send_value = match snap.last_send_ok {
            None => HealthValue::Status {
                label: "No sends yet".to_owned(),
                active: false,
                detail: None,
            },
            Some(true) => HealthValue::Status {
                label: "OK".to_owned(),
                active: true,
                detail: None,
            },
            Some(false) => HealthValue::Status {
                label: "Failed".to_owned(),
                active: false,
                detail: None,
            },
        };

        let error_count = age_out_errors(&mut snap.error_timestamps);
        let errors_value = HealthValue::Text {
            primary: error_count.to_string(),
            secondary: Some("last 60 min".to_owned()),
        };

        [
            HealthMetric {
                label: "WEBHOOK LATENCY".to_owned(),
                value: latency_value,
            },
            HealthMetric {
                label: "RATE LIMIT".to_owned(),
                value: budget_value,
            },
            HealthMetric {
                label: "LAST SEND".to_owned(),
                value: last_send_value,
            },
            HealthMetric {
                label: "ERRORS".to_owned(),
                value: errors_value,
            },
        ]
    }

    fn stream(&self) -> HealthStream {
        let rx = self.health_tx.subscribe();
        Box::pin(BroadcastStream::new(rx).filter_map(|r| r.ok()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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

        assert!(deltas.iter().any(|d| d.index == 0), "missing latency delta");
        assert!(
            deltas.iter().any(|d| d.index == 2),
            "missing last-send delta"
        );
    }

    #[test]
    fn update_on_send_rate_limit_delta_carries_the_reset_hint_seconds() {
        let (_tx, snap) = make_health_state();

        let deltas = update_on_send(
            &mut snap.lock().unwrap(),
            10,
            true,
            3,
            5,
            Some("12".to_owned()),
        );

        let hint = deltas
            .iter()
            .find_map(|d| match &d.new_value {
                HealthValue::Ratio { reset_hint, .. } => reset_hint.clone(),
                _ => None,
            })
            .unwrap_or_default();
        assert!(hint.contains("12"), "reset hint lost the seconds: {hint:?}");
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
