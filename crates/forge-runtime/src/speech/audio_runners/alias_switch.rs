use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, RunContext, StepCatalogEntry, StepTimer, SubActionCategory, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry};

use super::{AliasTarget, KNOWN_ALIAS_SPEC, read_alias};
use crate::speech::speak_dispatcher::{VoiceAliasWriter, platform_scoped_alias_key};

const ALIAS_SWITCH_KIND_ID: &str = "tts.alias.switch_active";

pub(crate) struct AliasSwitchRunner {
    speak: Arc<dyn VoiceAliasWriter>,
}

impl AliasSwitchRunner {
    pub(crate) fn new(speak: Arc<dyn VoiceAliasWriter>) -> Self {
        Self { speak }
    }
}

impl StepCatalogEntry for AliasSwitchRunner {
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Tts
    }
    fn label(&self) -> &str {
        "Switch Active Voice Alias"
    }
    fn summary(&self) -> &str {
        "Repoint an existing viewer alias to a different engine and voice; no-op if the viewer has no alias"
    }
    fn search_text(&self) -> &str {
        "tts alias voice switch change viewer engine"
    }
    fn icon_name(&self) -> &str {
        "refresh-cw"
    }
}

#[async_trait]
impl SubActionRunner for AliasSwitchRunner {
    fn id(&self) -> &str {
        ALIAS_SWITCH_KIND_ID
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(KNOWN_ALIAS_SPEC)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, ALIAS_SWITCH_KIND_ID);
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
            .alias_switch(alias_key, engine_id, voice_id)
            .await;

        (
            timer.finish(crate::speech::speak_dispatcher::dispatch_outcome(&result)),
            None,
        )
    }
}
