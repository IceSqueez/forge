use forge_net::{CallClass, GatewayFailure, GatewayRequest, RequestBody};
use forge_speech_types::PcmBuffer;
use forge_speech_types::VoiceId;
use forge_tts_core::{SynthesisRequest, xml_escape};
use forge_types::Secret;

use crate::azure::SAMPLE_RATE_HZ;
use crate::azure::error::AzureError;
use crate::gateway_failure::{
    Classified, ProviderPolicy, VoiceSignal, classify, failure_detail, probe_failure_detail,
};
use crate::network::ProviderLink;

pub(crate) const SUBSCRIPTION_KEY_HEADER: &str = "Ocp-Apim-Subscription-Key";
pub(crate) const VOICES_LIST_PATH: &str = "/cognitiveservices/voices/list";
pub(crate) const INVALID_API_KEY: &str = "invalid API key";
const QUOTA_MARKERS: &[&str] = &["quota", "characters"];
const SPEAK_ELEMENT_OPEN: &str = "<speak";
const FALLBACK_LOCALE: &str = "en-US";
const LOCALE_PARTS: usize = 2;
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
        if req.rate_multiplier != 1.0 || req.pitch_semitones.abs() > f32::EPSILON {
            tracing::debug!(
                rate_multiplier = req.rate_multiplier,
                pitch_semitones = req.pitch_semitones,
                "Azure SSML is sent as written; rate and pitch are ignored"
            );
        }
        if req.text.contains(SPEAK_ELEMENT_OPEN) {
            return req.text.clone();
        }
        return speak_document(&req.voice_id.0, &req.text);
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

    speak_document(&req.voice_id.0, &inner)
}

fn speak_document(voice_id: &str, inner: &str) -> String {
    format!(
        r#"<speak version="1.0" xml:lang="{}"><voice name="{}">{inner}</voice></speak>"#,
        xml_escape(voice_locale(voice_id)),
        xml_escape(voice_id)
    )
}

fn voice_locale(voice_id: &str) -> &str {
    let mut parts = voice_id.splitn(LOCALE_PARTS + 1, '-');
    let language = parts.next().unwrap_or_default();
    let region = parts.next().unwrap_or_default();
    if parts.next().is_some() && !language.is_empty() && !region.is_empty() {
        &voice_id[..language.len() + 1 + region.len()]
    } else {
        FALLBACK_LOCALE
    }
}

const POLICY: ProviderPolicy = ProviderPolicy {
    quota_markers: QUOTA_MARKERS,
    payment_required_is_quota: false,
    voice_signal: VoiceSignal::BadRequestNamingVoice,
};

pub(super) fn classify_with(
    failure: &GatewayFailure,
    link: &ProviderLink,
    voice: Option<&VoiceId>,
    detail: fn(&GatewayFailure) -> String,
) -> AzureError {
    match classify(failure, link, POLICY, voice, detail) {
        Classified::Quota(excerpt) => AzureError::QuotaExceeded(excerpt),
        Classified::Throttled(retry_after_secs) => AzureError::RateLimited { retry_after_secs },
        Classified::Auth => AzureError::Unauthorized(INVALID_API_KEY.into()),
        Classified::VoiceMissing(voice) => AzureError::VoiceNotFound(voice),
        Classified::Rejected(detail) => AzureError::Rejected(detail),
        Classified::Transient(detail) => AzureError::Http(detail),
    }
}

fn classify_error(failure: &GatewayFailure, link: &ProviderLink, voice_id: &VoiceId) -> AzureError {
    classify_with(failure, link, Some(voice_id), failure_detail)
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
        Err(failure) => Err(classify_with(&failure, link, None, probe_failure_detail)),
    }
}

#[cfg(test)]
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

    #[test]
    fn the_ssml_language_follows_the_voice_locale() {
        for (voice, lang) in [
            ("en-US-AriaNeural", "en-US"),
            ("uk-UA-OstapNeural", "uk-UA"),
            ("zh-CN-liaoning-XiaobeiNeural", "zh-CN"),
            ("de-DE-Seraphina:DragonHDLatestNeural", "de-DE"),
            ("Aria", "en-US"),
            ("en-US", "en-US"),
            ("-US-Aria", "en-US"),
            ("en--Aria", "en-US"),
            ("", "en-US"),
        ] {
            let req = SynthesisRequest {
                voice_id: VoiceId(voice.into()),
                ..plain_req("x")
            };
            let expected = format!(r#"<speak version="1.0" xml:lang="{lang}">"#);
            assert!(build_ssml(&req).starts_with(&expected), "{voice}");
        }
    }

    #[test]
    fn ssml_input_without_a_speak_element_is_wrapped_in_the_voice_document() {
        let req = SynthesisRequest {
            text: "hello <break time=\"1s\"/> world".into(),
            ssml: true,
            ..plain_req("")
        };
        assert_eq!(
            build_ssml(&req),
            r#"<speak version="1.0" xml:lang="en-US"><voice name="en-US-AriaNeural">hello <break time="1s"/> world</voice></speak>"#
        );
    }

    #[test]
    fn ssml_input_ignores_rate_and_pitch() {
        let input = r#"<speak version="1.0"><voice name="Aria">test</voice></speak>"#;
        let req = SynthesisRequest {
            text: input.into(),
            ssml: true,
            rate_multiplier: 2.0,
            pitch_semitones: 5.0,
            ..plain_req("")
        };
        assert_eq!(build_ssml(&req), input);
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
            AzureError::Rejected(_) => "Rejected".into(),
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
                "Rejected".to_owned(),
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
            matches!(&error, AzureError::Rejected(detail) if detail.contains(&catalog_cap_text())),
            "{error}"
        );
    }

    #[tokio::test]
    async fn every_gateway_failure_without_a_body_is_classified_by_the_provider_table() {
        let link = crate::network::CloudGateways::new()
            .provider("p")
            .link("https://api.example.com")
            .unwrap();
        let voice = VoiceId("v".into());

        let got: Vec<(&str, String)> = crate::network::test_support::every_failure_without_a_body()
            .iter()
            .map(|(label, failure)| {
                (
                    *label,
                    outcome(&classify_with(failure, &link, Some(&voice), failure_detail)),
                )
            })
            .collect();

        let want: Vec<(&str, String)> = [
            ("401", "Unauthorized"),
            ("403", "Unauthorized"),
            ("404", "Rejected"),
            ("429", "RateLimited(3)"),
            ("400", "Rejected"),
            ("402", "Rejected"),
            ("408", "Http"),
            ("418", "Rejected"),
            ("500", "Http"),
            ("302", "Rejected"),
            ("budget", "RateLimited(0)"),
            ("too-large", "Rejected"),
            ("cancelled", "Rejected"),
            ("address", "Rejected"),
            ("invalid", "Rejected"),
        ]
        .into_iter()
        .map(|(label, class)| (label, class.to_owned()))
        .collect();
        assert_eq!(got, want);
    }

    #[tokio::test]
    async fn probe_and_synthesis_give_the_same_class_for_auth_and_throttle_statuses() {
        let no_hint = format!("RateLimited({NO_HINT_WAIT_SECS})");
        for (status, retry, expected) in [
            (401, None, "Unauthorized".to_owned()),
            (403, None, "Unauthorized".to_owned()),
            (429, None, no_hint),
            (429, Some(9), "RateLimited(9)".to_owned()),
        ] {
            let template = match retry {
                Some(secs) => throttled(secs),
                None => ResponseTemplate::new(status),
            };
            let server = serving(template).await;
            let synthesized = synthesize(&link_to(&server), API_KEY, plain_req("hello"))
                .await
                .unwrap_err();
            let probed = probe_connection(&link_to(&server), API_KEY)
                .await
                .unwrap_err();
            assert_eq!(
                (outcome(&synthesized), outcome(&probed)),
                (expected.clone(), expected),
                "{status} {retry:?}"
            );
        }
    }
}
