mod log;

use forge_storage::DataProvider;
use forge_types::{ArgStack, SubActionOutcome, SubActionSpec, SubActionTelemetry};

use crate::EventBus;

pub async fn dispatch(
    spec: &SubActionSpec,
    arg_stack: &ArgStack,
    index: usize,
    _bus: &EventBus,
    _dp: &dyn DataProvider,
) -> SubActionTelemetry {
    match spec {
        SubActionSpec::Log { message, .. } => {
            let interpolated = arg_stack.interpolate(message);
            log::run(spec, index, &interpolated)
        }
        SubActionSpec::SendChat { .. }
        | SubActionSpec::SetGlobal { .. }
        | SubActionSpec::Delay { .. } => SubActionTelemetry {
            index,
            kind: spec.kind_label().to_string(),
            started_at: time::OffsetDateTime::now_utc(),
            duration_ms: 0,
            outcome: SubActionOutcome::Skipped("runner not available".to_string()),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::EventBus;
    use forge_storage::DataProvider;
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{ArgStack, LogLevel, SubActionSpec, Variant};
    use std::sync::Arc;

    async fn make_dp() -> Arc<dyn DataProvider> {
        Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        )
    }

    #[tokio::test]
    async fn log_dispatch_returns_success_telemetry() {
        let dp = make_dp().await;
        let bus = EventBus::new();
        let spec = SubActionSpec::Log {
            level: LogLevel::Info,
            message: "hello".to_string(),
        };
        let telemetry = dispatch(&spec, &ArgStack::new(), 0, &bus, dp.as_ref()).await;
        assert_eq!(telemetry.kind, "Log");
        assert_eq!(telemetry.index, 0);
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
    }

    #[tokio::test]
    async fn log_dispatch_interpolates_message() {
        let dp = make_dp().await;
        let bus = EventBus::new();
        let spec = SubActionSpec::Log {
            level: LogLevel::Info,
            message: "hello %user%".to_string(),
        };
        let stack = ArgStack::new().set("user".to_string(), Variant::String("alice".to_string()));
        let telemetry = dispatch(&spec, &stack, 0, &bus, dp.as_ref()).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
    }

    #[tokio::test]
    async fn unavailable_runner_returns_skipped() {
        let dp = make_dp().await;
        let bus = EventBus::new();
        let spec = SubActionSpec::Delay { ms: 100 };
        let telemetry = dispatch(&spec, &ArgStack::new(), 1, &bus, dp.as_ref()).await;
        assert_eq!(telemetry.index, 1);
        assert!(matches!(telemetry.outcome, SubActionOutcome::Skipped(_)));
    }
}
