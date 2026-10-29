use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, IntegerEditor, IntegerField, Interpolation,
    ProducedVariable, RunContext, StepTimer, SubActionCategory, SubActionIo, SubActionRunner,
    TypedField, ValueBounds, VariableField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};
use rand::RngExt;

const TARGET_VAR_KEY: &str = "target_var";

const MIN: IntegerField = IntegerField {
    key: "min",
    label: "Minimum",
    editor: IntegerEditor::Text { placeholder: "1" },
    default: 1,
    bounds: ValueBounds::UNBOUNDED,
};

const MAX: IntegerField = IntegerField {
    key: "max",
    label: "Maximum",
    editor: IntegerEditor::Text { placeholder: "100" },
    default: 100,
    bounds: ValueBounds::UNBOUNDED,
};

const TARGET_VAR: VariableField = VariableField {
    key: TARGET_VAR_KEY,
    label: "Output Variable",
    placeholder: "random_result",
    default: "",
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&MIN, &MAX, &TARGET_VAR]);

fn ordered(min: i64, max: i64) -> Result<(), ConfigProblem> {
    if min <= max {
        Ok(())
    } else {
        Err(ConfigProblem::field(
            MAX.key,
            ConfigReason::OutOfRange(ValueBounds::at_least(min)),
        ))
    }
}

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(i64, i64, String), ConfigProblem> {
    let min = MIN.read(config, args)?;
    let max = MAX.read(config, args)?;
    ordered(min, max)?;
    Ok((min, max, TARGET_VAR.read(config, args)?))
}

pub struct CoreRandomIntRunner;

#[async_trait]
impl SubActionRunner for CoreRandomIntRunner {
    fn id(&self) -> &str {
        "core.random.int"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Random Integer"
    }

    fn summary(&self) -> &str {
        "Generate a random integer in [min, max] and store it in a variable"
    }

    fn search_text(&self) -> &str {
        "random integer number generate range"
    }

    fn icon_name(&self) -> &str {
        "dice"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        match (
            MIN.peek(config, interpolation)?,
            MAX.peek(config, interpolation)?,
        ) {
            (Some(min), Some(max)) => ordered(min, max),
            _ => Ok(()),
        }
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: TARGET_VAR_KEY.to_owned(),
                kind: VariantKind::Int,
                label: "Random integer".to_owned(),
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
        let (min, max, target_var) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let value = rand::rng().random_range(min..=max);
        let stack = ctx.arg_stack.clone().set(target_var, Variant::Int(value));

        (timer.success(), Some(stack))
    }
}
