use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::ActionRepo;
use forge_types::{
    ActionId, ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant,
};
use time::OffsetDateTime;

pub struct CoreActionDisableRunner {
    actions: Arc<dyn ActionRepo>,
}

impl CoreActionDisableRunner {
    pub fn new(actions: Arc<dyn ActionRepo>) -> Self {
        Self { actions }
    }

    async fn run(&self, config: &SubActionConfig, ctx: &RunContext<'_>) -> SubActionOutcome {
        let raw = config
            .get("action_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let resolved = ctx.arg_stack.interpolate(raw);
        let Ok(action_id) = resolved.parse::<ActionId>() else {
            return SubActionOutcome::Failed(format!(
                "core.action.disable: invalid action_id '{resolved}'"
            ));
        };
        match self.actions.get(action_id).await {
            Ok(Some(mut action)) => {
                action.enabled = false;
                match self.actions.save(&action).await {
                    Ok(()) => SubActionOutcome::Success,
                    Err(e) => SubActionOutcome::Failed(format!("core.action.disable: {e}")),
                }
            }
            Ok(None) => SubActionOutcome::Failed(format!(
                "core.action.disable: unknown action '{action_id}'"
            )),
            Err(e) => SubActionOutcome::Failed(format!("core.action.disable: {e}")),
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreActionDisableRunner {
    fn id(&self) -> &str {
        "core.action.disable"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Disable Action"
    }

    fn summary(&self) -> &str {
        "Prevent an action from running when triggered"
    }

    fn search_text(&self) -> &str {
        "disable action block deactivate stop"
    }

    fn icon_name(&self) -> &str {
        "toggle-off"
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
        match config.get("action_id").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.action.disable: action_id is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let outcome = self.run(config, ctx).await;
        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;
        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.action.disable".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
