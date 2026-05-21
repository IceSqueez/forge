use std::sync::Arc;

use forge_types::{ArgStack, SubActionOutcome, SubActionSpec, SubActionTelemetry};
use time::OffsetDateTime;

use crate::speak_dispatcher::SpeakDispatcher;

pub(super) async fn run(
    spec: &SubActionSpec,
    index: usize,
    dispatcher: Option<&Arc<dyn SpeakDispatcher>>,
) -> (SubActionTelemetry, Option<ArgStack>) {
    let kind = spec.kind_label().to_string();
    let started_at = OffsetDateTime::now_utc();

    let Some(dispatcher) = dispatcher else {
        return (
            SubActionTelemetry {
                index,
                kind,
                started_at,
                duration_ms: 0,
                outcome: SubActionOutcome::Skipped("speak subsystem unavailable".to_string()),
            },
            None,
        );
    };

    let SubActionSpec::Speak {
        text,
        voice_id_override,
    } = spec
    else {
        unreachable!("speak::run called with non-Speak spec")
    };

    let result = dispatcher
        .speak(text.clone(), voice_id_override.clone())
        .await;

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
    use async_trait::async_trait;
    use forge_types::{SubActionOutcome, SubActionSpec};

    use super::*;
    use crate::speak_dispatcher::{SpeakDispatchError, SpeakDispatcher};

    struct AlwaysOkSpeaker;

    #[async_trait]
    impl SpeakDispatcher for AlwaysOkSpeaker {
        async fn speak(
            &self,
            _text: String,
            _voice_id_override: Option<String>,
        ) -> Result<(), SpeakDispatchError> {
            Ok(())
        }
    }

    struct AlwaysFailSpeaker;

    #[async_trait]
    impl SpeakDispatcher for AlwaysFailSpeaker {
        async fn speak(
            &self,
            _text: String,
            _voice_id_override: Option<String>,
        ) -> Result<(), SpeakDispatchError> {
            Err(SpeakDispatchError::Dispatch("queue full".to_string()))
        }
    }

    fn speak_spec() -> SubActionSpec {
        SubActionSpec::Speak {
            text: "Hello chat!".to_string(),
            voice_id_override: None,
        }
    }

    #[tokio::test]
    async fn none_dispatcher_returns_skipped() {
        let spec = speak_spec();
        let (telemetry, updated) = run(&spec, 0, None).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Skipped(_)));
        assert_eq!(telemetry.kind, "Speak");
        assert_eq!(telemetry.index, 0);
        assert!(updated.is_none());
    }

    #[tokio::test]
    async fn ok_dispatcher_returns_success() {
        let spec = speak_spec();
        let dispatcher: Arc<dyn SpeakDispatcher> = Arc::new(AlwaysOkSpeaker);
        let (telemetry, updated) = run(&spec, 1, Some(&dispatcher)).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert_eq!(telemetry.index, 1);
        assert!(updated.is_none());
    }

    #[tokio::test]
    async fn fail_dispatcher_returns_failed() {
        let spec = speak_spec();
        let dispatcher: Arc<dyn SpeakDispatcher> = Arc::new(AlwaysFailSpeaker);
        let (telemetry, _) = run(&spec, 2, Some(&dispatcher)).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(telemetry.index, 2);
    }

    #[tokio::test]
    async fn voice_override_forwarded() {
        use std::sync::{Arc, Mutex};

        struct CapturingSpeaker {
            captured_override: Arc<Mutex<Option<String>>>,
        }

        #[async_trait]
        impl SpeakDispatcher for CapturingSpeaker {
            async fn speak(
                &self,
                _text: String,
                voice_id_override: Option<String>,
            ) -> Result<(), SpeakDispatchError> {
                *self.captured_override.lock().unwrap() = voice_id_override;
                Ok(())
            }
        }

        let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let dispatcher: Arc<dyn SpeakDispatcher> = Arc::new(CapturingSpeaker {
            captured_override: Arc::clone(&captured),
        });

        let spec = SubActionSpec::Speak {
            text: "test".to_string(),
            voice_id_override: Some("piper/en_US-amy-medium".to_string()),
        };

        run(&spec, 0, Some(&dispatcher)).await;

        assert_eq!(
            *captured.lock().unwrap(),
            Some("piper/en_US-amy-medium".to_string())
        );
    }
}
