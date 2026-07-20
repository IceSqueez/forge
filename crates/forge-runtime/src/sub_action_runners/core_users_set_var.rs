use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::{GlobalsRepo, UserGlobalsRepo};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::core_users_shared::resolve_broadcaster_id;

pub struct CoreUsersSetVarRunner {
    globals: Arc<dyn GlobalsRepo>,
    user_globals: Arc<dyn UserGlobalsRepo>,
}

impl CoreUsersSetVarRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>, user_globals: Arc<dyn UserGlobalsRepo>) -> Self {
        Self {
            globals,
            user_globals,
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreUsersSetVarRunner {
    fn id(&self) -> &str {
        "core.users.set_var"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Set User Variable"
    }

    fn summary(&self) -> &str {
        "Set a per-user variable to a value"
    }

    fn search_text(&self) -> &str {
        "set user variable per-user state store write"
    }

    fn icon_name(&self) -> &str {
        "user-cog"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "user_login".to_owned(),
            Variant::String("%user_login%".to_owned()),
        );
        cfg.insert("var_name".to_owned(), Variant::String(String::new()));
        cfg.insert("value".to_owned(), Variant::String(String::new()));
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
            FormField::Text {
                key: "value",
                label: "Value",
                placeholder: "42",
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
                "core.users.set_var: user_login and var_name are required".to_owned(),
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
        let raw_value = resolve(
            config
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
        );
        let value = super::interpolate::parse_variant(&raw_value);
        let broadcaster_id = resolve_broadcaster_id(ctx.arg_stack, self.globals.as_ref()).await;

        let outcome = match self
            .user_globals
            .set(&broadcaster_id, &user_id, &var_name, value)
            .await
        {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "core.users.set_var".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
