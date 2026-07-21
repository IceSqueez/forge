use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_storage::ActionRepo;
use forge_types::{
    ActionId, ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant,
};

pub struct CoreActionEnableRunner {
    actions: Arc<dyn ActionRepo>,
}

impl CoreActionEnableRunner {
    pub fn new(actions: Arc<dyn ActionRepo>) -> Self {
        Self { actions }
    }

    async fn run(&self, config: &SubActionConfig, ctx: &RunContext<'_>) -> SubActionOutcome {
        let raw = config.str("action_id").unwrap_or_default();
        let resolved = ctx.arg_stack.interpolate(raw);
        let Ok(action_id) = resolved.parse::<ActionId>() else {
            return SubActionOutcome::Failed(format!(
                "core.action.enable: invalid action_id '{resolved}'"
            ));
        };
        match self.actions.get(action_id).await {
            Ok(Some(mut action)) => {
                action.enabled = true;
                match self.actions.save(&action).await {
                    Ok(()) => SubActionOutcome::Success,
                    Err(e) => SubActionOutcome::Failed(format!("core.action.enable: {e}")),
                }
            }
            Ok(None) => SubActionOutcome::Failed(format!(
                "core.action.enable: unknown action '{action_id}'"
            )),
            Err(e) => SubActionOutcome::Failed(format!("core.action.enable: {e}")),
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreActionEnableRunner {
    fn id(&self) -> &str {
        "core.action.enable"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Enable Action"
    }

    fn summary(&self) -> &str {
        "Allow an action to run when triggered"
    }

    fn search_text(&self) -> &str {
        "enable action allow activate"
    }

    fn icon_name(&self) -> &str {
        "toggle-on"
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

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("action_id").map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.action.enable");
        let outcome = self.run(config, ctx).await;
        (timer.finish(outcome), None)
    }
}
