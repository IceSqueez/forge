use forge_net::{CallClass, GatewayFailure, GatewayRequest, RequestBody, Secret};
use forge_speech_types::PcmBuffer;
use forge_speech_types::VoiceId;
use forge_tts_core::SynthesisRequest;
use serde_json::json;

use crate::elevenlabs::SAMPLE_RATE_HZ;
use crate::elevenlabs::error::ElevenLabsError;
use crate::gateway_failure::{
    Classified, ProviderPolicy, VoiceSignal, classify, failure_detail, probe_failure_detail,
};
use crate::network::{ProviderLink, is_plain_path_segment};

pub(crate) const API_KEY_HEADER: &str = "xi-api-key";
pub(crate) const INVALID_API_KEY: &str = "invalid API key";
const SPEECH_PATH: &str = "/v1/text-to-speech";
const USER_PATH: &str = "/v1/user";
const OUTPUT_FORMAT_QUERY: &str = "output_format";
const QUOTA_MARKERS: &[&str] = &["quota", "characters"];

const POLICY: ProviderPolicy = ProviderPolicy {
    quota_markers: QUOTA_MARKERS,
    payment_required_is_quota: true,
    voice_signal: VoiceSignal::NotFoundStatus,
};

pub(super) fn classify_with(
    failure: &GatewayFailure,
    link: &ProviderLink,
    voice: Option<&VoiceId>,
    detail: fn(&GatewayFailure) -> String,
) -> ElevenLabsError {
    match classify(failure, link, POLICY, voice, detail) {
        Classified::Quota(excerpt) => ElevenLabsError::QuotaExceeded(excerpt),
        Classified::Throttled(retry_after_secs) => {
            ElevenLabsError::RateLimited { retry_after_secs }
        }
        Classified::Auth => ElevenLabsError::Unauthorized(INVALID_API_KEY.into()),
        Classified::VoiceMissing(voice) => ElevenLabsError::VoiceNotFound(voice),
        Classified::Rejected(detail) => ElevenLabsError::Rejected(detail),
        Classified::Transient(detail) => ElevenLabsError::Http(detail),
    }
}

fn classify_error(
    failure: &GatewayFailure,
    link: &ProviderLink,
    voice_id: &VoiceId,
) -> ElevenLabsError {
    classify_with(failure, link, Some(voice_id), failure_detail)
}

pub(super) async fn synthesize(
    link: &ProviderLink,
    api_key: &str,
    req: SynthesisRequest,
) -> Result<PcmBuffer, ElevenLabsError> {
    if !is_plain_path_segment(&req.voice_id.0) {
        return Err(ElevenLabsError::VoiceNotFound(req.voice_id));
    }
    if req.rate_multiplier != 1.0 {
        tracing::debug!(
            rate_multiplier = req.rate_multiplier,
            "ElevenLabs has no rate control; rate_multiplier ignored"
        );
    }
    if req.pitch_semitones.abs() > f32::EPSILON {
        tracing::debug!(
            pitch_semitones = req.pitch_semitones,
            "ElevenLabs has no pitch control; pitch_semitones ignored"
        );
    }

    let body = RequestBody::json(&json!({
        "text": req.text,
        "model_id": "eleven_multilingual_v2",
        "voice_settings": {
            "stability": 0.5,
            "similarity_boost": 0.75
        }
    }))
    .map_err(|e| ElevenLabsError::Rejected(e.to_string()))?;

    let request = GatewayRequest::post(format!("{SPEECH_PATH}/{}", req.voice_id.0))
        .query(OUTPUT_FORMAT_QUERY, format!("pcm_{SAMPLE_RATE_HZ}"))
        .api_key(API_KEY_HEADER, Secret::new(api_key))
        .body(body);
    let response = link
        .send(request)
        .await
        .map_err(|failure| classify_error(&failure, link, &req.voice_id))?;

    let (pairs, _) = response.body().as_chunks::<2>();
    let samples: Vec<i16> = pairs.iter().map(|pair| i16::from_le_bytes(*pair)).collect();
    Ok(PcmBuffer::new(samples, SAMPLE_RATE_HZ, 1))
}

pub(super) async fn probe_connection(
    link: &ProviderLink,
    api_key: &str,
) -> Result<(), ElevenLabsError> {
    let request = GatewayRequest::get(USER_PATH)
        .api_key(API_KEY_HEADER, Secret::new(api_key))
        .class(CallClass::Catalog);
    match link.send(request).await {
        Ok(_) => Ok(()),
        Err(failure) => Err(classify_with(&failure, link, None, probe_failure_detail)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::test_support::{
        API_KEY, NO_HINT_WAIT_SECS, assert_reveals_nothing, catalog_cap_text, link_to,
        over_catalog_cap, request_count, serving, throttled,
    };
    use forge_tts_core::TtsError;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn request_for(voice: &str) -> SynthesisRequest {
        SynthesisRequest {
            text: "hello".into(),
            voice_id: VoiceId(voice.into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            ssml: false,
        }
    }

    fn raw_pcm_bytes(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    fn outcome(error: &ElevenLabsError) -> String {
        match error {
            ElevenLabsError::Http(_) => "Http".into(),
            ElevenLabsError::Unauthorized(_) => "Unauthorized".into(),
            ElevenLabsError::QuotaExceeded(_) => "QuotaExceeded".into(),
            ElevenLabsError::RateLimited { retry_after_secs } => {
                format!("RateLimited({retry_after_secs})")
            }
            ElevenLabsError::VoiceNotFound(_) => "VoiceNotFound".into(),
            ElevenLabsError::Rejected(_) => "Rejected".into(),
            ElevenLabsError::Decode(_) => "Decode".into(),
            ElevenLabsError::Io(_) => "Io".into(),
        }
    }

    #[tokio::test]
    async fn synthesize_posts_to_the_voice_path_with_the_key_header_and_reads_pcm() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/text-to-speech/voice-123"))
            .and(query_param("output_format", "pcm_24000"))
            .and(header("xi-api-key", API_KEY))
            .respond_with(
                ResponseTemplate::new(200).set_body_bytes(raw_pcm_bytes(&[100, -100, 200, -200])),
            )
            .mount(&server)
            .await;

        let pcm = synthesize(&link_to(&server), API_KEY, request_for("voice-123"))
            .await
            .unwrap();

        assert_eq!(
            (pcm.sample_rate, pcm.channels, pcm.samples),
            (24_000, 1, vec![100, -100, 200, -200])
        );
    }

    #[tokio::test]
    async fn a_voice_id_that_is_not_path_safe_is_voice_not_found_without_a_request() {
        for voice in [
            "a?b",
            "a#b",
            "../../v1/user",
            "../voice-123",
            "a/b",
            "%2e%2e/user",
            "",
            ".",
            "..",
            "a.b",
            "a b",
            "a\\b",
            "é",
            "\u{0661}",
        ] {
            let server = serving(ResponseTemplate::new(200)).await;
            let result = synthesize(&link_to(&server), API_KEY, request_for(voice)).await;
            assert!(
                matches!(result, Err(ElevenLabsError::VoiceNotFound(_))),
                "{voice}: {result:?}"
            );
            assert_eq!(
                request_count(&server).await,
                0,
                "{voice} reached the network"
            );
        }
    }

    #[tokio::test]
    async fn a_voice_id_of_ascii_word_characters_reaches_its_own_path() {
        for voice in ["a", "Z", "0", "-", "_", "21m00Tcm4TlvDq8ikWAM", "a-b_C9"] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path(format!("/v1/text-to-speech/{voice}")))
                .respond_with(ResponseTemplate::new(200))
                .mount(&server)
                .await;
            let result = synthesize(&link_to(&server), API_KEY, request_for(voice)).await;
            assert!(result.is_ok(), "{voice}: {result:?}");
        }
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
                ResponseTemplate::new(403).set_body_string(echo.clone()),
                "Unauthorized".to_owned(),
            ),
            (
                ResponseTemplate::new(402).set_body_string(echo.clone()),
                "QuotaExceeded".to_owned(),
            ),
            (ResponseTemplate::new(404), "VoiceNotFound".to_owned()),
            (
                ResponseTemplate::new(429).set_body_string("slow down"),
                no_hint.clone(),
            ),
            (throttled(7), "RateLimited(7)".to_owned()),
            (
                ResponseTemplate::new(429).set_body_string(format!(
                    r#"{{"status":"quota_exceeded","key":"{API_KEY}"}}"#
                )),
                "QuotaExceeded".to_owned(),
            ),
            (
                ResponseTemplate::new(429).set_body_string("not enough characters left"),
                "QuotaExceeded".to_owned(),
            ),
            (
                ResponseTemplate::new(429).set_body_string("Monthly CHARACTERS limit reached"),
                "QuotaExceeded".to_owned(),
            ),
            (
                ResponseTemplate::new(429).set_body_string("Quota_Exceeded"),
                "QuotaExceeded".to_owned(),
            ),
            (
                ResponseTemplate::new(429).set_body_string("too_many_concurrent_requests"),
                no_hint.clone(),
            ),
            (
                ResponseTemplate::new(503)
                    .insert_header("Retry-After", "2")
                    .set_body_string("quota"),
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
            let error = synthesize(&link_to(&server), API_KEY, request_for("voice-123"))
                .await
                .unwrap_err();
            assert_eq!(outcome(&error), expected, "{error}");
            assert_reveals_nothing(&error.to_string(), &server);
            assert_reveals_nothing(&TtsError::from(error).to_string(), &server);
        }
    }

    #[tokio::test]
    async fn synthesis_reads_replies_larger_than_the_catalog_cap() {
        let body = over_catalog_cap();
        let samples = body.len() / 2;
        let server = serving(ResponseTemplate::new(200).set_body_bytes(body)).await;

        let pcm = synthesize(&link_to(&server), API_KEY, request_for("voice-123"))
            .await
            .unwrap();

        assert_eq!(pcm.samples.len(), samples);
    }

    #[tokio::test]
    async fn probe_outcomes_follow_the_user_endpoint_status() {
        let no_hint = format!("RateLimited({NO_HINT_WAIT_SECS})");
        for (template, expected) in [
            (
                ResponseTemplate::new(200).set_body_bytes(b"{}"),
                "Ok".to_owned(),
            ),
            (ResponseTemplate::new(401), "Unauthorized".to_owned()),
            (ResponseTemplate::new(429), no_hint),
            (throttled(9), "RateLimited(9)".to_owned()),
            (ResponseTemplate::new(403), "Unauthorized".to_owned()),
            (ResponseTemplate::new(500), "Http".to_owned()),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/v1/user"))
                .and(header("xi-api-key", API_KEY))
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
            matches!(&error, ElevenLabsError::Http(detail) if detail == "unexpected status 500"),
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
            matches!(&error, ElevenLabsError::Rejected(detail) if detail.contains(&catalog_cap_text())),
            "{error}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn every_gateway_failure_without_a_body_is_classified_by_the_provider_table() {
        let link = crate::network::CloudGateways::new()
            .provider("p")
            .link("https://api.example.com")
            .unwrap();
        let voice = VoiceId("v".into());

        let got: Vec<(&str, String)> = crate::network::test_support::every_failure_without_a_body()
            .iter()
            .map(|(label, failure)| {
                (
                    *label,
                    outcome(&classify_with(failure, &link, Some(&voice), failure_detail)),
                )
            })
            .collect();

        let want: Vec<(&str, String)> = [
            ("401", "Unauthorized"),
            ("403", "Unauthorized"),
            ("404", "VoiceNotFound"),
            ("429", "RateLimited(3)"),
            ("400", "Rejected"),
            ("402", "QuotaExceeded"),
            ("408", "Http"),
            ("418", "Rejected"),
            ("500", "Http"),
            ("302", "Rejected"),
            ("budget", "RateLimited(0)"),
            ("too-large", "Rejected"),
            ("cancelled", "Rejected"),
            ("address", "Rejected"),
            ("invalid", "Rejected"),
        ]
        .into_iter()
        .map(|(label, class)| (label, class.to_owned()))
        .collect();
        assert_eq!(got, want);
    }

    #[tokio::test]
    async fn probe_and_synthesis_give_the_same_class_for_auth_and_throttle_statuses() {
        let no_hint = format!("RateLimited({NO_HINT_WAIT_SECS})");
        for (status, retry, expected) in [
            (401, None, "Unauthorized".to_owned()),
            (403, None, "Unauthorized".to_owned()),
            (429, None, no_hint),
            (429, Some(9), "RateLimited(9)".to_owned()),
        ] {
            let template = match retry {
                Some(secs) => throttled(secs),
                None => ResponseTemplate::new(status),
            };
            let server = serving(template).await;
            let synthesized = synthesize(&link_to(&server), API_KEY, request_for("voice-123"))
                .await
                .unwrap_err();
            let probed = probe_connection(&link_to(&server), API_KEY)
                .await
                .unwrap_err();
            assert_eq!(
                (outcome(&synthesized), outcome(&probed)),
                (expected.clone(), expected),
                "{status} {retry:?}"
            );
        }
    }
}
