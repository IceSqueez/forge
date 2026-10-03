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
