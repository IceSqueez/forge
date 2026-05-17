use forge_types::{LogLevel, SubActionOutcome, SubActionSpec, SubActionTelemetry};
use time::OffsetDateTime;
use tracing::{debug, error, info, trace, warn};

pub(super) fn run(
    spec: &SubActionSpec,
    index: usize,
    interpolated_message: &str,
) -> SubActionTelemetry {
    let started_at = OffsetDateTime::now_utc();

    let level = match spec {
        SubActionSpec::Log { level, .. } => level,
        _ => unreachable!(),
    };

    match level {
        LogLevel::Trace => trace!(target: "forge::action", message = interpolated_message),
        LogLevel::Debug => debug!(target: "forge::action", message = interpolated_message),
        LogLevel::Info => info!(target: "forge::action", message = interpolated_message),
        LogLevel::Warn => warn!(target: "forge::action", message = interpolated_message),
        LogLevel::Error => error!(target: "forge::action", message = interpolated_message),
    }

    let finished_at = OffsetDateTime::now_utc();
    let duration_ms = (finished_at - started_at).whole_milliseconds().max(0) as u64;

    SubActionTelemetry {
        kind: "Log".to_string(),
        started_at,
        duration_ms,
        outcome: SubActionOutcome::Success,
        index,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn log_runner_returns_success_telemetry() {
        let spec = SubActionSpec::Log {
            level: LogLevel::Info,
            message: "hello world".to_string(),
        };
        let telemetry = run(&spec, 0, "hello world");
        assert_eq!(telemetry.kind, "Log");
        assert_eq!(telemetry.index, 0);
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
    }

    #[test]
    fn log_runner_records_correct_step_index() {
        let spec = SubActionSpec::Log {
            level: LogLevel::Debug,
            message: "step 3".to_string(),
        };
        let telemetry = run(&spec, 3, "step 3");
        assert_eq!(telemetry.index, 3);
    }

    #[test]
    fn log_all_levels_produce_success() {
        for level in [
            LogLevel::Trace,
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warn,
            LogLevel::Error,
        ] {
            let spec = SubActionSpec::Log {
                level,
                message: "test".to_string(),
            };
            let t = run(&spec, 0, "test");
            assert!(matches!(t.outcome, SubActionOutcome::Success));
        }
    }
}
