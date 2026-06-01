use forge_tts_core::{EngineId, TtsVoice, VoiceGender, VoiceId};
use reqwest::Client;
use serde::Deserialize;

use crate::azure::error::AzureError;

#[derive(Deserialize)]
struct AzureVoice {
    #[serde(rename = "ShortName")]
    short_name: String,
    #[serde(rename = "DisplayName")]
    display_name: String,
    #[serde(rename = "Locale")]
    locale: String,
    #[serde(rename = "Gender")]
    gender: String,
}

fn gender_from_str(s: &str) -> VoiceGender {
    match s {
        "Female" => VoiceGender::Female,
        "Male" => VoiceGender::Male,
        _ => VoiceGender::Neutral,
    }
}

pub(super) async fn fetch_voices(
    client: &Client,
    api_key: &str,
    base_url: &str,
) -> Result<Vec<TtsVoice>, AzureError> {
    let url = format!(
        "{}/cognitiveservices/voices/list",
        base_url.trim_end_matches('/')
    );
    let resp = client
        .get(&url)
        .header("Ocp-Apim-Subscription-Key", api_key)
        .send()
        .await
        .map_err(|e| AzureError::Http(e.to_string()))?;

    match resp.status().as_u16() {
        401 | 403 => return Err(AzureError::Unauthorized("invalid API key".into())),
        200..=299 => {}
        s => {
            let body = resp.text().await.unwrap_or_default();
            return Err(AzureError::Http(format!("HTTP {s}: {body}")));
        }
    }

    let raw: Vec<AzureVoice> = resp
        .json()
        .await
        .map_err(|e| AzureError::Decode(e.to_string()))?;

    let engine_id = EngineId("azure".into());
    Ok(raw
        .into_iter()
        .map(|v| TtsVoice {
            id: VoiceId(v.short_name),
            name: v.display_name,
            locale: v.locale,
            gender: gender_from_str(&v.gender),
            engine_id: engine_id.clone(),
            is_neural: true,
            sample_rate_hint: 24_000,
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
        let body = serde_json::json!([
            {
                "ShortName": "en-US-AriaNeural",
                "DisplayName": "Aria",
                "Locale": "en-US",
                "Gender": "Female"
            },
            {
                "ShortName": "en-US-GuyNeural",
                "DisplayName": "Guy",
                "Locale": "en-US",
                "Gender": "Male"
            },
            {
                "ShortName": "uk-UA-OstapNeural",
                "DisplayName": "Ostap",
                "Locale": "uk-UA",
                "Gender": "Male"
            }
        ]);

        Mock::given(method("GET"))
            .and(path("/cognitiveservices/voices/list"))
            .and(header_exists("Ocp-Apim-Subscription-Key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let client = Client::new();
        let voices = fetch_voices(&client, "test-key", &server.uri())
            .await
            .unwrap();

        assert_eq!(voices.len(), 3);
        assert_eq!(voices[0].id, VoiceId("en-US-AriaNeural".into()));
        assert_eq!(voices[0].name, "Aria");
        assert_eq!(voices[0].locale, "en-US");
        assert!(matches!(voices[0].gender, VoiceGender::Female));
        assert!(matches!(voices[1].gender, VoiceGender::Male));
        assert_eq!(voices[2].locale, "uk-UA");
        assert_eq!(voices[0].sample_rate_hint, 24_000);
        assert!(voices[0].is_neural);
    }

    #[tokio::test]
    async fn list_voices_maps_401_to_unauthorized() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/cognitiveservices/voices/list"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = Client::new();
        let result = fetch_voices(&client, "bad-key", &server.uri()).await;
        assert!(matches!(result, Err(AzureError::Unauthorized(_))));
    }
}
