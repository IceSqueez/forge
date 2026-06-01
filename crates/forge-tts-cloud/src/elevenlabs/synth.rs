use forge_audio::PcmBuffer;
use forge_tts_core::{SynthesisRequest, VoiceId};
use reqwest::Client;
use serde_json::json;

use crate::elevenlabs::error::ElevenLabsError;

fn classify_error(
    status: u16,
    retry_after_secs: u64,
    body: &str,
    voice_id: &VoiceId,
) -> ElevenLabsError {
    match status {
        401 | 403 => ElevenLabsError::Unauthorized("invalid API key".into()),
        402 => ElevenLabsError::QuotaExceeded(body.to_string()),
        404 => ElevenLabsError::VoiceNotFound(voice_id.clone()),
        429 => {
            if body.contains("quota") || body.contains("characters") {
                ElevenLabsError::QuotaExceeded(body.to_string())
            } else {
                ElevenLabsError::RateLimited { retry_after_secs }
            }
        }
        _ => ElevenLabsError::Http(format!("HTTP {status}: {body}")),
    }
}

pub(super) async fn synthesize(
    client: &Client,
    api_key: &str,
    base_url: &str,
    req: SynthesisRequest,
) -> Result<PcmBuffer, ElevenLabsError> {
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

    let body = json!({
        "text": req.text,
        "model_id": "eleven_multilingual_v2",
        "voice_settings": {
            "stability": 0.5,
            "similarity_boost": 0.75
        }
    });

    let url = format!(
        "{}/v1/text-to-speech/{}?output_format=pcm_24000",
        base_url.trim_end_matches('/'),
        req.voice_id.0
    );

    let resp = client
        .post(&url)
        .header("xi-api-key", api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| ElevenLabsError::Http(e.to_string()))?;

    let status = resp.status();
    let status_code = status.as_u16();

    if status.is_success() {
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ElevenLabsError::Http(e.to_string()))?;
        let samples: Vec<i16> = bytes
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        Ok(PcmBuffer::new(samples, 24_000, 1))
    } else {
        let retry_after_secs = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let body_text = resp.text().await.unwrap_or_default();
        Err(classify_error(
            status_code,
            retry_after_secs,
            &body_text,
            &req.voice_id,
        ))
    }
}

pub(super) async fn probe_connection(
    client: &Client,
    api_key: &str,
    base_url: &str,
) -> Result<(), ElevenLabsError> {
    let url = format!("{}/v1/user", base_url.trim_end_matches('/'));
    let resp = client
        .get(&url)
        .header("xi-api-key", api_key)
        .send()
        .await
        .map_err(|e| ElevenLabsError::Http(e.to_string()))?;

    match resp.status().as_u16() {
        200..=299 => Ok(()),
        401 => Err(ElevenLabsError::Unauthorized("invalid API key".into())),
        s => Err(ElevenLabsError::Http(format!("unexpected status {s}"))),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_tts_core::VoiceId;
    use wiremock::matchers::{header_exists, method, path, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_request() -> SynthesisRequest {
        SynthesisRequest {
            text: "hello".into(),
            voice_id: VoiceId("test-voice-id".into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            ssml: false,
        }
    }

    fn raw_pcm_bytes(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    #[tokio::test]
    async fn synthesize_returns_pcm_for_valid_response() {
        let server = MockServer::start().await;
        let pcm_bytes = raw_pcm_bytes(&[100i16, -100i16, 200i16, -200i16]);

        Mock::given(method("POST"))
            .and(path_regex(r"/v1/text-to-speech/.*"))
            .and(header_exists("xi-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(pcm_bytes))
            .mount(&server)
            .await;

        let client = Client::new();
        let result = synthesize(&client, "xi-test", &server.uri(), test_request()).await;

        let pcm = result.unwrap();
        assert_eq!(pcm.sample_rate, 24_000);
        assert_eq!(pcm.channels, 1);
        assert_eq!(pcm.samples, vec![100i16, -100, 200, -200]);
    }

    #[tokio::test]
    async fn synthesize_maps_401_to_auth_failed() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex(r"/v1/text-to-speech/.*"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = Client::new();
        let result = synthesize(&client, "bad-key", &server.uri(), test_request()).await;
        assert!(matches!(result, Err(ElevenLabsError::Unauthorized(_))));
    }

    #[tokio::test]
    async fn synthesize_maps_unknown_voice_to_voice_not_found() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/text-to-speech/test-voice-id"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = Client::new();
        let result = synthesize(&client, "xi-test", &server.uri(), test_request()).await;
        assert!(matches!(result, Err(ElevenLabsError::VoiceNotFound(_))));
    }

    #[tokio::test]
    async fn probe_connection_returns_ok_on_200() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/user"))
            .and(header_exists("xi-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"{}"))
            .mount(&server)
            .await;

        let client = Client::new();
        let result = probe_connection(&client, "xi-test", &server.uri()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn probe_connection_returns_auth_failed_on_401() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/user"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = Client::new();
        let result = probe_connection(&client, "bad-key", &server.uri()).await;
        assert!(matches!(result, Err(ElevenLabsError::Unauthorized(_))));
    }
}
