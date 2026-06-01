use forge_audio::PcmBuffer;
use forge_tts_core::{SynthesisRequest, VoiceId};
use reqwest::Client;
use serde_json::json;

use crate::credentials::PollyCredentials;
use crate::polly::error::PollyError;
use crate::polly::signer;

fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            c => out.push(c),
        }
    }
    out
}

fn format_signed_percent(val: i32) -> String {
    if val >= 0 {
        format!("+{val}%")
    } else {
        format!("{val}%")
    }
}

pub(super) fn build_polly_body(req: &SynthesisRequest) -> (String, bool) {
    if req.ssml {
        let body = json!({
            "Text": req.text,
            "TextType": "ssml",
            "VoiceId": req.voice_id.0,
            "OutputFormat": "pcm",
            "SampleRate": "16000",
            "Engine": "neural"
        });
        return (body.to_string(), true);
    }

    let rate_pct = ((req.rate_multiplier - 1.0) * 100.0).clamp(-80.0, 200.0);
    let pitch_pct = ((2f32.powf(req.pitch_semitones / 12.0) - 1.0) * 100.0).clamp(-33.3, 50.0);
    let rate_rounded = rate_pct.round() as i32;
    let pitch_rounded = pitch_pct.round() as i32;

    if rate_rounded != 0 || pitch_rounded != 0 {
        let escaped = xml_escape(&req.text);
        let mut attrs = String::new();
        if rate_rounded != 0 {
            attrs.push_str(&format!(
                " rate=\"{}\"",
                format_signed_percent(rate_rounded)
            ));
        }
        if pitch_rounded != 0 {
            attrs.push_str(&format!(
                " pitch=\"{}\"",
                format_signed_percent(pitch_rounded)
            ));
        }
        let text = format!("<speak><prosody{attrs}>{escaped}</prosody></speak>");
        let body = json!({
            "Text": text,
            "TextType": "ssml",
            "VoiceId": req.voice_id.0,
            "OutputFormat": "pcm",
            "SampleRate": "16000",
            "Engine": "neural"
        });
        (body.to_string(), true)
    } else {
        let body = json!({
            "Text": req.text,
            "TextType": "text",
            "VoiceId": req.voice_id.0,
            "OutputFormat": "pcm",
            "SampleRate": "16000",
            "Engine": "neural"
        });
        (body.to_string(), false)
    }
}

fn classify_error(
    status: u16,
    retry_after_secs: u64,
    body: &str,
    voice_id: &VoiceId,
) -> PollyError {
    match status {
        // Polly returns 403 (not 401) for invalid credentials.
        403 => PollyError::Unauthorized("invalid credentials".into()),
        400 if body.to_ascii_lowercase().contains("voice") => {
            PollyError::VoiceNotFound(voice_id.0.clone())
        }
        429 if body.contains("quota") || body.contains("characters") => {
            PollyError::QuotaExceeded(body.to_string())
        }
        429 => PollyError::RateLimited { retry_after_secs },
        _ => PollyError::Http(format!("HTTP {status}: {body}")),
    }
}

pub(super) async fn synthesize(
    client: &Client,
    credentials: &PollyCredentials,
    base_url: &str,
    req: SynthesisRequest,
) -> Result<PcmBuffer, PollyError> {
    let url = format!("{}/v1/speech", base_url.trim_end_matches('/'));
    let (body_str, _is_ssml) = build_polly_body(&req);
    let body_bytes = body_str.as_bytes();

    let signed = signer::sign("POST", &url, body_bytes, credentials)?;

    let resp = client
        .post(&url)
        .header("content-type", "application/json")
        .header("x-amz-date", &signed.x_amz_date)
        .header("Authorization", &signed.authorization)
        .body(body_str)
        .send()
        .await
        .map_err(|e| PollyError::Http(e.to_string()))?;

    let status = resp.status();
    let status_code = status.as_u16();

    if status.is_success() {
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| PollyError::Http(e.to_string()))?;
        let samples: Vec<i16> = bytes
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        Ok(PcmBuffer::new(samples, 16_000, 1))
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
    credentials: &PollyCredentials,
    base_url: &str,
) -> Result<(), PollyError> {
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
        200..=299 => Ok(()),
        403 => Err(PollyError::Unauthorized("invalid credentials".into())),
        s => Err(PollyError::Http(format!("unexpected status {s}"))),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_tts_core::VoiceId;
    use wiremock::matchers::{header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn plain_req(text: &str) -> SynthesisRequest {
        SynthesisRequest {
            text: text.into(),
            voice_id: VoiceId("Joanna".into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            ssml: false,
        }
    }

    fn raw_pcm_bytes(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    fn test_creds(base_url: &str) -> PollyCredentials {
        PollyCredentials {
            access_key_id: "AKID".into(),
            secret_access_key: "secret".into(),
            region: "us-east-1".into(),
            base_url: Some(base_url.to_string()),
        }
    }

    #[test]
    fn synthesize_uses_text_type_when_plain_default() {
        let req = plain_req("hello");
        let (body, is_ssml) = build_polly_body(&req);
        assert!(!is_ssml);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["TextType"], "text");
        assert_eq!(parsed["Text"], "hello");
    }

    #[test]
    fn synthesize_passes_ssml_input_through() {
        let input = "<speak>hello</speak>";
        let req = SynthesisRequest {
            text: input.into(),
            voice_id: VoiceId("Joanna".into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            ssml: true,
        };
        let (body, is_ssml) = build_polly_body(&req);
        assert!(is_ssml);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["TextType"], "ssml");
        assert_eq!(parsed["Text"], input);
    }

    #[test]
    fn synthesize_wraps_plain_text_in_ssml_when_prosody_differs() {
        let req = SynthesisRequest {
            text: "hello".into(),
            voice_id: VoiceId("Joanna".into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.5,
            ssml: false,
        };
        let (body, is_ssml) = build_polly_body(&req);
        assert!(is_ssml);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["TextType"], "ssml");
        let text = parsed["Text"].as_str().unwrap();
        assert!(text.contains("<prosody"));
        assert!(text.contains("+50%"));
    }

    #[test]
    fn synthesize_wraps_when_only_pitch_differs() {
        let req = SynthesisRequest {
            text: "test".into(),
            voice_id: VoiceId("Joanna".into()),
            pitch_semitones: 2.0,
            rate_multiplier: 1.0,
            ssml: false,
        };
        let (body, is_ssml) = build_polly_body(&req);
        assert!(is_ssml);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["TextType"], "ssml");
    }

    #[tokio::test]
    async fn synthesize_returns_pcm_for_valid_response() {
        let server = MockServer::start().await;
        let pcm_bytes = raw_pcm_bytes(&[100i16, -100i16, 200i16, -200i16]);

        Mock::given(method("POST"))
            .and(path("/v1/speech"))
            .and(header_exists("Authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(pcm_bytes))
            .mount(&server)
            .await;

        let creds = test_creds(&server.uri());
        let result = synthesize(&Client::new(), &creds, &server.uri(), plain_req("hello")).await;

        let pcm = result.unwrap();
        assert_eq!(pcm.sample_rate, 16_000);
        assert_eq!(pcm.channels, 1);
        assert_eq!(pcm.samples, vec![100i16, -100, 200, -200]);
    }

    #[tokio::test]
    async fn synthesize_maps_403_to_auth_failed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/speech"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let creds = test_creds(&server.uri());
        let result = synthesize(&Client::new(), &creds, &server.uri(), plain_req("hello")).await;
        assert!(matches!(result, Err(PollyError::Unauthorized(_))));
    }

    #[tokio::test]
    async fn synthesize_includes_authorization_header() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/speech"))
            .and(header_exists("Authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(raw_pcm_bytes(&[0i16, 0i16])))
            .mount(&server)
            .await;

        let creds = test_creds(&server.uri());
        let result = synthesize(&Client::new(), &creds, &server.uri(), plain_req("test")).await;
        assert!(result.is_ok(), "expected Ok but got: {result:?}");

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let auth = requests[0]
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            auth.starts_with("AWS4-HMAC-SHA256 "),
            "unexpected Authorization header: {auth}"
        );
    }

    #[tokio::test]
    async fn probe_connection_returns_ok_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/voices"))
            .and(header_exists("Authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"{\"Voices\":[]}"))
            .mount(&server)
            .await;

        let creds = test_creds(&server.uri());
        let result = probe_connection(&Client::new(), &creds, &server.uri()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn probe_connection_auth_failed_on_403() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/voices"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let creds = test_creds(&server.uri());
        let result = probe_connection(&Client::new(), &creds, &server.uri()).await;
        assert!(matches!(result, Err(PollyError::Unauthorized(_))));
    }
}
