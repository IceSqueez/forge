use forge_audio::PcmBuffer;
use forge_net::{CallClass, GatewayFailure, GatewayRequest, RequestBody, Secret};
use forge_tts_core::{SynthesisRequest, VoiceId};
use serde_json::json;

use crate::gateway_failure::{
    PAYMENT_REQUIRED, failure_detail, probe_failure_detail, quota_excerpt, rejected_excerpt,
};
use crate::network::ProviderLink;
use crate::openai::error::OpenAiError;

const VALID_VOICES: &[&str] = &["alloy", "echo", "fable", "onyx", "nova", "shimmer"];
const SPEECH_PATH: &str = "/v1/audio/speech";
const MODELS_PATH: &str = "/v1/models";
const INVALID_API_KEY: &str = "invalid API key";
const QUOTA_MARKERS: &[&str] = &["insufficient_quota", "billing"];

fn map_voice_id(id: &VoiceId) -> Result<&'static str, OpenAiError> {
    VALID_VOICES
        .iter()
        .find(|&&v| v == id.0.as_str())
        .copied()
        .ok_or_else(|| OpenAiError::VoiceNotFound(id.clone()))
}

fn map_speed(rate_multiplier: f32) -> f64 {
    f64::from(rate_multiplier.clamp(0.25, 4.0))
}

fn classify_error(failure: &GatewayFailure, link: &ProviderLink) -> OpenAiError {
    if let Some(excerpt) = quota_excerpt(failure, QUOTA_MARKERS) {
        return OpenAiError::QuotaExceeded(excerpt.to_owned());
    }
    if let Some(retry_after_secs) = link.throttle_secs(failure) {
        return OpenAiError::RateLimited { retry_after_secs };
    }
    if let Some(excerpt) = rejected_excerpt(failure, PAYMENT_REQUIRED) {
        return OpenAiError::QuotaExceeded(excerpt.to_owned());
    }
    match failure {
        GatewayFailure::Unauthorized { .. } => OpenAiError::Unauthorized(INVALID_API_KEY.into()),
        other => OpenAiError::Http(failure_detail(other)),
    }
}

pub(super) async fn synthesize(
    link: &ProviderLink,
    api_key: &str,
    req: SynthesisRequest,
) -> Result<PcmBuffer, OpenAiError> {
    let voice = map_voice_id(&req.voice_id)?;
    let speed = map_speed(req.rate_multiplier);

    if req.pitch_semitones.abs() > f32::EPSILON {
        tracing::debug!(
            pitch_semitones = req.pitch_semitones,
            "OpenAI TTS has no pitch control; pitch_semitones ignored"
        );
    }

    let body = RequestBody::json(&json!({
        "model": "tts-1",
        "voice": voice,
        "input": req.text,
        "response_format": "wav",
        "speed": speed,
    }))
    .map_err(|e| OpenAiError::Http(e.to_string()))?;

    let request = GatewayRequest::post(SPEECH_PATH)
        .bearer(Secret::new(api_key))
        .body(body);
    let response = link
        .send(request)
        .await
        .map_err(|failure| classify_error(&failure, link))?;
    forge_audio::decode_bytes(response.body(), Some("wav"))
        .map_err(|e| OpenAiError::Decode(e.to_string()))
}

pub(super) async fn probe_connection(
    link: &ProviderLink,
    api_key: &str,
) -> Result<(), OpenAiError> {
    let request = GatewayRequest::get(MODELS_PATH)
        .bearer(Secret::new(api_key))
        .class(CallClass::Catalog);
    match link.send(request).await {
        Ok(_) => Ok(()),
        Err(GatewayFailure::Unauthorized { .. }) => {
            Err(OpenAiError::Unauthorized(INVALID_API_KEY.into()))
        }
        Err(failure) => match link.throttle_secs(&failure) {
            Some(retry_after_secs) => Err(OpenAiError::RateLimited { retry_after_secs }),
            None => Err(OpenAiError::Http(probe_failure_detail(&failure))),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use crate::network::test_support::{
        API_KEY, NO_HINT_WAIT_SECS, assert_reveals_nothing, catalog_cap_text, link_to,
        over_catalog_cap, request_count, serving, throttled,
    };
    use forge_net::CallClass;
    use forge_tts_core::TtsError;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_wav_bytes(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let data_len = (samples.len() * 2) as u32;
        let block_align = channels * 2;
        let byte_rate = sample_rate * u32::from(block_align);
        let file_len = 36 + data_len;

        let mut buf = Vec::with_capacity(44 + data_len as usize);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&file_len.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        buf.extend_from_slice(&block_align.to_le_bytes());
        buf.extend_from_slice(&16u16.to_le_bytes());
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_len.to_le_bytes());
        for &s in samples {
            buf.extend_from_slice(&s.to_le_bytes());
        }
        buf
    }

    fn silent_wav(sample_rate: u32, frames: usize) -> Vec<u8> {
        make_wav_bytes(sample_rate, 1, &vec![0i16; frames])
    }

    fn request_for(voice: &str) -> SynthesisRequest {
        SynthesisRequest {
            text: "hello".into(),
            voice_id: VoiceId(voice.into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            ssml: false,
        }
    }

    fn outcome(error: &OpenAiError) -> String {
        match error {
            OpenAiError::Http(_) => "Http".into(),
            OpenAiError::Unauthorized(_) => "Unauthorized".into(),
            OpenAiError::QuotaExceeded(_) => "QuotaExceeded".into(),
            OpenAiError::RateLimited { retry_after_secs } => {
                format!("RateLimited({retry_after_secs})")
            }
            OpenAiError::VoiceNotFound(_) => "VoiceNotFound".into(),
            OpenAiError::Decode(_) => "Decode".into(),
            OpenAiError::Io(_) => "Io".into(),
        }
    }

    #[tokio::test]
    async fn synthesize_sends_the_bearer_key_and_decodes_the_wav_reply() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/audio/speech"))
            .and(header(
                "authorization",
                format!("Bearer {API_KEY}").as_str(),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(silent_wav(24_000, 2400)))
            .mount(&server)
            .await;

        let pcm = synthesize(&link_to(&server), API_KEY, request_for("alloy"))
            .await
            .unwrap();

        assert_eq!(
            (pcm.sample_rate, pcm.channels, pcm.samples.len()),
            (24_000, 1, 2400)
        );
    }

    #[tokio::test]
    async fn synthesize_rejects_an_unknown_voice_before_any_request() {
        let server = serving(ResponseTemplate::new(200)).await;
        let result = synthesize(&link_to(&server), API_KEY, request_for("unknown-voice")).await;
        assert!(matches!(result, Err(OpenAiError::VoiceNotFound(_))));
        assert_eq!(request_count(&server).await, 0);
    }

    #[tokio::test]
    async fn synthesis_failures_map_to_provider_errors_without_revealing_the_key() {
        let echo = format!("key {API_KEY} rejected");
        let no_hint = format!("RateLimited({NO_HINT_WAIT_SECS})");
        for (template, expected) in [
            (
                ResponseTemplate::new(401).set_body_string(echo.clone()),
                "Unauthorized".to_owned(),
            ),
            (
                ResponseTemplate::new(402).set_body_string(echo.clone()),
                "QuotaExceeded".to_owned(),
            ),
            (
                ResponseTemplate::new(429).set_body_string("slow down"),
                no_hint.clone(),
            ),
            (throttled(7), "RateLimited(7)".to_owned()),
            (
                ResponseTemplate::new(429).set_body_string(format!(
                    r#"{{"code":"insufficient_quota","key":"{API_KEY}"}}"#
                )),
                "QuotaExceeded".to_owned(),
            ),
            (
                ResponseTemplate::new(429).set_body_string("check your billing details"),
                "QuotaExceeded".to_owned(),
            ),
            (
                ResponseTemplate::new(429).set_body_string("INSUFFICIENT_QUOTA"),
                "QuotaExceeded".to_owned(),
            ),
            (
                ResponseTemplate::new(429).set_body_string("Billing hard limit reached"),
                "QuotaExceeded".to_owned(),
            ),
            (
                ResponseTemplate::new(429).set_body_string("Rate limit reached for requests"),
                no_hint.clone(),
            ),
            (
                ResponseTemplate::new(503)
                    .insert_header("Retry-After", "2")
                    .set_body_string("insufficient_quota"),
                "RateLimited(2)".to_owned(),
            ),
            (
                ResponseTemplate::new(503).set_body_string(echo.clone()),
                "Http".to_owned(),
            ),
            (
                ResponseTemplate::new(500).set_body_string(echo.clone()),
                "Http".to_owned(),
            ),
        ] {
            let server = serving(template).await;
            let error = synthesize(&link_to(&server), API_KEY, request_for("alloy"))
                .await
                .unwrap_err();
            assert_eq!(outcome(&error), expected, "{error}");
            assert_reveals_nothing(&error.to_string(), &server);
            assert_reveals_nothing(&TtsError::from(error).to_string(), &server);
        }
    }

    #[tokio::test]
    async fn synthesis_reads_replies_larger_than_the_catalog_cap() {
        let frames = usize::try_from(CallClass::Catalog.body_cap()).unwrap() / 2 + 1;
        let server =
            serving(ResponseTemplate::new(200).set_body_bytes(silent_wav(24_000, frames))).await;

        let pcm = synthesize(&link_to(&server), API_KEY, request_for("alloy"))
            .await
            .unwrap();

        assert_eq!(pcm.samples.len(), frames);
    }

    #[tokio::test]
    async fn probe_outcomes_follow_the_models_endpoint_status() {
        let no_hint = format!("RateLimited({NO_HINT_WAIT_SECS})");
        for (template, expected) in [
            (
                ResponseTemplate::new(200).set_body_bytes(b"{}"),
                "Ok".to_owned(),
            ),
            (ResponseTemplate::new(401), "Unauthorized".to_owned()),
            (ResponseTemplate::new(429), no_hint),
            (throttled(9), "RateLimited(9)".to_owned()),
            (ResponseTemplate::new(403), "Http".to_owned()),
            (ResponseTemplate::new(500), "Http".to_owned()),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v1/models"))
                .and(header(
                    "authorization",
                    format!("Bearer {API_KEY}").as_str(),
                ))
                .respond_with(template)
                .mount(&server)
                .await;
            let result = probe_connection(&link_to(&server), API_KEY).await;
            let got = result.as_ref().map_or_else(outcome, |_| "Ok".to_owned());
            assert_eq!(got, expected);
        }
    }

    #[tokio::test]
    async fn probe_reports_the_status_without_the_body() {
        let server =
            serving(ResponseTemplate::new(500).set_body_string(format!("boom {API_KEY}"))).await;
        let error = probe_connection(&link_to(&server), API_KEY)
            .await
            .unwrap_err();
        assert!(
            matches!(&error, OpenAiError::Http(detail) if detail == "unexpected status 500"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn probe_stops_reading_at_the_catalog_cap() {
        let server = serving(ResponseTemplate::new(200).set_body_bytes(over_catalog_cap())).await;
        let error = probe_connection(&link_to(&server), API_KEY)
            .await
            .unwrap_err();
        assert!(
            matches!(&error, OpenAiError::Http(detail) if detail.contains(&catalog_cap_text())),
            "{error}"
        );
    }
}
