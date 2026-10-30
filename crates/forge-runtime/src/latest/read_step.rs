use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, ListedChoiceField, ProducedVariable, RegistryError, RunContext,
    StepTimer, SubActionCategory, SubActionIo, SubActionRegistry, SubActionRunner, TextEditor,
    TextField, TypedField, VariableField,
};
use forge_types::{
    ArgStack, LATEST_DONATION_SLOT, LatestScope, LatestValueReader, SubActionConfig,
    SubActionTelemetry, Variant, VariantKind,
};

use crate::latest::slots::latest_slot_ids;

pub const LATEST_GET_SUB_ACTION: &str = "core.latest.get";

const INTO_VAR_KEY: &str = "into_var";
const DEFAULT_INTO_VAR: &str = "latest";

const SLOT: ListedChoiceField = ListedChoiceField {
    key: "slot",
    label: "Slot",
    default: LATEST_DONATION_SLOT,
    options: latest_slot_ids,
};

const PLATFORM: TextField = TextField {
    key: "platform",
    label: "Platform or service (empty = most recent of all)",
    editor: TextEditor::Line { placeholder: "" },
    default: "",
    required: false,
};

const INTO_VAR: VariableField = VariableField {
    key: INTO_VAR_KEY,
    label: "Output Variable",
    placeholder: DEFAULT_INTO_VAR,
    default: DEFAULT_INTO_VAR,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SLOT, &PLATFORM, &INTO_VAR]);

struct LatestRead {
    slot: &'static str,
    platform: String,
    into_var: String,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<LatestRead, ConfigProblem> {
    Ok(LatestRead {
        slot: SLOT.read(config, args)?,
        platform: PLATFORM.read(config, args)?,
        into_var: INTO_VAR.read(config, args)?,
    })
}

pub struct LatestGetRunner {
    values: Arc<dyn LatestValueReader>,
}

impl LatestGetRunner {
    pub fn new(values: Arc<dyn LatestValueReader>) -> Self {
        Self { values }
    }
}

#[async_trait]
impl SubActionRunner for LatestGetRunner {
    fn id(&self) -> &str {
        LATEST_GET_SUB_ACTION
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Get Latest Value"
    }

    fn summary(&self) -> &str {
        "Read the latest recorded value of a widget slot (such as the last donation) into an argument"
    }

    fn search_text(&self) -> &str {
        "latest last donation tip cheer bits super chat now playing widget read"
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
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::Object,
                label: "Latest value".to_owned(),
            }],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, LATEST_GET_SUB_ACTION);
        let LatestRead {
            slot,
            platform,
            into_var,
        } = match read_config(config, ctx.arg_stack) {
            Ok(read) => read,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let value = self
            .values
            .latest(slot, LatestScope::from_platform_filter(&platform))
            .map(|value| value.to_variant())
            .unwrap_or_else(|| Variant::Object(BTreeMap::new()));
        let stack = ctx.arg_stack.clone().set(into_var, value);
        (timer.success(), Some(stack))
    }
}

pub fn register_latest_sub_actions(
    registry: &mut SubActionRegistry,
    values: Arc<dyn LatestValueReader>,
) -> Result<(), RegistryError> {
    registry.register(Box::new(LatestGetRunner::new(values)))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use forge_events::{Event, EventPublisher};
    use forge_types::{EventId, LatestValue, NOW_PLAYING_SLOT, SubActionOutcome};
    use time::OffsetDateTime;

    use super::*;
    use forge_registry::ConfigReason;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Asked {
        Merged(String),
        Platform(String, String),
    }

    #[derive(Default)]
    struct Reader {
        filled: bool,
        asked: Mutex<Vec<Asked>>,
    }

    impl LatestValueReader for Reader {
        fn latest(&self, slot: &str, scope: LatestScope<'_>) -> Option<LatestValue> {
            let (asked, platform) = match scope {
                LatestScope::MostRecentAcrossPlatforms => {
                    (Asked::Merged(slot.to_owned()), "merged")
                }
                LatestScope::Platform(platform) => (
                    Asked::Platform(slot.to_owned(), platform.to_owned()),
                    platform,
                ),
            };
            self.asked.lock().unwrap().push(asked);
            self.filled.then(|| {
                LatestValue::new(
                    platform,
                    OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap(),
                    BTreeMap::from([("user_name".to_owned(), Variant::String("Olena".to_owned()))]),
                )
            })
        }
    }

    struct NullPublisher;

    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    fn config(slot: &str, platform: &str, into_var: &str) -> SubActionConfig {
        SubActionConfig::from([
            (SLOT.key.to_owned(), Variant::String(slot.to_owned())),
            (
                PLATFORM.key.to_owned(),
                Variant::String(platform.to_owned()),
            ),
            (
                INTO_VAR_KEY.to_owned(),
                Variant::String(into_var.to_owned()),
            ),
        ])
    }

    async fn run(
        reader: Arc<Reader>,
        config: &SubActionConfig,
        stack: ArgStack,
    ) -> (SubActionOutcome, Option<ArgStack>) {
        let runner = LatestGetRunner::new(reader);
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (telemetry, stack) = runner.execute(config, &ctx).await;
        (telemetry.outcome, stack)
    }

    fn user_name_in(stack: &ArgStack, var: &str) -> Option<Variant> {
        match stack.get(var) {
            Some(Variant::Object(fields)) => fields.get("user_name").cloned(),
            _ => None,
        }
    }

    #[tokio::test]
    async fn platform_filter_is_interpolated_and_blank_means_merged() {
        let stack = ArgStack::new().set("svc".to_owned(), Variant::String("monobank".to_owned()));
        for (platform, expected) in [
            ("", Asked::Merged(LATEST_DONATION_SLOT.to_owned())),
            ("  ", Asked::Merged(LATEST_DONATION_SLOT.to_owned())),
            (
                "%svc%",
                Asked::Platform(LATEST_DONATION_SLOT.to_owned(), "monobank".to_owned()),
            ),
        ] {
            let reader = Arc::new(Reader::default());

            run(
                Arc::clone(&reader),
                &config(LATEST_DONATION_SLOT, platform, "latest"),
                stack.clone(),
            )
            .await;

            assert_eq!(*reader.asked.lock().unwrap(), [expected], "{platform:?}");
        }
    }

    #[tokio::test]
    async fn recorded_value_lands_in_the_output_variable_as_an_object() {
        let reader = Arc::new(Reader {
            filled: true,
            ..Reader::default()
        });

        let (outcome, stack) = run(
            reader,
            &config(LATEST_DONATION_SLOT, "", "%last_tip%"),
            ArgStack::new(),
        )
        .await;

        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            user_name_in(&stack.unwrap(), "last_tip"),
            Some(Variant::String("Olena".to_owned()))
        );
    }

    #[tokio::test]
    async fn empty_slot_yields_an_empty_object() {
        let (outcome, stack) = run(
            Arc::new(Reader::default()),
            &config(NOW_PLAYING_SLOT, "", "latest"),
            ArgStack::new(),
        )
        .await;

        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            stack.unwrap().get("latest"),
            Some(&Variant::Object(BTreeMap::new()))
        );
    }

    #[tokio::test]
    async fn unknown_slot_fails_the_step_without_reading() {
        let reader = Arc::new(Reader::default());

        let (outcome, stack) = run(
            Arc::clone(&reader),
            &config("no_such_slot", "", "latest"),
            ArgStack::new(),
        )
        .await;

        assert!(
            matches!(&outcome, SubActionOutcome::Failed(reason) if crate::test_support::failure::rejected_field(reason) == Some(SLOT.key)),
            "{outcome:?}"
        );
        assert!(stack.is_none());
        assert!(reader.asked.lock().unwrap().is_empty());
    }

    #[test]
    fn validate_config_accepts_every_declared_slot_and_lists_the_slots_for_any_other_or_blank() {
        let runner = LatestGetRunner::new(Arc::new(Reader::default()));
        for slot in latest_slot_ids() {
            assert!(
                runner.validate_config(&config(slot, "", "latest")).is_ok(),
                "{slot}"
            );
        }
        let listing = ConfigReason::choices(latest_slot_ids().iter().copied());
        for (slot, reason) in [
            ("", listing.clone()),
            ("no_such_slot", listing.clone()),
            ("Donation", listing),
        ] {
            assert_eq!(
                runner
                    .validate_config(&config(slot, "", "latest"))
                    .unwrap_err()
                    .into_config_problem(),
                forge_registry::ConfigProblem::field(SLOT.key, reason),
                "{slot:?}"
            );
        }
    }
}
