use forge_audio::PcmBuffer;
use forge_tts_core::{SynthesisRequest, VoiceId};
use reqwest::Client;
use serde_json::json;

use crate::openai::error::OpenAiError;

const VALID_VOICES: &[&str] = &["alloy", "echo", "fable", "onyx", "nova", "shimmer"];

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

fn classify_error(status: u16, retry_after_secs: u64, body: &str) -> OpenAiError {
    match status {
        401 => OpenAiError::Unauthorized("invalid API key".into()),
        402 => OpenAiError::QuotaExceeded(body.to_string()),
        429 => {
            if body.contains("insufficient_quota") || body.contains("billing") {
                OpenAiError::QuotaExceeded(body.to_string())
            } else {
                OpenAiError::RateLimited { retry_after_secs }
            }
        }
        _ => OpenAiError::Http(format!("HTTP {status}: {body}")),
    }
}

pub(super) async fn synthesize(
    client: &Client,
    api_key: &str,
    base_url: &str,
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

    let body = json!({
        "model": "tts-1",
        "voice": voice,
        "input": req.text,
        "response_format": "wav",
        "speed": speed,
    });

    let url = format!("{}/v1/audio/speech", base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| OpenAiError::Http(e.to_string()))?;

    let status = resp.status().as_u16();

    if (200..300).contains(&(status as usize)) {
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| OpenAiError::Http(e.to_string()))?;
        forge_audio::decode_bytes(&bytes, Some("wav"))
            .map_err(|e| OpenAiError::Decode(e.to_string()))
    } else {
        let retry_after_secs = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let body_text = resp.text().await.unwrap_or_default();
        Err(classify_error(status, retry_after_secs, &body_text))
    }
}

pub(super) async fn probe_connection(
    client: &Client,
    api_key: &str,
    base_url: &str,
) -> Result<(), OpenAiError> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let resp = client
        .get(&url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|e| OpenAiError::Http(e.to_string()))?;

    match resp.status().as_u16() {
        200..=299 => Ok(()),
        401 => Err(OpenAiError::Unauthorized("invalid API key".into())),
        s => Err(OpenAiError::Http(format!("unexpected status {s}"))),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use wiremock::matchers::{header_exists, method, path};
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

    fn test_request() -> SynthesisRequest {
        SynthesisRequest {
            text: "hello".into(),
            voice_id: VoiceId("alloy".into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            ssml: false,
        }
    }

    #[tokio::test]
    async fn synthesize_returns_pcm_for_valid_wav_response() {
        let server = MockServer::start().await;
        let wav = silent_wav(24_000, 2400);

        Mock::given(method("POST"))
            .and(path("/v1/audio/speech"))
            .and(header_exists("authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(wav))
            .mount(&server)
            .await;

        let client = Client::new();
        let result = synthesize(&client, "sk-test", &server.uri(), test_request()).await;

        let pcm = result.unwrap();
        assert_eq!(pcm.sample_rate, 24_000);
        assert_eq!(pcm.channels, 1);
    }

    #[tokio::test]
    async fn synthesize_maps_401_to_unauthorized() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/audio/speech"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = Client::new();
        let result = synthesize(&client, "bad-key", &server.uri(), test_request()).await;

        assert!(matches!(result, Err(OpenAiError::Unauthorized(_))));
    }

    #[tokio::test]
    async fn synthesize_maps_unknown_voice_to_voice_not_found() {
        let client = Client::new();
        let req = SynthesisRequest {
            text: "hello".into(),
            voice_id: VoiceId("unknown-voice".into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            ssml: false,
        };
        let result = synthesize(&client, "sk-test", "http://localhost:0", req).await;
        assert!(matches!(result, Err(OpenAiError::VoiceNotFound(_))));
    }

    #[tokio::test]
    async fn probe_connection_returns_ok_on_200() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header_exists("authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"{}"))
            .mount(&server)
            .await;

        let client = Client::new();
        let result = probe_connection(&client, "sk-test", &server.uri()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn probe_connection_returns_unauthorized_on_401() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = Client::new();
        let result = probe_connection(&client, "bad-key", &server.uri()).await;
        assert!(matches!(result, Err(OpenAiError::Unauthorized(_))));
    }
}
