use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{RunContext, StepCatalogEntry, StepTimer, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry, Variant};

use crate::error::VTubeError;
use crate::runners::step_failure;
use crate::sink::VTubeSink;
use crate::vocabulary::{sub_action_ids, variables};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LookupKind {
    CurrentModel,
    Hotkeys,
    Expressions,
    Parameters,
    Items,
}

type Published = Vec<(&'static str, Variant)>;

fn count_of(len: usize) -> Variant {
    Variant::Int(i64::try_from(len).unwrap_or(i64::MAX))
}

fn strings(values: Vec<String>) -> Variant {
    Variant::Array(values.into_iter().map(Variant::String).collect())
}

fn bools(values: Vec<bool>) -> Variant {
    Variant::Array(values.into_iter().map(Variant::Bool).collect())
}

impl LookupKind {
    pub(crate) const ALL: [Self; 5] = [
        Self::CurrentModel,
        Self::Hotkeys,
        Self::Expressions,
        Self::Parameters,
        Self::Items,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::CurrentModel => sub_action_ids::LOOKUP_CURRENT_MODEL,
            Self::Hotkeys => sub_action_ids::LOOKUP_HOTKEYS,
            Self::Expressions => sub_action_ids::LOOKUP_EXPRESSIONS,
            Self::Parameters => sub_action_ids::LOOKUP_PARAMETERS,
            Self::Items => sub_action_ids::LOOKUP_ITEMS,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::CurrentModel => "Get Current Model",
            Self::Hotkeys => "Get Hotkeys",
            Self::Expressions => "Get Expressions",
            Self::Parameters => "Get Parameters",
            Self::Items => "Get Items",
        }
    }

    fn summary(self) -> &'static str {
        match self {
            Self::CurrentModel => "Queries VTube Studio for the currently loaded model.",
            Self::Hotkeys => "Queries VTube Studio for the hotkeys in the current model.",
            Self::Expressions => "Queries VTube Studio for the expressions in the current model.",
            Self::Parameters => {
                "Queries VTube Studio for the input and Live2D parameters of the current model."
            }
            Self::Items => "Queries VTube Studio for the item instances currently in the scene.",
        }
    }

    fn search_text(self) -> &'static str {
        match self {
            Self::CurrentModel => "vtube get current model name id loaded lookup vts",
            Self::Hotkeys => "vtube get hotkeys current model names ids lookup vts",
            Self::Expressions => "vtube get expressions names active state lookup vts",
            Self::Parameters => "vtube get parameters input live2d names lookup vts",
            Self::Items => "vtube get items scene instance ids file names lookup vts",
        }
    }

    fn icon_name(self) -> &'static str {
        match self {
            Self::CurrentModel => "user",
            Self::Hotkeys => "keyboard",
            Self::Expressions => "smile",
            Self::Parameters => "sliders",
            Self::Items => "image",
        }
    }

    async fn fetch(self, sink: &dyn VTubeSink) -> Result<Published, VTubeError> {
        Ok(match self {
            Self::CurrentModel => {
                let model = sink.get_current_model().await?;
                vec![
                    (variables::model::NAME, Variant::String(model.name)),
                    (variables::model::ID, Variant::String(model.id)),
                    (variables::model::LOADED, Variant::Bool(model.loaded)),
                ]
            }
            Self::Hotkeys => {
                let hotkeys = sink.get_hotkeys().await?;
                let count = count_of(hotkeys.names.len());
                vec![
                    (variables::hotkeys::NAMES, strings(hotkeys.names)),
                    (variables::hotkeys::IDS, strings(hotkeys.ids)),
                    (variables::hotkeys::COUNT, count),
                ]
            }
            Self::Expressions => {
                let expressions = sink.get_expressions().await?;
                let count = count_of(expressions.names.len());
                vec![
                    (variables::expressions::NAMES, strings(expressions.names)),
                    (variables::expressions::ACTIVE, bools(expressions.active)),
                    (variables::expressions::COUNT, count),
                ]
            }
            Self::Parameters => {
                let parameters = sink.get_parameters().await?;
                let count = count_of(parameters.names.len());
                vec![
                    (variables::parameters::NAMES, strings(parameters.names)),
                    (variables::parameters::COUNT, count),
                ]
            }
            Self::Items => {
                let items = sink.get_items().await?;
                let count = count_of(items.instance_ids.len());
                vec![
                    (variables::items::INSTANCE_IDS, strings(items.instance_ids)),
                    (variables::items::FILE_NAMES, strings(items.file_names)),
                    (variables::items::COUNT, count),
                ]
            }
        })
    }
}

pub(crate) struct LookupRunner {
    kind: LookupKind,
    sink: Arc<dyn VTubeSink>,
}

impl LookupRunner {
    pub(crate) fn new(kind: LookupKind, sink: Arc<dyn VTubeSink>) -> Self {
        Self { kind, sink }
    }
}

impl StepCatalogEntry for LookupRunner {
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Integration
    }
    fn label(&self) -> &str {
        self.kind.label()
    }
    fn summary(&self) -> &str {
        self.kind.summary()
    }
    fn search_text(&self) -> &str {
        self.kind.search_text()
    }
    fn icon_name(&self) -> &str {
        self.kind.icon_name()
    }
}

#[async_trait]
impl SubActionRunner for LookupRunner {
    fn id(&self) -> &str {
        self.kind.id()
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());

        match self.kind.fetch(self.sink.as_ref()).await {
            Ok(published) => {
                let stack = published
                    .into_iter()
                    .fold(ctx.arg_stack.clone(), |stack, (name, value)| {
                        stack.set(name.to_owned(), value)
                    });
                (timer.success(), Some(stack))
            }
            Err(e) => (timer.finish(step_failure::failed(&e)), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};
    use forge_types::SubActionOutcome;
    use std::collections::BTreeMap;

    fn text(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    fn texts(values: &[&str]) -> Variant {
        Variant::Array(values.iter().map(|value| text(value)).collect())
    }

    #[tokio::test]
    async fn execute_publishes_each_kinds_fields_into_the_arg_stack() {
        let cases: [(LookupKind, Vec<(&str, Variant)>); 5] = [
            (
                LookupKind::CurrentModel,
                vec![
                    ("vtube.model.name", text("MyAvatar")),
                    ("vtube.model.id", text("model-abc")),
                    ("vtube.model.loaded", Variant::Bool(true)),
                ],
            ),
            (
                LookupKind::Hotkeys,
                vec![
                    ("vtube.hotkeys.names", texts(&["Wave", "Blush"])),
                    ("vtube.hotkeys.ids", texts(&["hk-1", "hk-2"])),
                    ("vtube.hotkeys.count", Variant::Int(2)),
                ],
            ),
            (
                LookupKind::Expressions,
                vec![
                    (
                        "vtube.expressions.names",
                        texts(&["Smile.exp3.json", "Angry.exp3.json"]),
                    ),
                    (
                        "vtube.expressions.active",
                        Variant::Array(vec![Variant::Bool(true), Variant::Bool(false)]),
                    ),
                    ("vtube.expressions.count", Variant::Int(2)),
                ],
            ),
            (
                LookupKind::Parameters,
                vec![
                    (
                        "vtube.parameters.names",
                        texts(&["FaceAngleX", "MouthOpen"]),
                    ),
                    ("vtube.parameters.count", Variant::Int(2)),
                ],
            ),
            (
                LookupKind::Items,
                vec![
                    ("vtube.items.instance_ids", texts(&["inst-1"])),
                    ("vtube.items.file_names", texts(&["crown.png"])),
                    ("vtube.items.count", Variant::Int(1)),
                ],
            ),
        ];

        for (kind, expected) in cases {
            let runner = LookupRunner::new(kind, Arc::new(MockSink::new()));
            let stack = ArgStack::new();
            let ctx = make_ctx(&stack);
            let (tel, extra) = runner.execute(&BTreeMap::new(), &ctx).await;

            assert_eq!(tel.outcome, SubActionOutcome::Success, "{kind:?}");
            let out = extra.expect("success must surface an arg stack");
            for (name, value) in expected {
                assert_eq!(out.get(name), Some(&value), "{kind:?} {name}");
            }
        }
    }

    #[tokio::test]
    async fn execute_on_sink_error_fails_with_no_arg_stack() {
        for kind in LookupKind::ALL {
            let runner = LookupRunner::new(kind, Arc::new(MockSink::failing()));
            let stack = ArgStack::new();
            let ctx = make_ctx(&stack);
            let (tel, extra) = runner.execute(&BTreeMap::new(), &ctx).await;

            assert!(
                matches!(tel.outcome, SubActionOutcome::Failed(_)),
                "{kind:?}"
            );
            assert!(extra.is_none(), "{kind:?}");
        }
    }
}
