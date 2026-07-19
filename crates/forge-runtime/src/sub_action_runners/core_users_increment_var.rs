use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::{GlobalsRepo, UserGlobalsRepo};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::core_users_shared::resolve_broadcaster_id;

pub struct CoreUsersIncrementVarRunner {
    globals: Arc<dyn GlobalsRepo>,
    user_globals: Arc<dyn UserGlobalsRepo>,
}

impl CoreUsersIncrementVarRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>, user_globals: Arc<dyn UserGlobalsRepo>) -> Self {
        Self {
            globals,
            user_globals,
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreUsersIncrementVarRunner {
    fn id(&self) -> &str {
        "core.users.increment_var"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Increment User Variable"
    }

    fn summary(&self) -> &str {
        "Increment a numeric per-user variable by an amount"
    }

    fn search_text(&self) -> &str {
        "increment user variable per-user counter add"
    }

    fn icon_name(&self) -> &str {
        "user-plus"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "user_login".to_owned(),
            Variant::String("%user_login%".to_owned()),
        );
        cfg.insert("var_name".to_owned(), Variant::String(String::new()));
        cfg.insert("amount".to_owned(), Variant::Int(1));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "user_login",
                label: "User Login",
                placeholder: "%user_login%",
            },
            FormField::Text {
                key: "var_name",
                label: "Variable Name",
                placeholder: "points",
            },
            FormField::Integer {
                key: "amount",
                label: "Amount",
                min: i64::MIN,
                max: i64::MAX,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let user_ok = config
            .get("user_login")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        let var_ok = config
            .get("var_name")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        if user_ok && var_ok {
            Ok(())
        } else {
            Err(RegistryError::UnknownKindId(
                "core.users.increment_var: user_login and var_name are required".to_owned(),
            ))
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let resolve = |template: &str| ctx.arg_stack.interpolate(template);

        let user_id = resolve(
            config
                .get("user_login")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
        );
        let var_name = resolve(
            config
                .get("var_name")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
        );
        let amount = config.get("amount").and_then(|v| v.as_int()).unwrap_or(1);
        let broadcaster_id = resolve_broadcaster_id(ctx.arg_stack, self.globals.as_ref()).await;

        let outcome = match self
            .user_globals
            .get(&broadcaster_id, &user_id, &var_name)
            .await
        {
            Ok(current) => match increment(current, amount) {
                Some(next) => match self
                    .user_globals
                    .set(&broadcaster_id, &user_id, &var_name, next)
                    .await
                {
                    Ok(()) => SubActionOutcome::Success,
                    Err(e) => SubActionOutcome::Failed(e.to_string()),
                },
                None => {
                    SubActionOutcome::Failed("existing user variable is not numeric".to_owned())
                }
            },
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.users.increment_var".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

/// A missing variable starts at zero; a non-numeric existing value yields `None` (Failed).
fn increment(current: Option<Variant>, amount: i64) -> Option<Variant> {
    match current {
        None => Some(Variant::Int(amount)),
        Some(Variant::Int(i)) => Some(Variant::Int(i.saturating_add(amount))),
        Some(Variant::Float(f)) => Variant::float(f + amount as f64).ok(),
        Some(_) => None,
    }
}
