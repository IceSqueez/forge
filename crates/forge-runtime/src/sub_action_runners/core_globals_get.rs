use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, ProducedVariable, RunContext, StepTimer, SubActionCategory,
    SubActionIo, SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_storage::GlobalsRepo;
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
    strip_var_decoration,
};

use super::core_globals_shared::{GLOBAL_BY_NAME, global_name};
use crate::core_messages;

const INTO_ARG_KEY: &str = "into_arg";

const INTO_ARG: TextField = TextField {
    key: INTO_ARG_KEY,
    label: "Output Variable",
    editor: TextEditor::Line {
        placeholder: "result",
    },
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&GLOBAL_BY_NAME, &INTO_ARG]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String), ConfigProblem> {
    Ok((
        global_name(&GLOBAL_BY_NAME, config, args)?,
        strip_var_decoration(&INTO_ARG.read(config, args)?),
    ))
}

pub struct CoreGlobalsGetRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsGetRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsGetRunner {
    fn id(&self) -> &str {
        "core.globals.get"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Get Global"
    }

    fn summary(&self) -> &str {
        "Read a global variable into an argument"
    }

    fn search_text(&self) -> &str {
        "get global variable read load"
    }

    fn icon_name(&self) -> &str {
        "database-import"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_ARG_KEY.to_owned(),
                kind: VariantKind::String,
                label: "Global value".to_owned(),
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
        let (name, into) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let (outcome, updated_stack) = match self.globals.get(&name).await {
            Ok(value) => {
                let variant = value.unwrap_or(Variant::String(String::new()));
                let new_stack = ctx.arg_stack.clone().set(into, variant);
                (SubActionOutcome::Success, Some(new_stack))
            }
            Err(e) => (
                SubActionOutcome::Failed(core_messages::storage_failed(e).into()),
                None,
            ),
        };

        (timer.finish(outcome), updated_stack)
    }
}
