use forge_audio::PcmBuffer;
use forge_tts_core::{SynthesisRequest, VoiceId};
use reqwest::Client;

use crate::azure::error::AzureError;

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

fn build_ssml(req: &SynthesisRequest) -> String {
    if req.ssml {
        return req.text.clone();
    }

    let escaped = xml_escape(&req.text);

    let rate_pct = ((req.rate_multiplier - 1.0) * 100.0).clamp(-50.0, 200.0);
    let pitch_pct = ((2f32.powf(req.pitch_semitones / 12.0) - 1.0) * 100.0).clamp(-50.0, 50.0);
    let rate_rounded = rate_pct.round() as i32;
    let pitch_rounded = pitch_pct.round() as i32;

    let inner = if rate_rounded == 0 && pitch_rounded == 0 {
        escaped
    } else {
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
        format!("<prosody{attrs}>{escaped}</prosody>")
    };

    format!(
        r#"<speak version="1.0" xml:lang="en-US"><voice name="{}">{inner}</voice></speak>"#,
        req.voice_id.0
    )
}

fn classify_error(
    status: u16,
    retry_after_secs: u64,
    body: &str,
    voice_id: &VoiceId,
) -> AzureError {
    match status {
        401 | 403 => AzureError::Unauthorized("invalid API key".into()),
        400 if body.to_ascii_lowercase().contains("voice") => {
            AzureError::VoiceNotFound(voice_id.clone())
        }
        429 if body.contains("quota") || body.contains("characters") => {
            AzureError::QuotaExceeded(body.to_string())
        }
        429 => AzureError::RateLimited { retry_after_secs },
        _ => AzureError::Http(format!("HTTP {status}: {body}")),
    }
}

pub(super) async fn synthesize(
    client: &Client,
    api_key: &str,
    base_url: &str,
    req: SynthesisRequest,
) -> Result<PcmBuffer, AzureError> {
    let url = format!("{}/cognitiveservices/v1", base_url.trim_end_matches('/'));
    let body = build_ssml(&req);

    let resp = client
        .post(&url)
        .header("Ocp-Apim-Subscription-Key", api_key)
        .header("Content-Type", "application/ssml+xml")
        .header("X-Microsoft-OutputFormat", "raw-24khz-16bit-mono-pcm")
        // Azure rejects requests without User-Agent header.
        .header("User-Agent", "forge-tts")
        .body(body)
        .send()
        .await
        .map_err(|e| AzureError::Http(e.to_string()))?;

    let status = resp.status();
    let status_code = status.as_u16();

    if status.is_success() {
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AzureError::Http(e.to_string()))?;
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
) -> Result<(), AzureError> {
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
        200..=299 => Ok(()),
        401 | 403 => Err(AzureError::Unauthorized("invalid API key".into())),
        s => Err(AzureError::Http(format!("unexpected status {s}"))),
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
            voice_id: VoiceId("en-US-AriaNeural".into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            ssml: false,
        }
    }

    fn raw_pcm_bytes(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    #[test]
    fn synthesize_wraps_plain_text_in_ssml() {
        let req = plain_req("hello");
        let ssml = build_ssml(&req);
        assert!(ssml.contains("<speak"));
        assert!(ssml.contains(r#"<voice name="en-US-AriaNeural">hello</voice>"#));
    }

    #[test]
    fn synthesize_passes_ssml_input_through_unchanged() {
        let input = r#"<speak version="1.0"><voice name="Aria">test</voice></speak>"#;
        let req = SynthesisRequest {
            text: input.into(),
            voice_id: VoiceId("en-US-AriaNeural".into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            ssml: true,
        };
        assert_eq!(build_ssml(&req), input);
    }

    #[test]
    fn synthesize_wraps_prosody_when_rate_differs() {
        let req = SynthesisRequest {
            text: "hello".into(),
            voice_id: VoiceId("en-US-AriaNeural".into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.5,
            ssml: false,
        };
        let ssml = build_ssml(&req);
        assert!(ssml.contains("<prosody rate="));
        assert!(ssml.contains("+50%"));
    }

    #[test]
    fn synthesize_escapes_xml_specials() {
        let req = plain_req("<&>");
        let ssml = build_ssml(&req);
        assert!(ssml.contains("&lt;&amp;&gt;"));
        assert!(!ssml.contains("<&>"));
    }

    #[tokio::test]
    async fn synthesize_returns_pcm_for_valid_response() {
        let server = MockServer::start().await;
        let pcm_bytes = raw_pcm_bytes(&[100i16, -100i16, 200i16, -200i16]);

        Mock::given(method("POST"))
            .and(path("/cognitiveservices/v1"))
            .and(header_exists("Ocp-Apim-Subscription-Key"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(pcm_bytes))
            .mount(&server)
            .await;

        let client = Client::new();
        let result = synthesize(&client, "test-key", &server.uri(), plain_req("hello")).await;

        let pcm = result.unwrap();
        assert_eq!(pcm.sample_rate, 24_000);
        assert_eq!(pcm.channels, 1);
        assert_eq!(pcm.samples, vec![100i16, -100, 200, -200]);
    }

    #[tokio::test]
    async fn synthesize_maps_401_to_auth_failed() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/cognitiveservices/v1"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = Client::new();
        let result = synthesize(&client, "bad-key", &server.uri(), plain_req("hello")).await;
        assert!(matches!(result, Err(AzureError::Unauthorized(_))));
    }

    #[tokio::test]
    async fn probe_connection_returns_ok_on_200() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/cognitiveservices/voices/list"))
            .and(header_exists("Ocp-Apim-Subscription-Key"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"[]"))
            .mount(&server)
            .await;

        let client = Client::new();
        let result = probe_connection(&client, "test-key", &server.uri()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn probe_connection_returns_auth_failed_on_401() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/cognitiveservices/voices/list"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = Client::new();
        let result = probe_connection(&client, "bad-key", &server.uri()).await;
        assert!(matches!(result, Err(AzureError::Unauthorized(_))));
    }
}
