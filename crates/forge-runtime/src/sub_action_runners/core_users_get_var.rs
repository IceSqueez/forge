use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::{GlobalsRepo, UserGlobalsRepo};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::core_users_shared::resolve_broadcaster_id;

pub struct CoreUsersGetVarRunner {
    globals: Arc<dyn GlobalsRepo>,
    user_globals: Arc<dyn UserGlobalsRepo>,
}

impl CoreUsersGetVarRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>, user_globals: Arc<dyn UserGlobalsRepo>) -> Self {
        Self {
            globals,
            user_globals,
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreUsersGetVarRunner {
    fn id(&self) -> &str {
        "core.users.get_var"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Get User Variable"
    }

    fn summary(&self) -> &str {
        "Read a per-user variable into an argument"
    }

    fn search_text(&self) -> &str {
        "get user variable per-user state read load"
    }

    fn icon_name(&self) -> &str {
        "user-search"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "user_login".to_owned(),
            Variant::String("%user_login%".to_owned()),
        );
        cfg.insert("var_name".to_owned(), Variant::String(String::new()));
        cfg.insert("into_var".to_owned(), Variant::String(String::new()));
        cfg.insert("default_value".to_owned(), Variant::String(String::new()));
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
                key: "into_var",
                label: "Output Argument",
                placeholder: "result",
            },
            FormField::Text {
                key: "default_value",
                label: "Default Value",
                placeholder: "0",
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
        let into_ok = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        if user_ok && var_ok && into_ok {
            Ok(())
        } else {
            Err(RegistryError::UnknownKindId(
                "core.users.get_var: user_login, var_name and into_var are required".to_owned(),
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
        let into_var = super::interpolate::sanitize_var_name(&resolve(
            config
                .get("into_var")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
        ));
        let default_raw = resolve(
            config
                .get("default_value")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
        );
        let broadcaster_id = resolve_broadcaster_id(ctx.arg_stack, self.globals.as_ref()).await;

        let (outcome, updated_stack) = match self
            .user_globals
            .get(&broadcaster_id, &user_id, &var_name)
            .await
        {
            Ok(Some(value)) => {
                let new_stack = ctx.arg_stack.clone().set(into_var, value);
                (SubActionOutcome::Success, Some(new_stack))
            }
            Ok(None) => {
                let fallback = super::interpolate::parse_variant(&default_raw);
                let new_stack = ctx.arg_stack.clone().set(into_var, fallback);
                (SubActionOutcome::Success, Some(new_stack))
            }
            Err(e) => (SubActionOutcome::Failed(e.to_string()), None),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.users.get_var".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            updated_stack,
        )
    }
}
