use forge_types::{SubActionOutcome, SubActionSpec, SubActionTelemetry};
use std::time::Duration;
use time::OffsetDateTime;

// Requests above this threshold are silently reduced to prevent runaway delays.
const MAX_DELAY_MS: u64 = 60_000;

pub(super) async fn run(spec: &SubActionSpec, index: usize) -> SubActionTelemetry {
    let started_at = OffsetDateTime::now_utc();

    let SubActionSpec::Delay { ms } = spec else {
        unreachable!()
    };

    tokio::time::sleep(Duration::from_millis(effective_ms(*ms))).await;

    let finished_at = OffsetDateTime::now_utc();
    let duration_ms = (finished_at - started_at).whole_milliseconds().max(0) as u64;

    SubActionTelemetry {
        index,
        kind: "Delay".to_string(),
        started_at,
        duration_ms,
        outcome: SubActionOutcome::Success,
    }
}

fn effective_ms(ms: u64) -> u64 {
    ms.min(MAX_DELAY_MS)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn delay_cap_reduces_above_sixty_seconds() {
        assert_eq!(effective_ms(120_000), 60_000);
    }

    #[test]
    fn delay_below_cap_passes_through_unchanged() {
        assert_eq!(effective_ms(5_000), 5_000);
    }

    #[test]
    fn delay_at_cap_boundary_is_unchanged() {
        assert_eq!(effective_ms(60_000), 60_000);
    }

    #[tokio::test]
    async fn delay_waits_at_least_requested_duration() {
        let spec = SubActionSpec::Delay { ms: 10 };
        let before = std::time::Instant::now();
        let telemetry = run(&spec, 0).await;
        assert!(before.elapsed().as_millis() >= 10);
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert_eq!(telemetry.kind, "Delay");
    }
}
