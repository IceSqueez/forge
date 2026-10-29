use async_trait::async_trait;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigSpec, ControlSignal, RunContext, STOP_KIND_ID,
    StepTimer, StopMark, SubActionCategory, SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry};

#[derive(Clone, Copy, PartialEq, Eq)]
enum StopOutcome {
    Completed,
    Failed,
}

impl ConfigChoice for StopOutcome {
    const ALL: &'static [Self] = &[Self::Completed, Self::Failed];
    const WIRE_NAMES: &'static [&'static str] = &["completed", "failed"];

    fn wire_name(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

const MARK_AS: ChoiceField<StopOutcome> = ChoiceField {
    key: "mark_as",
    label: "Mark As",
    default: StopOutcome::Completed,
};

const REASON: TextField = TextField {
    key: "reason",
    label: "Reason",
    editor: TextEditor::Line {
        placeholder: "why the chain stopped",
    },
    default: "",
    required: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&MARK_AS, &REASON]);

fn read_mark(config: &SubActionConfig, args: &ArgStack) -> Result<StopMark, ConfigProblem> {
    let failed = MARK_AS.read(config, args)? == StopOutcome::Failed;
    let reason = Some(REASON.read(config, args)?).filter(|text| !text.is_empty());
    Ok(StopMark { failed, reason })
}

pub struct CoreLogicStopRunner;

#[async_trait]
impl SubActionRunner for CoreLogicStopRunner {
    fn id(&self) -> &str {
        STOP_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Stop Action"
    }

    fn summary(&self) -> &str {
        "Halt the current action chain immediately"
    }

    fn search_text(&self) -> &str {
        "stop halt end action chain abort flow control"
    }

    fn icon_name(&self) -> &str {
        "stop-circle"
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
        match read_mark(config, ctx.arg_stack) {
            Ok(mark) => {
                ctx.control.set(ControlSignal::Stop(mark));
                (timer.success(), None)
            }
            Err(problem) => (timer.invalid_config(&problem), None),
        }
    }
}
