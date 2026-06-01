use std::collections::HashMap;

use forge_tts_core::{EngineId, TtsVoice, VoiceGender, VoiceId};
use reqwest::Client;
use serde::Deserialize;

use crate::elevenlabs::error::ElevenLabsError;

#[derive(Deserialize)]
struct VoiceList {
    voices: Vec<VoiceEntry>,
}

#[derive(Deserialize)]
struct VoiceEntry {
    voice_id: String,
    name: String,
    #[serde(default)]
    labels: HashMap<String, String>,
}

fn gender_from_labels(labels: &HashMap<String, String>) -> VoiceGender {
    match labels.get("gender").map(String::as_str) {
        Some("female") => VoiceGender::Female,
        Some("male") => VoiceGender::Male,
        _ => VoiceGender::Neutral,
    }
}

pub(super) async fn fetch_voices(
    client: &Client,
    api_key: &str,
    base_url: &str,
) -> Result<Vec<TtsVoice>, ElevenLabsError> {
    let url = format!("{}/v1/voices", base_url.trim_end_matches('/'));
    let resp = client
        .get(&url)
        .header("xi-api-key", api_key)
        .send()
        .await
        .map_err(|e| ElevenLabsError::Http(e.to_string()))?;

    match resp.status().as_u16() {
        401 | 403 => return Err(ElevenLabsError::Unauthorized("invalid API key".into())),
        200..=299 => {}
        s => {
            let body = resp.text().await.unwrap_or_default();
            return Err(ElevenLabsError::Http(format!("HTTP {s}: {body}")));
        }
    }

    let list: VoiceList = resp
        .json()
        .await
        .map_err(|e| ElevenLabsError::Http(e.to_string()))?;

    let engine_id = EngineId("elevenlabs".into());
    Ok(list
        .voices
        .into_iter()
        .map(|v| {
            let locale = v.labels.get("language").cloned().unwrap_or_default();
            let gender = gender_from_labels(&v.labels);
            TtsVoice {
                id: VoiceId(v.voice_id),
                name: v.name,
                locale,
                gender,
                engine_id: engine_id.clone(),
                is_neural: true,
                sample_rate_hint: 24_000,
            }
        })
        .collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use wiremock::matchers::{header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn list_voices_parses_json() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "voices": [
                {
                    "voice_id": "abc123",
                    "name": "Rachel",
                    "labels": { "gender": "female", "language": "en" }
                },
                {
                    "voice_id": "def456",
                    "name": "Adam",
                    "labels": { "gender": "male", "language": "en" }
                },
                {
                    "voice_id": "ghi789",
                    "name": "Custom Voice",
                    "labels": {}
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/v1/voices"))
            .and(header_exists("xi-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let client = Client::new();
        let voices = fetch_voices(&client, "xi-test", &server.uri())
            .await
            .unwrap();

        assert_eq!(voices.len(), 3);
        assert_eq!(voices[0].id, VoiceId("abc123".into()));
        assert_eq!(voices[0].name, "Rachel");
        assert_eq!(voices[0].locale, "en");
        assert!(matches!(voices[0].gender, VoiceGender::Female));
        assert!(matches!(voices[1].gender, VoiceGender::Male));
        assert!(matches!(voices[2].gender, VoiceGender::Neutral));
        assert_eq!(voices[0].sample_rate_hint, 24_000);
        assert!(voices[0].is_neural);
    }

    #[tokio::test]
    async fn list_voices_maps_401_to_unauthorized() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/voices"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = Client::new();
        let result = fetch_voices(&client, "bad-key", &server.uri()).await;
        assert!(matches!(result, Err(ElevenLabsError::Unauthorized(_))));
    }
}
