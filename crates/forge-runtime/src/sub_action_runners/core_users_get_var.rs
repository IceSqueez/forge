use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, ProducedVariable, RunContext, StepTimer, SubActionCategory,
    SubActionIo, SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_storage::{GlobalsRepo, UserGlobalsRepo};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, VariantKind,
    strip_var_decoration,
};

use super::core_users_shared::{USER_LOGIN, VAR_NAME, resolve_broadcaster_id, resolve_user_login};

const INTO_VAR_KEY: &str = "into_var";

const INTO_VAR: TextField = TextField {
    key: INTO_VAR_KEY,
    label: "Output Variable",
    editor: TextEditor::Line {
        placeholder: "result",
    },
    default: "",
    required: true,
};

const DEFAULT_VALUE: TextField = TextField {
    key: "default_value",
    label: "Default Value",
    editor: TextEditor::Line { placeholder: "0" },
    default: "",
    required: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&USER_LOGIN, &VAR_NAME, &INTO_VAR, &DEFAULT_VALUE]);

struct GetVarConfig {
    user_login_template: String,
    var_name: String,
    into_var: String,
    default_raw: String,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<GetVarConfig, ConfigProblem> {
    Ok(GetVarConfig {
        user_login_template: USER_LOGIN.read(config, args)?,
        var_name: VAR_NAME.read(config, args)?,
        into_var: strip_var_decoration(&INTO_VAR.read(config, args)?),
        default_raw: DEFAULT_VALUE.read(config, args)?,
    })
}

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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::String,
                label: "User variable value".to_owned(),
            }],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let GetVarConfig {
            user_login_template,
            var_name,
            into_var,
            default_raw,
        } = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let user_login = match resolve_user_login(&user_login_template, ctx.arg_stack) {
            Ok(login) => login,
            Err(reason) => return (timer.failed(reason), None),
        };

        let broadcaster_id = resolve_broadcaster_id(ctx.arg_stack, self.globals.as_ref()).await;

        let (outcome, updated_stack) = match self
            .user_globals
            .get(&broadcaster_id, &user_login, &var_name)
            .await
        {
            Ok(Some(value)) => {
                let new_stack = ctx.arg_stack.clone().set(into_var, value);
                (SubActionOutcome::Success, Some(new_stack))
            }
            Ok(None) => {
                let fallback = super::variant_text::parse_variant(&default_raw);
                let new_stack = ctx.arg_stack.clone().set(into_var, fallback);
                (SubActionOutcome::Success, Some(new_stack))
            }
            Err(e) => (SubActionOutcome::Failed(e.to_string()), None),
        };

        (timer.finish(outcome), updated_stack)
    }
}
