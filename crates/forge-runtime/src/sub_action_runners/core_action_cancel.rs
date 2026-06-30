use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{
    ActionId, ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant,
};
use time::OffsetDateTime;

use crate::action_cancel::ActionCancelRegistry;

pub struct CoreActionCancelRunner {
    cancel_registry: Arc<ActionCancelRegistry>,
}

impl CoreActionCancelRunner {
    pub fn new(cancel_registry: Arc<ActionCancelRegistry>) -> Self {
        Self { cancel_registry }
    }

    fn run(&self, config: &SubActionConfig, ctx: &RunContext<'_>) -> SubActionOutcome {
        let raw = config
            .get("action_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let resolved = ctx.arg_stack.interpolate(raw);
        let resolved = resolved.trim();

        if resolved.is_empty() {
            self.cancel_registry.cancel_all();
            return SubActionOutcome::Success;
        }

        let Ok(action_id) = resolved.parse::<ActionId>() else {
            return SubActionOutcome::Failed(format!(
                "core.action.cancel: invalid action_id '{resolved}'"
            ));
        };
        self.cancel_registry.cancel(action_id);
        SubActionOutcome::Success
    }
}

#[async_trait]
impl SubActionRunner for CoreActionCancelRunner {
    fn id(&self) -> &str {
        "core.action.cancel"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Cancel Action"
    }

    fn summary(&self) -> &str {
        "Stop in-flight runs of an action (or all running actions)"
    }

    fn search_text(&self) -> &str {
        "cancel action stop abort kill running"
    }

    fn icon_name(&self) -> &str {
        "octagon-x"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("action_id".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "action_id",
            label: "Action",
            options_key: "action.ids",
        }]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let outcome = self.run(config, ctx);
        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;
        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.action.cancel".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use forge_events::{Event, EventPublisher};
    use forge_registry::CancelSignal;
    use forge_types::EventId;

    use super::*;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    fn cfg(action_id: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert(
            "action_id".to_owned(),
            Variant::String(action_id.to_owned()),
        );
        c
    }

    async fn run_with(
        registry: Arc<ActionCancelRegistry>,
        config: &SubActionConfig,
        arg_stack: &ArgStack,
    ) -> SubActionOutcome {
        let runner = CoreActionCancelRunner::new(registry);
        let ctx = RunContext::leaf(arg_stack, 0, EventId::new(), &NullPublisher);
        let (telemetry, _) = runner.execute(config, &ctx).await;
        telemetry.outcome
    }

    #[tokio::test]
    async fn valid_action_id_cancels_only_that_actions_signal_and_succeeds() {
        let registry = Arc::new(ActionCancelRegistry::new());
        let target = ActionId::new();
        let bystander = ActionId::new();
        let target_sig = CancelSignal::new();
        let bystander_sig = CancelSignal::new();
        registry.register(target, target_sig.clone());
        registry.register(bystander, bystander_sig.clone());

        let outcome = run_with(
            Arc::clone(&registry),
            &cfg(&target.to_string()),
            &ArgStack::new(),
        )
        .await;

        assert!(matches!(outcome, SubActionOutcome::Success));
        assert!(target_sig.is_cancelled());
        assert!(!bystander_sig.is_cancelled());
    }

    #[tokio::test]
    async fn empty_action_id_cancels_every_registered_signal() {
        let registry = Arc::new(ActionCancelRegistry::new());
        let one = CancelSignal::new();
        let two = CancelSignal::new();
        registry.register(ActionId::new(), one.clone());
        registry.register(ActionId::new(), two.clone());

        let outcome = run_with(Arc::clone(&registry), &cfg(""), &ArgStack::new()).await;

        assert!(matches!(outcome, SubActionOutcome::Success));
        assert!(one.is_cancelled());
        assert!(two.is_cancelled());
    }

    #[tokio::test]
    async fn unparseable_action_id_fails_and_cancels_nothing() {
        let registry = Arc::new(ActionCancelRegistry::new());
        let untouched = CancelSignal::new();
        registry.register(ActionId::new(), untouched.clone());

        let outcome = run_with(Arc::clone(&registry), &cfg("not-a-ulid"), &ArgStack::new()).await;

        assert!(
            matches!(&outcome, SubActionOutcome::Failed(m) if m.contains("invalid action_id")),
            "expected a Failed carrying the bad id, got {outcome:?}"
        );
        assert!(
            !untouched.is_cancelled(),
            "a parse failure must bail before cancelling anything"
        );
    }

    #[tokio::test]
    async fn known_format_action_id_with_nothing_running_succeeds_idempotently() {
        let registry = Arc::new(ActionCancelRegistry::new());
        let outcome = run_with(
            registry,
            &cfg(&ActionId::new().to_string()),
            &ArgStack::new(),
        )
        .await;
        assert!(matches!(outcome, SubActionOutcome::Success));
    }

    #[tokio::test]
    async fn action_id_is_interpolated_and_trimmed_before_cancelling() {
        let registry = Arc::new(ActionCancelRegistry::new());
        let target = ActionId::new();
        let target_sig = CancelSignal::new();
        registry.register(target, target_sig.clone());

        let stack = ArgStack::new().set("target".to_owned(), Variant::String(target.to_string()));
        let outcome = run_with(Arc::clone(&registry), &cfg("  %target%  "), &stack).await;

        assert!(matches!(outcome, SubActionOutcome::Success));
        assert!(
            target_sig.is_cancelled(),
            "the %target% placeholder must resolve from the arg stack before cancelling"
        );
    }
}
