use forge_audio::PcmBuffer;
use forge_net::{CallClass, GatewayFailure, Method};
use forge_tts_core::{SynthesisRequest, VoiceId, xml_escape};
use serde_json::json;

use crate::gateway_failure::{
    failure_detail, probe_failure_detail, quota_excerpt, rejected_excerpt,
};
use crate::network::ProviderLink;
use crate::polly::SAMPLE_RATE_HZ;
use crate::polly::error::PollyError;
use crate::polly::signer::{PollySigner, SignedCall};

pub(crate) const VOICES_PATH: &str = "/v1/voices";
pub(crate) const INVALID_CREDENTIALS: &str = "invalid credentials";
const SPEECH_PATH: &str = "/v1/speech";
const BAD_REQUEST: u16 = 400;
const QUOTA_MARKERS: &[&str] = &["quota", "characters"];

fn format_signed_percent(val: i32) -> String {
    if val >= 0 {
        format!("+{val}%")
    } else {
        format!("{val}%")
    }
}

pub(super) fn build_polly_body(req: &SynthesisRequest) -> String {
    if req.ssml {
        let body = json!({
            "Text": req.text,
            "TextType": "ssml",
            "VoiceId": req.voice_id.0,
            "OutputFormat": "pcm",
            "SampleRate": SAMPLE_RATE_HZ.to_string(),
            "Engine": "neural"
        });
        return body.to_string();
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
            "SampleRate": SAMPLE_RATE_HZ.to_string(),
            "Engine": "neural"
        });
        body.to_string()
    } else {
        let body = json!({
            "Text": req.text,
            "TextType": "text",
            "VoiceId": req.voice_id.0,
            "OutputFormat": "pcm",
            "SampleRate": SAMPLE_RATE_HZ.to_string(),
            "Engine": "neural"
        });
        body.to_string()
    }
}

fn classify_error(failure: &GatewayFailure, link: &ProviderLink, voice_id: &VoiceId) -> PollyError {
    if let Some(excerpt) = quota_excerpt(failure, QUOTA_MARKERS) {
        return PollyError::QuotaExceeded(excerpt.to_owned());
    }
    if let Some(retry_after_secs) = link.throttle_secs(failure) {
        return PollyError::RateLimited { retry_after_secs };
    }
    if rejected_excerpt(failure, BAD_REQUEST)
        .is_some_and(|excerpt| excerpt.to_ascii_lowercase().contains("voice"))
    {
        return PollyError::VoiceNotFound(voice_id.0.clone());
    }
    match failure {
        GatewayFailure::Unauthorized { .. } | GatewayFailure::Forbidden { .. } => {
            PollyError::Unauthorized(INVALID_CREDENTIALS.into())
        }
        other => PollyError::Http(failure_detail(other)),
    }
}

pub(super) async fn synthesize(
    link: &ProviderLink,
    signer: &PollySigner,
    req: SynthesisRequest,
) -> Result<PcmBuffer, PollyError> {
    let body = build_polly_body(&req);
    let request = signer.request(
        link,
        SignedCall {
            method: Method::Post,
            path: SPEECH_PATH,
            query: &[],
            json_body: Some(body.into_bytes()),
        },
    )?;
    let response = link
        .send(request)
        .await
        .map_err(|failure| classify_error(&failure, link, &req.voice_id))?;
    let (pairs, _) = response.body().as_chunks::<2>();
    let samples: Vec<i16> = pairs.iter().map(|pair| i16::from_le_bytes(*pair)).collect();
    Ok(PcmBuffer::new(samples, SAMPLE_RATE_HZ, 1))
}

pub(super) async fn probe_connection(
    link: &ProviderLink,
    signer: &PollySigner,
) -> Result<(), PollyError> {
    let request = signer
        .request(
            link,
            SignedCall {
                method: Method::Get,
                path: VOICES_PATH,
                query: &[],
                json_body: None,
            },
        )?
        .class(CallClass::Catalog);
    match link.send(request).await {
        Ok(_) => Ok(()),
        Err(GatewayFailure::Unauthorized { .. } | GatewayFailure::Forbidden { .. }) => {
            Err(PollyError::Unauthorized(INVALID_CREDENTIALS.into()))
        }
        Err(failure) => match link.throttle_secs(&failure) {
            Some(retry_after_secs) => Err(PollyError::RateLimited { retry_after_secs }),
            None => Err(PollyError::Http(probe_failure_detail(&failure))),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::test_support::{NO_HINT_WAIT_SECS, assert_reveals_nothing, link_to};
    use crate::polly::signer::test_support::{
        assert_reveals_no_credential, credentials, echo_server, echoing, outcome,
    };
    use forge_tts_core::TtsError;
    use wiremock::matchers::{body_string, header, method, path};
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

    fn signer() -> PollySigner {
        PollySigner::new(credentials(None))
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
    fn synthesize_escapes_xml_specials_inside_prosody_wrapper() {
        let req = SynthesisRequest {
            rate_multiplier: 1.5,
            ..plain_req(r#"</prosody><break time="10s"/> & 'q'"#)
        };
        let (body, _) = build_polly_body(&req);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            parsed["Text"],
            r#"<speak><prosody rate="+50%">&lt;/prosody&gt;&lt;break time=&quot;10s&quot;/&gt; &amp; &apos;q&apos;</prosody></speak>"#
        );
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
    async fn synthesize_posts_the_json_body_and_reads_little_endian_pcm() {
        let request = plain_req("hello");
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(SPEECH_PATH))
            .and(header("content-type", "application/json"))
            .and(body_string(build_polly_body(&request).0))
            .respond_with(
                ResponseTemplate::new(200).set_body_bytes(raw_pcm_bytes(&[100, -100, 200, -200])),
            )
            .mount(&server)
            .await;

        let pcm = synthesize(&link_to(&server), &signer(), request)
            .await
            .unwrap();

        assert_eq!(
            (pcm.sample_rate, pcm.channels, pcm.samples),
            (16_000, 1, vec![100, -100, 200, -200])
        );
    }

    #[tokio::test]
    async fn synthesis_failures_map_to_provider_errors_without_revealing_credentials() {
        let no_hint = format!("RateLimited({NO_HINT_WAIT_SECS})");
        for (status, text, retry_after, expected) in [
            (401, "denied", None, "Unauthorized".to_owned()),
            (
                403,
                "SignatureDoesNotMatch",
                None,
                "Unauthorized".to_owned(),
            ),
            (
                400,
                "Voice Joanna is not supported",
                None,
                "VoiceNotFound".to_owned(),
            ),
            (400, "malformed request", None, "Http".to_owned()),
            (429, "slow down", None, no_hint.clone()),
            (429, "slow down", Some(7), "RateLimited(7)".to_owned()),
            (429, "quota exceeded", None, "QuotaExceeded".to_owned()),
            (429, "QUOTA reached", Some(7), "QuotaExceeded".to_owned()),
            (
                429,
                "monthly Characters used up",
                None,
                "QuotaExceeded".to_owned(),
            ),
            (503, "quota", Some(2), "RateLimited(2)".to_owned()),
            (503, "unavailable", None, "Http".to_owned()),
            (500, "internal", None, "Http".to_owned()),
        ] {
            let server = echo_server(echoing(status, text, retry_after)).await;
            let error = synthesize(&link_to(&server), &signer(), plain_req("hello"))
                .await
                .unwrap_err();
            assert_eq!(outcome(&error), expected, "{status} {text}: {error}");
            for shown in [error.to_string(), TtsError::from(error).to_string()] {
                assert_reveals_no_credential(&shown, &server).await;
                assert_reveals_nothing(&shown, &server);
            }
        }
    }

    #[tokio::test]
    async fn probe_outcomes_follow_the_voice_list_status_without_the_body() {
        let no_hint = format!("RateLimited({NO_HINT_WAIT_SECS})");
        for (status, retry_after, expected) in [
            (200, None, "Ok".to_owned()),
            (401, None, "Unauthorized".to_owned()),
            (403, None, "Unauthorized".to_owned()),
            (429, None, no_hint.clone()),
            (429, Some(7), "RateLimited(7)".to_owned()),
            (500, None, "Http(unexpected status 500)".to_owned()),
        ] {
            let server = echo_server(echoing(status, "probe reply", retry_after)).await;
            let result = probe_connection(&link_to(&server), &signer()).await;
            let seen = match &result {
                Ok(()) => "Ok".to_owned(),
                Err(PollyError::Http(detail)) => format!("Http({detail})"),
                Err(error) => outcome(error),
            };
            assert_eq!(seen, expected, "{status}");
            let requests = server.received_requests().await.unwrap();
            assert_eq!(
                (requests[0].method.as_str(), requests[0].url.path()),
                ("GET", VOICES_PATH)
            );
        }
    }
}
