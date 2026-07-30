use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct SwitchCurrentSceneRunner {
    sink: Arc<dyn ObsSink>,
}

impl SwitchCurrentSceneRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SwitchCurrentSceneRunner {
    fn id(&self) -> &str {
        "obs.scenes.switch_current"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Switch Scene"
    }

    fn summary(&self) -> &str {
        "Sets the current OBS program scene."
    }

    fn search_text(&self) -> &str {
        "obs switch scene current program set"
    }

    fn icon_name(&self) -> &str {
        "arrows-shuffle"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("scene".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "scene",
            label: "Scene",
            options_key: "obs.scene_names",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("scene") {
            Some(Variant::String(s)) if !s.trim().is_empty() => Ok(()),
            Some(Variant::String(_)) => Err(RegistryError::InvalidConfig(
                "obs.scenes.switch_current: 'scene' must not be empty".to_owned(),
            )),
            _ => Err(RegistryError::InvalidConfig(
                "obs.scenes.switch_current: 'scene' must be a string".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw = config.str("scene").unwrap_or_default();
        let scene = ctx.arg_stack.interpolate(raw);

        let already_current = matches!(
            self.sink.get_current_scene().await,
            Ok(Some(current)) if current == scene
        );
        let outcome = if already_current {
            SubActionOutcome::Success
        } else {
            SubActionOutcome::from_result(&self.sink.set_scene(&scene).await)
        };

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "obs.scenes.switch_current".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::runners::test_support::{RecordingSink, make_ctx};

    fn scene_config(scene: &str) -> SubActionConfig {
        BTreeMap::from([("scene".to_owned(), Variant::String(scene.to_owned()))])
    }

    // Why: OBS re-runs the whole transition when the program scene is set to the scene already on
    // program, so a repeat trigger visibly restarts stingers and media sources.
    #[tokio::test]
    async fn switching_to_the_scene_already_on_program_reports_success_without_setting_it() {
        let sink = RecordingSink::new();
        let runner = SwitchCurrentSceneRunner::new(Arc::clone(&sink) as Arc<dyn ObsSink>);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&scene_config("Gameplay"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(sink.calls(), vec!["get_current_scene".to_owned()]);
    }

    #[tokio::test]
    async fn switching_to_a_scene_that_is_not_on_program_sets_it() {
        let sink = RecordingSink::new();
        let runner = SwitchCurrentSceneRunner::new(Arc::clone(&sink) as Arc<dyn ObsSink>);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&scene_config("Starting Soon"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            sink.calls(),
            vec![
                "get_current_scene".to_owned(),
                "set_scene(Starting Soon)".to_owned()
            ],
        );
    }

    // The guard must compare the resolved name; comparing the raw template would never match and
    // every interpolated switch would replay the transition it is meant to skip.
    #[tokio::test]
    async fn the_no_op_guard_compares_the_interpolated_scene_name() {
        let sink = RecordingSink::new();
        let runner = SwitchCurrentSceneRunner::new(Arc::clone(&sink) as Arc<dyn ObsSink>);
        let stack =
            ArgStack::new().set("target".to_owned(), Variant::String("Gameplay".to_owned()));

        runner
            .execute(&scene_config("%target%"), &make_ctx(&stack))
            .await;

        assert_eq!(sink.calls(), vec!["get_current_scene".to_owned()]);
    }

    // A sink that cannot report the program scene must not be read as "already there"; the switch
    // has to be attempted and its own failure reported.
    #[tokio::test]
    async fn an_unreadable_program_scene_still_attempts_the_switch() {
        let sink = RecordingSink::failing();
        let runner = SwitchCurrentSceneRunner::new(Arc::clone(&sink) as Arc<dyn ObsSink>);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&scene_config("Gameplay"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            sink.calls().contains(&"set_scene(Gameplay)".to_owned()),
            "the switch was skipped after a failed read: {:?}",
            sink.calls(),
        );
    }
}
