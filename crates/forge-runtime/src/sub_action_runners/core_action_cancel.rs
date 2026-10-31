use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::ACTION_OPTIONS_KEY;
use forge_registry::{
    BlankableSelectField, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TypedField,
};
use forge_types::{ActionId, ArgStack, SubActionConfig, SubActionTelemetry, Variant};

use super::core_action_shared::ACTION_ID_KEY;
use crate::action_cancel::ActionCancelRegistry;
use crate::core_messages;

const ACTION: BlankableSelectField<ActionId> =
    BlankableSelectField::new(ACTION_ID_KEY, "Action", ACTION_OPTIONS_KEY);

const CONFIG: ConfigSpec = ConfigSpec::new(&[&ACTION]);

pub struct CoreActionCancelRunner {
    cancel_registry: Arc<ActionCancelRegistry>,
}

impl CoreActionCancelRunner {
    pub fn new(cancel_registry: Arc<ActionCancelRegistry>) -> Self {
        Self { cancel_registry }
    }

    fn cancel(&self, action_id: Option<ActionId>) {
        match action_id {
            Some(action_id) => {
                self.cancel_registry.cancel(action_id);
            }
            None => {
                self.cancel_registry.cancel_all();
            }
        }
    }
}

fn names_an_action(config: &SubActionConfig) -> bool {
    config
        .get(ACTION_ID_KEY)
        .and_then(Variant::as_str)
        .is_some_and(|template| !template.trim().is_empty())
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        match ACTION.read(config, ctx.arg_stack) {
            Ok(None) if names_an_action(config) => (
                timer.failed(core_messages::field_required(ACTION_ID_KEY).into()),
                None,
            ),
            Ok(action_id) => {
                self.cancel(action_id);
                (timer.success(), None)
            }
            Err(problem) => (timer.invalid_config(&problem), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_events::test_support::NullPublisher;

    use forge_registry::CancelSignal;
    use forge_types::EventId;

    use super::*;
    use forge_types::SubActionOutcome;
    use forge_types::Variant;

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
    async fn a_template_that_fills_in_blank_fails_and_cancels_nothing() {
        let registry = Arc::new(ActionCancelRegistry::new());
        let untouched = CancelSignal::new();
        registry.register(ActionId::new(), untouched.clone());
        let stack = ArgStack::new().set("target".to_owned(), Variant::String("  ".to_owned()));

        let outcome = run_with(Arc::clone(&registry), &cfg("%target%"), &stack).await;

        assert_eq!(
            crate::test_support::failure::failure_message(&outcome).code(),
            &core_messages::FIELD_REQUIRED
        );
        assert!(!untouched.is_cancelled());
    }

    #[tokio::test]
    async fn unparseable_action_id_fails_and_cancels_nothing() {
        const SENTINEL: &str = "not-a-ulid-a-viewer-typed-this";
        let registry = Arc::new(ActionCancelRegistry::new());
        let untouched = CancelSignal::new();
        registry.register(ActionId::new(), untouched.clone());

        let outcome = run_with(Arc::clone(&registry), &cfg(SENTINEL), &ArgStack::new()).await;

        let SubActionOutcome::Failed(msg) = &outcome else {
            panic!("expected a Failed outcome, got {outcome:?}");
        };
        assert_eq!(
            msg,
            &crate::test_support::failure::plain_field_problem(
                "core.action.cancel",
                "action_id",
                "config.unknown_entry"
            ),
            "the failure must name the field without echoing the value"
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
