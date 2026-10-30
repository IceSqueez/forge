use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, NumberField, PickedTextField, RunContext, StepTimer, StoredNumber,
    SubActionCategory, SubActionRunner, TextPicker, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::step_failure;
use crate::ObsSink;
use crate::vocabulary::{config_keys, picker_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::AUDIO_SET_VOLUME;

const SOURCE: PickedTextField = PickedTextField {
    key: config_keys::SOURCE,
    label: "Audio Input",
    picker: TextPicker::Options {
        options_key: picker_keys::AUDIO_INPUTS,
    },
    default: "",
    required: true,
};

const VOLUME_DB: NumberField = NumberField {
    key: config_keys::VOLUME_DB,
    label: "Volume (dB)",
    placeholder: "e.g. -6.0",
    default: StoredNumber::Text("0.0"),
    bounds: ValueBounds::UNBOUNDED,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE, &VOLUME_DB]);

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<(String, f64), ConfigProblem> {
    Ok((SOURCE.read(config, args)?, VOLUME_DB.read(config, args)?))
}

pub struct SetVolumeRunner {
    sink: Arc<dyn ObsSink>,
}

impl SetVolumeRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SetVolumeRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Source Volume"
    }

    fn summary(&self) -> &str {
        "Sets the volume of an audio input in dB."
    }

    fn search_text(&self) -> &str {
        "obs audio volume input set db decibel source"
    }

    fn icon_name(&self) -> &str {
        "volume-2"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let (source, db) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = step_failure::outcome(&self.sink.set_input_volume_db(&source, db).await);
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, RecordingSink, make_ctx};
    use forge_types::SubActionOutcome;
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn runner() -> SetVolumeRunner {
        SetVolumeRunner::new(Arc::new(MockSink))
    }

    fn with_db(db: Option<Variant>) -> SubActionConfig {
        let mut config = BTreeMap::from([("source".to_owned(), Variant::String("Mic".to_owned()))]);
        if let Some(db) = db {
            config.insert("volume_db".to_owned(), db);
        }
        config
    }

    #[test]
    fn validate_config_accepts_a_decibel_figure_in_any_numeric_shape() {
        for db in [
            Variant::String("-6.0".to_owned()),
            Variant::Float(-3.0),
            Variant::Int(0),
        ] {
            assert!(
                runner().validate_config(&with_db(Some(db.clone()))).is_ok(),
                "rejected volume_db = {db:?}",
            );
        }
    }

    #[test]
    fn validate_config_rejects_a_decibel_figure_that_is_blank_or_unparseable() {
        for db in [
            Some(Variant::String("loud".to_owned())),
            Some(Variant::String("NaN".to_owned())),
            Some(Variant::String(String::new())),
            Some(Variant::Bool(true)),
        ] {
            assert!(
                runner().validate_config(&with_db(db.clone())).is_err(),
                "accepted volume_db = {db:?}",
            );
        }
    }

    #[tokio::test]
    async fn execute_sets_zero_decibels_when_the_figure_is_absent() {
        let sink = RecordingSink::new();
        let runner = SetVolumeRunner::new(Arc::clone(&sink) as Arc<dyn ObsSink>);
        let stack = ArgStack::new();

        let (tel, _) = runner.execute(&with_db(None), &make_ctx(&stack)).await;

        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(sink.calls(), vec!["set_input_volume_db(Mic,0)".to_owned()]);
    }

    #[tokio::test]
    async fn execute_fails_an_unparseable_figure_without_a_sink_call() {
        let sink = RecordingSink::new();
        let runner = SetVolumeRunner::new(Arc::clone(&sink) as Arc<dyn ObsSink>);
        let stack = ArgStack::new();
        let config = with_db(Some(Variant::String("loud".to_owned())));

        let (tel, _) = runner.execute(&config, &make_ctx(&stack)).await;

        assert!(
            matches!(&tel.outcome, SubActionOutcome::Failed(reason)
                if reason.code_or_text().starts_with("invalid step config: 'volume_db' ")),
            "{:?}",
            tel.outcome,
        );
        assert!(sink.calls().is_empty());
    }

    #[tokio::test]
    async fn execute_reports_success_with_correct_kind() {
        let stack = ArgStack::new();
        let config = BTreeMap::from([
            ("source".to_owned(), Variant::String("Mic".to_owned())),
            ("volume_db".to_owned(), Variant::String("-6.0".to_owned())),
        ]);
        let (tel, extra) = runner().execute(&config, &make_ctx(&stack)).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "obs.audio.set_volume");
        assert!(extra.is_none());
    }
}
