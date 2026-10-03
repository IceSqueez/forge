use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRegistry, SubActionRunner,
};
use forge_types::{
    ArgStack, LATEST_DONATION_SLOT, LatestScope, LatestValueReader, SubActionConfig,
    SubActionTelemetry, Variant, VariantKind, strip_var_decoration,
};

use crate::latest::slots::{latest_slot_ids, slot_declaration};

pub const LATEST_GET_SUB_ACTION: &str = "core.latest.get";

const SLOT_KEY: &str = "slot";
const PLATFORM_KEY: &str = "platform";
const INTO_VAR_KEY: &str = "into_var";
const DEFAULT_INTO_VAR: &str = "latest";

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

    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::from([
            (
                SLOT_KEY.to_owned(),
                Variant::String(LATEST_DONATION_SLOT.to_owned()),
            ),
            (PLATFORM_KEY.to_owned(), Variant::String(String::new())),
            (
                INTO_VAR_KEY.to_owned(),
                Variant::String(DEFAULT_INTO_VAR.to_owned()),
            ),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Select {
                key: SLOT_KEY,
                label: "Slot",
                options: latest_slot_ids(),
            },
            FormField::Text {
                key: PLATFORM_KEY,
                label: "Platform or service (empty = most recent of all)",
                placeholder: "",
            },
            FormField::Text {
                key: INTO_VAR_KEY,
                label: "Output Variable",
                placeholder: DEFAULT_INTO_VAR,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let slot = config.require_str(SLOT_KEY)?;
        if slot_declaration(slot).is_none() {
            return Err(RegistryError::InvalidConfig(format!(
                "unknown latest slot '{slot}'"
            )));
        }
        Ok(())
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::Object,
                label: "Latest value".to_owned(),
            }],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, LATEST_GET_SUB_ACTION);
        let slot = config.str(SLOT_KEY).unwrap_or_default();
        if slot_declaration(slot).is_none() {
            return (timer.failed(format!("unknown latest slot '{slot}'")), None);
        }
        let platform = ctx
            .arg_stack
            .interpolate(config.str(PLATFORM_KEY).unwrap_or_default());
        let into_var = strip_var_decoration(
            config
                .str_nonempty(INTO_VAR_KEY)
                .unwrap_or(DEFAULT_INTO_VAR),
        );
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Mutex;

    use forge_events::{Event, EventPublisher};
    use forge_types::{EventId, LatestValue, NOW_PLAYING_SLOT, SubActionOutcome};
    use time::OffsetDateTime;

    use super::*;

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
            (SLOT_KEY.to_owned(), Variant::String(slot.to_owned())),
            (
                PLATFORM_KEY.to_owned(),
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
            matches!(outcome, SubActionOutcome::Failed(reason) if reason.contains("no_such_slot"))
        );
        assert!(stack.is_none());
        assert!(reader.asked.lock().unwrap().is_empty());
    }

    #[test]
    fn validate_config_accepts_every_declared_slot_and_rejects_others() {
        let runner = LatestGetRunner::new(Arc::new(Reader::default()));
        for slot in latest_slot_ids() {
            assert!(
                runner.validate_config(&config(slot, "", "latest")).is_ok(),
                "{slot}"
            );
        }
        for slot in ["", "no_such_slot", "Donation"] {
            assert!(
                matches!(
                    runner.validate_config(&config(slot, "", "latest")),
                    Err(RegistryError::InvalidConfig(_))
                ),
                "{slot:?}"
            );
        }
    }
}
