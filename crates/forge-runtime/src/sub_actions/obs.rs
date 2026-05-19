use std::sync::Arc;

use forge_obs::{ObsError, ObsSink};
use forge_types::{ArgStack, SubActionOutcome, SubActionSpec, SubActionTelemetry};
use time::OffsetDateTime;

pub(super) async fn run(
    spec: &SubActionSpec,
    index: usize,
    obs_sink: Option<Arc<dyn ObsSink>>,
) -> (SubActionTelemetry, Option<ArgStack>) {
    let kind = spec.kind_label().to_string();
    let started_at = OffsetDateTime::now_utc();

    let Some(sink) = obs_sink else {
        return (
            SubActionTelemetry {
                index,
                kind,
                started_at,
                duration_ms: 0,
                outcome: SubActionOutcome::Skipped("OBS not connected".to_string()),
            },
            None,
        );
    };

    let result: Result<(), ObsError> = match spec {
        SubActionSpec::ObsSetScene { scene_name } => sink.set_scene(scene_name).await,
        SubActionSpec::ObsSetSourceVisible {
            scene_name,
            source_name,
            visible,
        } => {
            sink.set_source_visible(scene_name, source_name, *visible)
                .await
        }
        SubActionSpec::ObsSetInputMute { input_name, muted } => {
            sink.set_input_mute(input_name, *muted).await
        }
        SubActionSpec::ObsStartRecord => sink.start_record().await,
        SubActionSpec::ObsStopRecord => sink.stop_record().await,
        SubActionSpec::ObsStartStream => sink.start_stream().await,
        SubActionSpec::ObsStopStream => sink.stop_stream().await,
        SubActionSpec::ObsRaw {
            request_type,
            payload,
        } => sink.raw_request(request_type, payload).await.map(|_| ()),
        _ => unreachable!("obs::run called with non-OBS SubActionSpec"),
    };

    let duration_ms = (OffsetDateTime::now_utc() - started_at)
        .whole_milliseconds()
        .max(0) as u64;

    let outcome = match result {
        Ok(()) => SubActionOutcome::Success,
        Err(e) => SubActionOutcome::Failed(e.to_string()),
    };

    (
        SubActionTelemetry {
            index,
            kind,
            started_at,
            duration_ms,
            outcome,
        },
        None,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use forge_obs::{ObsError, ObsSink};
    use forge_types::{SubActionOutcome, SubActionSpec, Variant};

    use super::run;

    struct SuccessSink;

    #[async_trait]
    impl ObsSink for SuccessSink {
        async fn set_scene(&self, _scene: &str) -> Result<(), ObsError> {
            Ok(())
        }

        async fn set_source_visible(
            &self,
            _scene: &str,
            _source: &str,
            _visible: bool,
        ) -> Result<(), ObsError> {
            Ok(())
        }

        async fn set_input_mute(&self, _input: &str, _mute: bool) -> Result<(), ObsError> {
            Ok(())
        }

        async fn start_record(&self) -> Result<(), ObsError> {
            Ok(())
        }

        async fn stop_record(&self) -> Result<(), ObsError> {
            Ok(())
        }

        async fn start_stream(&self) -> Result<(), ObsError> {
            Ok(())
        }

        async fn stop_stream(&self) -> Result<(), ObsError> {
            Ok(())
        }

        async fn raw_request(
            &self,
            _request_type: &str,
            _payload: &Variant,
        ) -> Result<Variant, ObsError> {
            Ok(Variant::Bool(true))
        }
    }

    struct FailSink;

    #[async_trait]
    impl ObsSink for FailSink {
        async fn set_scene(&self, _scene: &str) -> Result<(), ObsError> {
            Err(ObsError::Disconnected)
        }

        async fn set_source_visible(
            &self,
            _scene: &str,
            _source: &str,
            _visible: bool,
        ) -> Result<(), ObsError> {
            Err(ObsError::Disconnected)
        }

        async fn set_input_mute(&self, _input: &str, _mute: bool) -> Result<(), ObsError> {
            Err(ObsError::Disconnected)
        }

        async fn start_record(&self) -> Result<(), ObsError> {
            Err(ObsError::Disconnected)
        }

        async fn stop_record(&self) -> Result<(), ObsError> {
            Err(ObsError::Disconnected)
        }

        async fn start_stream(&self) -> Result<(), ObsError> {
            Err(ObsError::Disconnected)
        }

        async fn stop_stream(&self) -> Result<(), ObsError> {
            Err(ObsError::Disconnected)
        }

        async fn raw_request(
            &self,
            _request_type: &str,
            _payload: &Variant,
        ) -> Result<Variant, ObsError> {
            Err(ObsError::Disconnected)
        }
    }

    #[tokio::test]
    async fn none_sink_returns_skipped_for_set_scene() {
        let spec = SubActionSpec::ObsSetScene {
            scene_name: "Gaming".to_string(),
        };
        let (telemetry, updated) = run(&spec, 0, None).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Skipped(_)));
        assert_eq!(telemetry.kind, "ObsSetScene");
        assert!(updated.is_none());
    }

    #[tokio::test]
    async fn success_sink_set_scene_returns_success() {
        let spec = SubActionSpec::ObsSetScene {
            scene_name: "Gaming".to_string(),
        };
        let sink: Option<Arc<dyn ObsSink>> = Some(Arc::new(SuccessSink));
        let (telemetry, updated) = run(&spec, 0, sink).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert!(updated.is_none());
    }

    #[tokio::test]
    async fn fail_sink_set_scene_returns_failed() {
        let spec = SubActionSpec::ObsSetScene {
            scene_name: "Gaming".to_string(),
        };
        let sink: Option<Arc<dyn ObsSink>> = Some(Arc::new(FailSink));
        let (telemetry, _) = run(&spec, 0, sink).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn success_sink_set_source_visible_returns_success() {
        let spec = SubActionSpec::ObsSetSourceVisible {
            scene_name: "Gaming".to_string(),
            source_name: "Camera".to_string(),
            visible: true,
        };
        let sink: Option<Arc<dyn ObsSink>> = Some(Arc::new(SuccessSink));
        let (telemetry, _) = run(&spec, 1, sink).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert_eq!(telemetry.index, 1);
    }

    #[tokio::test]
    async fn success_sink_raw_request_returns_success() {
        let spec = SubActionSpec::ObsRaw {
            request_type: "GetVersion".to_string(),
            payload: Variant::Object(std::collections::BTreeMap::new()),
        };
        let sink: Option<Arc<dyn ObsSink>> = Some(Arc::new(SuccessSink));
        let (telemetry, _) = run(&spec, 2, sink).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
    }

    #[tokio::test]
    async fn none_sink_start_stream_returns_skipped() {
        let spec = SubActionSpec::ObsStartStream;
        let (telemetry, _) = run(&spec, 0, None).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Skipped(_)));
        assert_eq!(telemetry.kind, "ObsStartStream");
    }
}
