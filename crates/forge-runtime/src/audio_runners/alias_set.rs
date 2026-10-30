use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry};

use super::{ALIAS_SPEC, AliasTarget, read_alias};
use crate::speak_dispatcher::{SpeakDispatcher, platform_scoped_alias_key};

const ALIAS_SET_KIND_ID: &str = "tts.alias.set";

pub struct AliasSetRunner {
    speak: Arc<dyn SpeakDispatcher>,
}

impl AliasSetRunner {
    pub fn new(speak: Arc<dyn SpeakDispatcher>) -> Self {
        Self { speak }
    }
}

#[async_trait]
impl SubActionRunner for AliasSetRunner {
    fn id(&self) -> &str {
        ALIAS_SET_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Tts
    }

    fn label(&self) -> &str {
        "Set Voice Alias"
    }

    fn summary(&self) -> &str {
        "Assign a specific engine and voice to a viewer alias"
    }

    fn search_text(&self) -> &str {
        "tts alias voice set assign viewer engine"
    }

    fn icon_name(&self) -> &str {
        "user-check"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(ALIAS_SPEC)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, ALIAS_SET_KIND_ID);
        let AliasTarget {
            alias_name,
            engine_id,
            voice_id,
        } = match read_alias(config, ctx.arg_stack) {
            Ok(target) => target,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let alias_key = platform_scoped_alias_key(ctx.arg_stack, &alias_name);
        let result = self
            .speak
            .alias_set(alias_key, alias_name, engine_id, voice_id)
            .await;

        (
            timer.finish(crate::speak_dispatcher::dispatch_outcome(&result)),
            None,
        )
    }
}
