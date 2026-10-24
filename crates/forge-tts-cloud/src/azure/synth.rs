use forge_audio::PcmBuffer;
use forge_net::{CallClass, GatewayFailure, GatewayRequest, RequestBody, Secret};
use forge_tts_core::{SynthesisRequest, VoiceId, xml_escape};

use crate::azure::SAMPLE_RATE_HZ;
use crate::azure::error::AzureError;
use crate::gateway_failure::{
    failure_detail, probe_failure_detail, quota_excerpt, rejected_excerpt,
};
use crate::network::ProviderLink;

pub(crate) const SUBSCRIPTION_KEY_HEADER: &str = "Ocp-Apim-Subscription-Key";
pub(crate) const VOICES_LIST_PATH: &str = "/cognitiveservices/voices/list";
pub(crate) const INVALID_API_KEY: &str = "invalid API key";
const BAD_REQUEST: u16 = 400;
const QUOTA_MARKERS: &[&str] = &["quota", "characters"];
const SYNTHESIS_PATH: &str = "/cognitiveservices/v1";
const SSML_CONTENT_TYPE: &str = "application/ssml+xml";
const OUTPUT_FORMAT_HEADER: &str = "X-Microsoft-OutputFormat";
const OUTPUT_FORMAT: &str = "raw-24khz-16bit-mono-pcm";

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
        xml_escape(&req.voice_id.0)
    )
}

fn classify_error(failure: &GatewayFailure, link: &ProviderLink, voice_id: &VoiceId) -> AzureError {
    if let Some(excerpt) = quota_excerpt(failure, QUOTA_MARKERS) {
        return AzureError::QuotaExceeded(excerpt.to_owned());
    }
    if let Some(retry_after_secs) = link.throttle_secs(failure) {
        return AzureError::RateLimited { retry_after_secs };
    }
    if rejected_excerpt(failure, BAD_REQUEST)
        .is_some_and(|excerpt| excerpt.to_ascii_lowercase().contains("voice"))
    {
        return AzureError::VoiceNotFound(voice_id.clone());
    }
    match failure {
        GatewayFailure::Unauthorized { .. } | GatewayFailure::Forbidden { .. } => {
            AzureError::Unauthorized(INVALID_API_KEY.into())
        }
        other => AzureError::Http(failure_detail(other)),
    }
}

pub(super) async fn synthesize(
    link: &ProviderLink,
    api_key: &str,
    req: SynthesisRequest,
) -> Result<PcmBuffer, AzureError> {
    let request = GatewayRequest::post(SYNTHESIS_PATH)
        .api_key(SUBSCRIPTION_KEY_HEADER, Secret::new(api_key))
        .header(OUTPUT_FORMAT_HEADER, OUTPUT_FORMAT)
        .body(RequestBody::raw(SSML_CONTENT_TYPE, build_ssml(&req)));
    let response = link
        .send(request)
        .await
        .map_err(|failure| classify_error(&failure, link, &req.voice_id))?;

    let (pairs, _) = response.body().as_chunks::<2>();
    let samples: Vec<i16> = pairs.iter().map(|pair| i16::from_le_bytes(*pair)).collect();
    Ok(PcmBuffer::new(samples, SAMPLE_RATE_HZ, 1))
}

pub(super) async fn probe_connection(link: &ProviderLink, api_key: &str) -> Result<(), AzureError> {
    let request = GatewayRequest::get(VOICES_LIST_PATH)
        .api_key(SUBSCRIPTION_KEY_HEADER, Secret::new(api_key))
        .class(CallClass::Catalog);
    match link.send(request).await {
        Ok(_) => Ok(()),
        Err(GatewayFailure::Unauthorized { .. } | GatewayFailure::Forbidden { .. }) => {
            Err(AzureError::Unauthorized(INVALID_API_KEY.into()))
        }
        Err(failure) => match link.throttle_secs(&failure) {
            Some(retry_after_secs) => Err(AzureError::RateLimited { retry_after_secs }),
            None => Err(AzureError::Http(probe_failure_detail(&failure))),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::network::test_support::{
        API_KEY, NO_HINT_WAIT_SECS, assert_reveals_nothing, catalog_cap_text, link_to,
        over_catalog_cap, serving, throttled,
    };
    use forge_tts_core::TtsError;
    use wiremock::matchers::{body_string, header, method, path};
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
    fn synthesize_escapes_xml_specials_in_text_and_voice_name() {
        let req = SynthesisRequest {
            voice_id: VoiceId(r#"en-US-AriaNeural"><audio src="x"#.into()),
            ..plain_req(r#"<&> "it's""#)
        };
        assert_eq!(
            build_ssml(&req),
            r#"<speak version="1.0" xml:lang="en-US"><voice name="en-US-AriaNeural&quot;&gt;&lt;audio src=&quot;x">&lt;&amp;&gt; &quot;it&apos;s&quot;</voice></speak>"#
        );
    }

    fn outcome(error: &AzureError) -> String {
        match error {
            AzureError::Http(_) => "Http".into(),
            AzureError::Unauthorized(_) => "Unauthorized".into(),
            AzureError::QuotaExceeded(_) => "QuotaExceeded".into(),
            AzureError::RateLimited { retry_after_secs } => {
                format!("RateLimited({retry_after_secs})")
            }
            AzureError::VoiceNotFound(_) => "VoiceNotFound".into(),
            AzureError::Decode(_) => "Decode".into(),
            AzureError::Io(_) => "Io".into(),
        }
    }

    #[tokio::test]
    async fn synthesize_posts_ssml_with_the_key_header_and_reads_pcm() {
        let request = plain_req("hello");
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/cognitiveservices/v1"))
            .and(header("Ocp-Apim-Subscription-Key", API_KEY))
            .and(header(
                "X-Microsoft-OutputFormat",
                "raw-24khz-16bit-mono-pcm",
            ))
            .and(header("content-type", "application/ssml+xml"))
            .and(body_string(build_ssml(&request)))
            .respond_with(
                ResponseTemplate::new(200).set_body_bytes(raw_pcm_bytes(&[100, -100, 200, -200])),
            )
            .mount(&server)
            .await;

        let pcm = synthesize(&link_to(&server), API_KEY, request)
            .await
            .unwrap();

        assert_eq!(
            (pcm.sample_rate, pcm.channels, pcm.samples),
            (24_000, 1, vec![100, -100, 200, -200])
        );
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
                ResponseTemplate::new(400).set_body_string("Unsupported Voice name"),
                "VoiceNotFound".to_owned(),
            ),
            (
                ResponseTemplate::new(400).set_body_string(echo.clone()),
                "Http".to_owned(),
            ),
            (
                ResponseTemplate::new(429).set_body_string("slow down"),
                no_hint.clone(),
            ),
            (throttled(7), "RateLimited(7)".to_owned()),
            (
                ResponseTemplate::new(429).set_body_string(format!("quota exceeded for {API_KEY}")),
                "QuotaExceeded".to_owned(),
            ),
            (
                ResponseTemplate::new(429).set_body_string("monthly characters used up"),
                "QuotaExceeded".to_owned(),
            ),
            (
                ResponseTemplate::new(429).set_body_string("Quota exceeded"),
                "QuotaExceeded".to_owned(),
            ),
            (
                ResponseTemplate::new(429).set_body_string("CHARACTERS limit"),
                "QuotaExceeded".to_owned(),
            ),
            (
                ResponseTemplate::new(429).set_body_string("Too many requests"),
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
            let error = synthesize(&link_to(&server), API_KEY, plain_req("hello"))
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

        let pcm = synthesize(&link_to(&server), API_KEY, plain_req("hello"))
            .await
            .unwrap();

        assert_eq!(pcm.samples.len(), samples);
    }

    #[tokio::test]
    async fn probe_outcomes_follow_the_voice_list_status() {
        let no_hint = format!("RateLimited({NO_HINT_WAIT_SECS})");
        for (template, expected) in [
            (
                ResponseTemplate::new(200).set_body_bytes(b"[]"),
                "Ok".to_owned(),
            ),
            (ResponseTemplate::new(401), "Unauthorized".to_owned()),
            (ResponseTemplate::new(403), "Unauthorized".to_owned()),
            (ResponseTemplate::new(429), no_hint),
            (throttled(9), "RateLimited(9)".to_owned()),
            (ResponseTemplate::new(500), "Http".to_owned()),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/cognitiveservices/voices/list"))
                .and(header("Ocp-Apim-Subscription-Key", API_KEY))
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
            matches!(&error, AzureError::Http(detail) if detail == "unexpected status 500"),
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
            matches!(&error, AzureError::Http(detail) if detail.contains(&catalog_cap_text())),
            "{error}"
        );
    }
}
