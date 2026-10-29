use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TypedField,
};
use forge_storage::{GlobalsRepo, StorageError, UserGlobalsRepo};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};

use super::core_globals_shared::AMOUNT;
use super::core_users_shared::{USER_LOGIN, VAR_NAME, resolve_broadcaster_id, resolve_user_login};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&USER_LOGIN, &VAR_NAME, &AMOUNT]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String, i64), ConfigProblem> {
    Ok((
        USER_LOGIN.read(config, args)?,
        VAR_NAME.read(config, args)?,
        AMOUNT.read(config, args)?,
    ))
}

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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let (user_login_template, var_name, amount) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let user_login = match resolve_user_login(&user_login_template, ctx.arg_stack) {
            Ok(login) => login,
            Err(reason) => return (timer.failed(reason), None),
        };

        let broadcaster_id = resolve_broadcaster_id(ctx.arg_stack, self.globals.as_ref()).await;

        let outcome = match self
            .user_globals
            .incr(&broadcaster_id, &user_login, &var_name, amount)
            .await
        {
            Ok(_) => SubActionOutcome::Success,
            Err(StorageError::TypeMismatch { .. }) => {
                SubActionOutcome::Failed("existing user variable is not numeric".to_owned())
            }
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (timer.finish(outcome), None)
    }
}
