use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ChainSignal, FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner,
};
use forge_storage::ActionRepo;
use forge_types::{
    ActionId, ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant,
};
use time::OffsetDateTime;

pub struct CoreActionRunRunner {
    actions: Arc<dyn ActionRepo>,
}

impl CoreActionRunRunner {
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
                "core.action.run: invalid action_id '{resolved}'"
            ));
        };

        let target = match self.actions.get(action_id).await {
            Ok(Some(a)) => a,
            Ok(None) => {
                return SubActionOutcome::Failed(format!(
                    "core.action.run: unknown action '{action_id}'"
                ));
            }
            Err(e) => return SubActionOutcome::Failed(format!("core.action.run: {e}")),
        };

        let inherit_args = config
            .get("inherit_args")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let child_stack = if inherit_args {
            ctx.arg_stack.clone()
        } else {
            ArgStack::new()
        };

        let outcome = ctx
            .executor
            .run_child_chain(&target.sub_actions, &child_stack, ctx.parent_event_id)
            .await;

        match outcome {
            Err(RegistryError::DepthExceeded(_)) => {
                SubActionOutcome::Failed("core.action.run: max nesting depth exceeded".to_owned())
            }
            Err(e) => SubActionOutcome::Failed(format!("core.action.run: {e}")),
            // Stop/Break/Continue are absorbed as Success here: the parent's
            // action-root maps all three to a successful execution, and no child
            // chain can emit them yet (flow-control runners that do are a later
            // tier and will revisit cross-action propagation).
            Ok(child) => match child.signal {
                ChainSignal::Completed
                | ChainSignal::Stop
                | ChainSignal::Break
                | ChainSignal::Continue => SubActionOutcome::Success,
                ChainSignal::Error(msg) => SubActionOutcome::Failed(msg),
                // The shared cancel is already tripped, so the parent's
                // action-root force-maps the whole execution to Cancelled; a
                // failed step halts the rest of the parent chain in the meantime.
                ChainSignal::Aborted => {
                    SubActionOutcome::Failed("core.action.run: cancelled".to_owned())
                }
            },
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreActionRunRunner {
    fn id(&self) -> &str {
        "core.action.run"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Run Action"
    }

    fn summary(&self) -> &str {
        "Run another action's sub-actions from inside this chain"
    }

    fn search_text(&self) -> &str {
        "run action call execute chain nested"
    }

    fn icon_name(&self) -> &str {
        "play"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("action_id".to_owned(), Variant::String(String::new()));
        cfg.insert("inherit_args".to_owned(), Variant::Bool(true));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: "action_id",
                label: "Action",
                options_key: "action.ids",
            },
            FormField::Toggle {
                key: "inherit_args",
                label: "Pass current arguments",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("action_id").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.action.run: action_id is required".to_owned(),
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
                kind: "core.action.run".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
