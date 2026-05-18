use forge_storage::{DataProvider, GlobalsRepo};
use forge_types::{ArgStack, SubActionOutcome, SubActionSpec, SubActionTelemetry, Variant};
use time::OffsetDateTime;

pub(super) async fn run(
    spec: &SubActionSpec,
    arg_stack: &ArgStack,
    index: usize,
    dp: &dyn DataProvider,
) -> (SubActionTelemetry, Option<ArgStack>) {
    let started_at = OffsetDateTime::now_utc();

    let SubActionSpec::GetGlobal { name, into_arg } = spec else {
        unreachable!()
    };

    let resolved_name = super::interpolate_with_globals(name, arg_stack, dp).await;
    let resolved_into = super::interpolate_with_globals(into_arg, arg_stack, dp).await;

    let (outcome, updated_stack) = match GlobalsRepo::get(dp, &resolved_name).await {
        Ok(value) => {
            let variant = value.unwrap_or(Variant::String(String::new()));
            let new_stack = arg_stack.clone().set(resolved_into, variant);
            (SubActionOutcome::Success, Some(new_stack))
        }
        Err(e) => (SubActionOutcome::Failed(e.to_string()), None),
    };

    let finished_at = OffsetDateTime::now_utc();
    let duration_ms = (finished_at - started_at).whole_milliseconds().max(0) as u64;

    let telemetry = SubActionTelemetry {
        index,
        kind: "GetGlobal".to_string(),
        started_at,
        duration_ms,
        outcome,
    };

    (telemetry, updated_stack)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_storage::GlobalsRepo;
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{ArgStack, SubActionSpec, Variant};
    use std::sync::Arc;

    async fn make_dp() -> Arc<dyn DataProvider> {
        Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        )
    }

    #[tokio::test]
    async fn get_global_stores_value_in_arg_stack() {
        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "counter", Variant::Int(7), false)
            .await
            .unwrap();

        let spec = SubActionSpec::GetGlobal {
            name: "counter".to_string(),
            into_arg: "x".to_string(),
        };
        let (telemetry, updated) = run(&spec, &ArgStack::new(), 0, dp.as_ref()).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        let new_stack = updated.unwrap();
        assert_eq!(new_stack.get("x"), Some(&Variant::Int(7)));
    }

    #[tokio::test]
    async fn get_global_missing_key_stores_empty_string() {
        let dp = make_dp().await;

        let spec = SubActionSpec::GetGlobal {
            name: "nonexistent".to_string(),
            into_arg: "result".to_string(),
        };
        let (telemetry, updated) = run(&spec, &ArgStack::new(), 0, dp.as_ref()).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        let new_stack = updated.unwrap();
        assert_eq!(
            new_stack.get("result"),
            Some(&Variant::String(String::new()))
        );
    }

    #[tokio::test]
    async fn get_global_interpolates_name_from_arg_stack() {
        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "my_counter", Variant::Int(42), false)
            .await
            .unwrap();

        let spec = SubActionSpec::GetGlobal {
            name: "%prefix%_counter".to_string(),
            into_arg: "val".to_string(),
        };
        let stack = ArgStack::new().set("prefix".to_string(), Variant::String("my".to_string()));
        let (telemetry, updated) = run(&spec, &stack, 0, dp.as_ref()).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert_eq!(updated.unwrap().get("val"), Some(&Variant::Int(42)));
    }

    #[tokio::test]
    async fn get_global_returns_correct_kind_and_index() {
        let dp = make_dp().await;
        let spec = SubActionSpec::GetGlobal {
            name: "x".to_string(),
            into_arg: "out".to_string(),
        };
        let (telemetry, _) = run(&spec, &ArgStack::new(), 3, dp.as_ref()).await;
        assert_eq!(telemetry.kind, "GetGlobal");
        assert_eq!(telemetry.index, 3);
    }
}
