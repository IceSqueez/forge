use forge_tts_core::{EngineId, TtsVoice, VoiceGender, VoiceId};
use reqwest::Client;
use serde::Deserialize;

use crate::credentials::PollyCredentials;
use crate::polly::error::PollyError;
use crate::polly::signer;

#[derive(Deserialize)]
struct PollyVoiceList {
    #[serde(rename = "Voices")]
    voices: Vec<PollyVoice>,
}

#[derive(Deserialize)]
struct PollyVoice {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "LanguageCode")]
    language_code: String,
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
    credentials: &PollyCredentials,
    base_url: &str,
) -> Result<Vec<TtsVoice>, PollyError> {
    let url = format!("{}/v1/voices", base_url.trim_end_matches('/'));
    let signed = signer::sign("GET", &url, b"", credentials)?;

    let resp = client
        .get(&url)
        .header("x-amz-date", &signed.x_amz_date)
        .header("Authorization", &signed.authorization)
        .send()
        .await
        .map_err(|e| PollyError::Http(e.to_string()))?;

    match resp.status().as_u16() {
        403 => return Err(PollyError::Unauthorized("invalid credentials".into())),
        200..=299 => {}
        s => {
            let body = resp.text().await.unwrap_or_default();
            return Err(PollyError::Http(format!("HTTP {s}: {body}")));
        }
    }

    let list: PollyVoiceList = resp
        .json()
        .await
        .map_err(|e| PollyError::Http(e.to_string()))?;

    let engine_id = EngineId("polly".into());
    Ok(list
        .voices
        .into_iter()
        .map(|v| TtsVoice {
            id: VoiceId(v.id),
            name: v.name,
            locale: v.language_code,
            gender: gender_from_str(&v.gender),
            engine_id: engine_id.clone(),
            is_neural: true,
            sample_rate_hint: 16_000,
        })
        .collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use wiremock::matchers::{header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_creds(base_url: &str) -> PollyCredentials {
        PollyCredentials {
            access_key_id: "AKID".into(),
            secret_access_key: "secret".into(),
            region: "us-east-1".into(),
            base_url: Some(base_url.to_string()),
        }
    }

    #[tokio::test]
    async fn list_voices_parses_json() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "Voices": [
                {
                    "Id": "Joanna",
                    "Name": "Joanna",
                    "LanguageCode": "en-US",
                    "Gender": "Female"
                },
                {
                    "Id": "Matthew",
                    "Name": "Matthew",
                    "LanguageCode": "en-US",
                    "Gender": "Male"
                },
                {
                    "Id": "Oxana",
                    "Name": "Oxana",
                    "LanguageCode": "ru-RU",
                    "Gender": "Female"
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/v1/voices"))
            .and(header_exists("Authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let creds = test_creds(&server.uri());
        let voices = fetch_voices(&Client::new(), &creds, &server.uri())
            .await
            .unwrap();

        assert_eq!(voices.len(), 3);
        assert_eq!(voices[0].id, VoiceId("Joanna".into()));
        assert_eq!(voices[0].name, "Joanna");
        assert_eq!(voices[0].locale, "en-US");
        assert!(matches!(voices[0].gender, VoiceGender::Female));
        assert!(matches!(voices[1].gender, VoiceGender::Male));
        assert_eq!(voices[2].locale, "ru-RU");
        assert_eq!(voices[0].sample_rate_hint, 16_000);
        assert!(voices[0].is_neural);
    }

    #[tokio::test]
    async fn list_voices_maps_403_to_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/voices"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let creds = test_creds(&server.uri());
        let result = fetch_voices(&Client::new(), &creds, &server.uri()).await;
        assert!(matches!(result, Err(PollyError::Unauthorized(_))));
    }
}
