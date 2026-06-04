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
                reset_hint: rate_reset_hint.map(|s| format!("resets in {s:.0}s")),
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

fn age_out_errors(timestamps: &mut VecDeque<Instant>) -> usize {
    let cutoff = Instant::now() - Duration::from_secs(3600);
    while timestamps.front().is_some_and(|t| *t < cutoff) {
        timestamps.pop_front();
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
    use forge_platform_core::{BuiltinHealth, HealthValue};
    use tokio_stream::StreamExt as _;

    use super::*;
    use crate::client::DiscordClient;

    #[test]
    fn metrics_returns_four_with_correct_labels() {
        let c = DiscordClient::new_for_test();
        let h: &dyn BuiltinHealth = &*c;
        let m = h.metrics();
        assert_eq!(m.len(), 4);
        assert_eq!(m[0].label, "WEBHOOK LATENCY");
        assert_eq!(m[1].label, "RATE LIMIT");
        assert_eq!(m[2].label, "LAST SEND");
        assert_eq!(m[3].label, "ERRORS");
    }

    #[test]
    fn initial_metrics_show_no_data() {
        let c = DiscordClient::new_for_test();
        let h: &dyn BuiltinHealth = &*c;
        let m = h.metrics();
        assert!(matches!(
            m[0].value,
            HealthValue::Text { ref primary, .. } if primary == "\u{2014}"
        ));
        assert!(matches!(
            m[2].value,
            HealthValue::Status { ref label, .. } if label == "No sends yet"
        ));
    }

    #[tokio::test]
    async fn stream_is_subscribable() {
        let c = DiscordClient::new_for_test();
        let h: &dyn BuiltinHealth = &*c;
        let items: Vec<_> = h.stream().take(0).collect().await;
        assert!(items.is_empty());
    }

    #[test]
    fn update_on_send_emits_deltas() {
        let (tx, snap) = make_health_state();
        let mut rx = tx.subscribe();

        let deltas = {
            let mut s = snap.lock().unwrap();
            update_on_send(&mut s, 50, true, 4, 5, None)
        };

        assert!(!deltas.is_empty());
        assert!(deltas.iter().any(|d| d.index == 0));
        assert!(deltas.iter().any(|d| d.index == 2));

        for delta in &deltas {
            let _ = tx.send(delta.clone());
        }
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn update_on_send_failed_increments_error_count() {
        let (_tx, snap) = make_health_state();
        {
            let mut s = snap.lock().unwrap();
            update_on_send(&mut s, 100, false, 0, 5, None);
            assert_eq!(s.error_timestamps.len(), 1);
        }
    }

    #[test]
    fn p50_of_single_element() {
        let mut q = VecDeque::new();
        q.push_back(42u64);
        assert_eq!(compute_p50(&q), Some(42));
    }

    #[test]
    fn p50_of_odd_count() {
        let mut q = VecDeque::new();
        for v in [10, 30, 20, 50, 40] {
            q.push_back(v);
        }
        assert_eq!(compute_p50(&q), Some(30));
    }
}
