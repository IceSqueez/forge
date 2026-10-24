use forge_net::{CallClass, GatewayFailure, GatewayRequest, Secret};
use forge_tts_core::{ENGINE_ID_AZURE, EngineId, TtsVoice, VoiceGender, VoiceId};
use serde::Deserialize;

use crate::azure::SAMPLE_RATE_HZ;
use crate::azure::error::AzureError;
use crate::azure::synth::{INVALID_API_KEY, SUBSCRIPTION_KEY_HEADER, VOICES_LIST_PATH};
use crate::gateway_failure::failure_detail;
use crate::network::ProviderLink;

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

fn classify_error(failure: &GatewayFailure, link: &ProviderLink) -> AzureError {
    if let Some(retry_after_secs) = link.throttle_secs(failure) {
        return AzureError::RateLimited { retry_after_secs };
    }
    match failure {
        GatewayFailure::Unauthorized { .. } | GatewayFailure::Forbidden { .. } => {
            AzureError::Unauthorized(INVALID_API_KEY.into())
        }
        other => AzureError::Http(failure_detail(other)),
    }
}

pub(super) async fn fetch_voices(
    link: &ProviderLink,
    api_key: &str,
) -> Result<Vec<TtsVoice>, AzureError> {
    let request = GatewayRequest::get(VOICES_LIST_PATH)
        .api_key(SUBSCRIPTION_KEY_HEADER, Secret::new(api_key))
        .class(CallClass::Catalog);
    let response = link
        .send(request)
        .await
        .map_err(|failure| classify_error(&failure, link))?;

    let raw: Vec<AzureVoice> = response
        .json()
        .map_err(|e| AzureError::Decode(e.to_string()))?;

    let engine_id = EngineId(ENGINE_ID_AZURE.into());
    Ok(raw
        .into_iter()
        .map(|v| TtsVoice {
            id: VoiceId(v.short_name),
            name: v.display_name,
            locale: v.locale,
            gender: gender_from_str(&v.gender),
            engine_id: engine_id.clone(),
            is_neural: true,
            sample_rate_hint: SAMPLE_RATE_HZ,
        })
        .collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
            .and(header("Ocp-Apim-Subscription-Key", API_KEY))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let voices = fetch_voices(&link_to(&server), API_KEY).await.unwrap();

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
                ResponseTemplate::new(200).set_body_string("[{"),
                "Decode".to_owned(),
            ),
            (
                ResponseTemplate::new(200).set_body_bytes(over_catalog_cap()),
                format!("Http({cap})"),
            ),
        ] {
            let server = serving(template).await;
            let error = fetch_voices(&link_to(&server), API_KEY).await.unwrap_err();
            let got = match &error {
                AzureError::Unauthorized(_) => "Unauthorized".to_owned(),
                AzureError::RateLimited { retry_after_secs } => {
                    format!("RateLimited({retry_after_secs})")
                }
                AzureError::Http(detail) if detail.contains(&cap) => format!("Http({cap})"),
                AzureError::Http(_) => "Http".to_owned(),
                AzureError::Decode(_) => "Decode".to_owned(),
                other => format!("{other:?}"),
            };
            assert_eq!(got, expected, "{error}");
            assert_reveals_nothing(&error.to_string(), &server);
        }
    }
}
