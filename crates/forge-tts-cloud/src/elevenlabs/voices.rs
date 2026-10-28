use std::collections::HashMap;

use forge_net::{CallClass, GatewayFailure, GatewayRequest, Secret};
use forge_speech_types::{EngineId, TtsVoice, VoiceGender, VoiceId};
use forge_tts_core::ENGINE_ID_ELEVENLABS;
use serde::Deserialize;

use crate::elevenlabs::SAMPLE_RATE_HZ;
use crate::elevenlabs::error::ElevenLabsError;
use crate::elevenlabs::synth::{API_KEY_HEADER, INVALID_API_KEY};
use crate::gateway_failure::failure_detail;
use crate::network::ProviderLink;

const VOICES_PATH: &str = "/v1/voices";

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

fn classify_error(failure: &GatewayFailure, link: &ProviderLink) -> ElevenLabsError {
    if let Some(retry_after_secs) = link.throttle_secs(failure) {
        return ElevenLabsError::RateLimited { retry_after_secs };
    }
    match failure {
        GatewayFailure::Unauthorized { .. } | GatewayFailure::Forbidden { .. } => {
            ElevenLabsError::Unauthorized(INVALID_API_KEY.into())
        }
        other => ElevenLabsError::Http(failure_detail(other)),
    }
}

pub(super) async fn fetch_voices(
    link: &ProviderLink,
    api_key: &str,
) -> Result<Vec<TtsVoice>, ElevenLabsError> {
    let request = GatewayRequest::get(VOICES_PATH)
        .api_key(API_KEY_HEADER, Secret::new(api_key))
        .class(CallClass::Catalog);
    let response = link
        .send(request)
        .await
        .map_err(|failure| classify_error(&failure, link))?;

    let list: VoiceList = response
        .json()
        .map_err(|e| ElevenLabsError::Http(e.to_string()))?;

    let engine_id = EngineId(ENGINE_ID_ELEVENLABS.into());
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
                sample_rate_hint: SAMPLE_RATE_HZ,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::test_support::{
        API_KEY, NO_HINT_WAIT_SECS, assert_reveals_nothing, catalog_cap_text, link_to,
        over_catalog_cap, serving, throttled,
    };
    use wiremock::matchers::{header, method, path};
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
            .and(header("xi-api-key", API_KEY))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let voices = fetch_voices(&link_to(&server), API_KEY).await.unwrap();

        assert_eq!(voices.len(), 3);
        assert_eq!(voices[0].id, VoiceId("abc123".into()));
        assert_eq!(voices[0].name, "Rachel");
        assert_eq!(voices[0].locale, "en");
        assert!(matches!(voices[0].gender, VoiceGender::Female));
        assert!(matches!(voices[1].gender, VoiceGender::Male));
        assert!(matches!(voices[2].gender, VoiceGender::Neutral));
        assert_eq!(voices[2].locale, "");
        assert_eq!(voices[0].sample_rate_hint, 24_000);
        assert!(voices[0].is_neural);
    }

    #[tokio::test]
    async fn catalog_failures_map_to_provider_errors_without_revealing_the_key() {
        let echo = format!("key {API_KEY} rejected");
        let cap = catalog_cap_text();
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
                ResponseTemplate::new(429),
                format!("RateLimited({NO_HINT_WAIT_SECS})"),
            ),
            (throttled(4), "RateLimited(4)".to_owned()),
            (
                ResponseTemplate::new(500).set_body_string(echo.clone()),
                "Http".to_owned(),
            ),
            (
                ResponseTemplate::new(200).set_body_string(r#"{"voices":"#),
                "Http".to_owned(),
            ),
            (
                ResponseTemplate::new(200).set_body_bytes(over_catalog_cap()),
                format!("Http({cap})"),
            ),
        ] {
            let server = serving(template).await;
            let error = fetch_voices(&link_to(&server), API_KEY).await.unwrap_err();
            let got = match &error {
                ElevenLabsError::Unauthorized(_) => "Unauthorized".to_owned(),
                ElevenLabsError::RateLimited { retry_after_secs } => {
                    format!("RateLimited({retry_after_secs})")
                }
                ElevenLabsError::Http(detail) if detail.contains(&cap) => format!("Http({cap})"),
                ElevenLabsError::Http(_) => "Http".to_owned(),
                other => format!("{other:?}"),
            };
            assert_eq!(got, expected, "{error}");
            assert_reveals_nothing(&error.to_string(), &server);
        }
    }
}
