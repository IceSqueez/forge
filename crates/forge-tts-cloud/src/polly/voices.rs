use forge_net::{CallClass, GatewayFailure, Method};
use forge_speech_types::{EngineId, TtsVoice, VoiceGender, VoiceId};
use forge_tts_core::ENGINE_ID_POLLY;
use serde::Deserialize;
use tokio::time::{Instant, timeout_at};

use crate::gateway_failure::failure_detail;
use crate::network::ProviderLink;
use crate::polly::SAMPLE_RATE_HZ;
use crate::polly::error::PollyError;
use crate::polly::signer::{PollySigner, SignedCall};
use crate::polly::synth::{NEURAL_ENGINE, VOICES_PATH, classify_with};

const NEXT_TOKEN_PARAM: &str = "NextToken";
const MAX_VOICE_PAGES: usize = 32;
const LISTING_BUDGET_ELAPSED: &str = "voice listing exceeded its time budget";
const LISTING_PAGE_LIMIT: &str = "voice listing exceeded its page limit";

#[derive(Deserialize)]
struct PollyVoiceList {
    #[serde(rename = "Voices")]
    voices: Vec<PollyVoice>,
    #[serde(rename = "NextToken", default)]
    next_token: Option<String>,
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
    #[serde(rename = "SupportedEngines", default)]
    supported_engines: Vec<String>,
}

impl PollyVoice {
    fn speaks_neural(&self) -> bool {
        self.supported_engines.is_empty()
            || self
                .supported_engines
                .iter()
                .any(|engine| engine == NEURAL_ENGINE)
    }
}

fn gender_from_str(s: &str) -> VoiceGender {
    match s {
        "Female" => VoiceGender::Female,
        "Male" => VoiceGender::Male,
        _ => VoiceGender::Neutral,
    }
}

fn classify_error(failure: &GatewayFailure, link: &ProviderLink) -> PollyError {
    classify_with(failure, link, None, failure_detail)
}

pub(super) async fn fetch_voices(
    link: &ProviderLink,
    signer: &PollySigner,
) -> Result<Vec<TtsVoice>, PollyError> {
    let deadline = Instant::now() + link.catalog_budget();
    timeout_at(deadline, fetch_all_pages(link, signer))
        .await
        .unwrap_or_else(|_| Err(PollyError::Http(LISTING_BUDGET_ELAPSED.into())))
}

async fn fetch_all_pages(
    link: &ProviderLink,
    signer: &PollySigner,
) -> Result<Vec<TtsVoice>, PollyError> {
    let body_cap = CallClass::Catalog.body_cap();
    let mut received: u64 = 0;
    let mut voices = Vec::new();
    let mut next_token: Option<String> = None;
    for _ in 0..MAX_VOICE_PAGES {
        let page = fetch_page(link, signer, next_token.as_deref()).await?;
        received = received.saturating_add(page.body_len);
        if received > body_cap {
            return Err(PollyError::Http(failure_detail(
                &GatewayFailure::ResponseTooLarge { cap: body_cap },
            )));
        }
        voices.extend(page.list.voices);
        match page.list.next_token.filter(|token| !token.is_empty()) {
            Some(token) => next_token = Some(token),
            None => return Ok(into_tts_voices(voices)),
        }
    }
    Err(PollyError::Http(LISTING_PAGE_LIMIT.into()))
}

struct VoicePage {
    list: PollyVoiceList,
    body_len: u64,
}

async fn fetch_page(
    link: &ProviderLink,
    signer: &PollySigner,
    next_token: Option<&str>,
) -> Result<VoicePage, PollyError> {
    let query: Vec<(&str, &str)> = next_token
        .map(|token| (NEXT_TOKEN_PARAM, token))
        .into_iter()
        .collect();
    let request = signer
        .request(
            link,
            SignedCall {
                method: Method::Get,
                path: VOICES_PATH,
                query: &query,
                json_body: None,
            },
        )?
        .class(CallClass::Catalog);
    let response = link
        .send(request)
        .await
        .map_err(|failure| classify_error(&failure, link))?;
    let list: PollyVoiceList = response
        .json()
        .map_err(|e| PollyError::Decode(e.to_string()))?;
    Ok(VoicePage {
        list,
        body_len: u64::try_from(response.body().len()).unwrap_or(u64::MAX),
    })
}

fn into_tts_voices(voices: Vec<PollyVoice>) -> Vec<TtsVoice> {
    let engine_id = EngineId(ENGINE_ID_POLLY.into());
    voices
        .into_iter()
        .filter(PollyVoice::speaks_neural)
        .map(|v| TtsVoice {
            id: VoiceId(v.id),
            name: v.name,
            locale: v.language_code,
            gender: gender_from_str(&v.gender),
            engine_id: engine_id.clone(),
            is_neural: true,
            sample_rate_hint: SAMPLE_RATE_HZ,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::network::test_support::{
        NO_HINT_WAIT_SECS, link_to, link_with_catalog_budget, request_count, unpaced_link_to,
    };
    use crate::polly::signer::test_support::{
        assert_reveals_no_credential, credentials, echo_server, echoing, outcome,
    };
    use wiremock::matchers::any;
    use wiremock::{Mock, MockServer, Respond, ResponseTemplate};

    const PADDED_TOKEN: &str = "a+b/c=d==";
    const SHORT_CATALOG_BUDGET: Duration = Duration::from_millis(400);
    const HANG_GUARD: Duration = Duration::from_secs(30);

    fn signer() -> PollySigner {
        PollySigner::new(credentials(None))
    }

    fn voice(id: &str, gender: &str) -> serde_json::Value {
        serde_json::json!({
            "Id": id,
            "Name": format!("{id} name"),
            "LanguageCode": "en-US",
            "Gender": gender
        })
    }

    fn page(ids: &[&str], next: Option<&str>) -> ResponseTemplate {
        let voices: Vec<serde_json::Value> = ids.iter().map(|id| voice(id, "Female")).collect();
        let mut body = serde_json::json!({ "Voices": voices });
        if let Some(token) = next {
            body["NextToken"] = token.into();
        }
        ResponseTemplate::new(200).set_body_json(body)
    }

    fn padded_page(next: Option<&str>, total_len: usize) -> ResponseTemplate {
        let shell = |pad: &str| match next {
            Some(token) => format!(r#"{{"Voices":[],"NextToken":"{token}","Pad":"{pad}"}}"#),
            None => format!(r#"{{"Voices":[],"Pad":"{pad}"}}"#),
        };
        let pad = "x".repeat(total_len - shell("").len());
        let body = shell(&pad);
        assert_eq!(body.len(), total_len);
        ResponseTemplate::new(200).set_body_raw(body.into_bytes(), "application/json")
    }

    fn incoming_token(request: &wiremock::Request) -> Option<String> {
        request
            .url
            .query_pairs()
            .find(|(name, _)| name == NEXT_TOKEN_PARAM)
            .map(|(_, value)| value.into_owned())
    }

    struct ByToken(Vec<(Option<String>, ResponseTemplate)>);

    impl Respond for ByToken {
        fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
            let token = incoming_token(request);
            self.0
                .iter()
                .find(|(expected, _)| *expected == token)
                .map(|(_, template)| template.clone())
                .unwrap_or_else(|| ResponseTemplate::new(404))
        }
    }

    fn chain(pages: Vec<ResponseTemplate>, tokens: &[&str]) -> ByToken {
        let incoming = std::iter::once(None).chain(tokens.iter().map(|t| Some((*t).to_owned())));
        ByToken(incoming.zip(pages).collect())
    }

    async fn serving(responder: impl Respond + 'static) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(responder)
            .mount(&server)
            .await;
        server
    }

    async fn sent_tokens(server: &MockServer) -> Vec<Option<String>> {
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(incoming_token)
            .collect()
    }

    fn page_chain(total_pages: usize) -> ByToken {
        let tokens: Vec<String> = (1..total_pages).map(|k| format!("p{k}")).collect();
        let pages = (0..total_pages)
            .map(|k| page(&[&format!("v{k}")], tokens.get(k).map(String::as_str)))
            .collect();
        let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
        chain(pages, &refs)
    }

    #[tokio::test]
    async fn list_voices_maps_each_polly_voice_into_a_neural_tts_voice() {
        let body = serde_json::json!({
            "Voices": [voice("Joanna", "Female"), voice("Matthew", "Male"), voice("Other", "")]
        });
        let server = serving(ResponseTemplate::new(200).set_body_json(body)).await;

        let voices = fetch_voices(&link_to(&server), &signer()).await.unwrap();

        let mapped: Vec<(String, String, String, VoiceGender, bool, u32)> = voices
            .into_iter()
            .map(|v| {
                (
                    v.id.0,
                    v.name,
                    v.locale,
                    v.gender,
                    v.is_neural,
                    v.sample_rate_hint,
                )
            })
            .collect();
        assert_eq!(
            mapped,
            vec![
                (
                    "Joanna".into(),
                    "Joanna name".into(),
                    "en-US".into(),
                    VoiceGender::Female,
                    true,
                    16_000
                ),
                (
                    "Matthew".into(),
                    "Matthew name".into(),
                    "en-US".into(),
                    VoiceGender::Male,
                    true,
                    16_000
                ),
                (
                    "Other".into(),
                    "Other name".into(),
                    "en-US".into(),
                    VoiceGender::Neutral,
                    true,
                    16_000
                ),
            ]
        );
    }

    #[test]
    fn voices_without_neural_support_are_filtered_out_of_the_catalog() {
        let listing: serde_json::Value = serde_json::json!([
            {"Id": "Neural", "Name": "n", "LanguageCode": "en-US", "Gender": "Female",
             "SupportedEngines": ["neural", "standard"]},
            {"Id": "StandardOnly", "Name": "s", "LanguageCode": "en-US", "Gender": "Female",
             "SupportedEngines": ["standard"]},
            {"Id": "NeuralOnly", "Name": "n2", "LanguageCode": "en-US", "Gender": "Male",
             "SupportedEngines": ["neural"]},
            {"Id": "Unreported", "Name": "u", "LanguageCode": "en-US", "Gender": "Male"},
            {"Id": "EmptyList", "Name": "e", "LanguageCode": "en-US", "Gender": "Male",
             "SupportedEngines": []}
        ]);
        let parsed: Vec<PollyVoice> = serde_json::from_value(listing).unwrap();

        let ids: Vec<String> = into_tts_voices(parsed)
            .into_iter()
            .map(|v| v.id.0)
            .collect();

        assert_eq!(ids, vec!["Neural", "NeuralOnly", "Unreported", "EmptyList"]);
    }

    #[tokio::test]
    async fn catalog_failures_map_to_provider_errors_without_revealing_credentials() {
        let no_hint = format!("RateLimited({NO_HINT_WAIT_SECS})");
        for (status, text, retry_after, expected) in [
            (401, "denied", None, "Unauthorized".to_owned()),
            (
                403,
                "SignatureDoesNotMatch",
                None,
                "Unauthorized".to_owned(),
            ),
            (429, "slow down", None, no_hint.clone()),
            (429, "quota exceeded", None, "QuotaExceeded".to_owned()),
            (429, "slow down", Some(7), "RateLimited(7)".to_owned()),
            (500, "internal", None, "Http".to_owned()),
            (200, "not json", None, "Decode".to_owned()),
        ] {
            let server = echo_server(echoing(status, text, retry_after)).await;
            let error = fetch_voices(&link_to(&server), &signer())
                .await
                .unwrap_err();
            assert_eq!(outcome(&error), expected, "{status} {text}: {error}");
            assert_reveals_no_credential(&error.to_string(), &server).await;
        }
    }

    #[tokio::test]
    async fn listing_follows_next_tokens_and_keeps_page_order() {
        let server = serving(chain(
            vec![
                page(&["a1", "a2"], Some(PADDED_TOKEN)),
                page(&["b1"], Some("t2")),
                page(&["c1"], None),
            ],
            &[PADDED_TOKEN, "t2"],
        ))
        .await;

        let voices = fetch_voices(&link_to(&server), &signer()).await.unwrap();

        let ids: Vec<String> = voices.into_iter().map(|v| v.id.0).collect();
        assert_eq!(ids, ["a1", "a2", "b1", "c1"]);
        assert_eq!(
            sent_tokens(&server).await,
            [None, Some(PADDED_TOKEN.to_owned()), Some("t2".to_owned())]
        );
    }

    #[tokio::test]
    async fn an_empty_next_token_ends_the_listing() {
        let server = serving(chain(
            vec![page(&["a1"], Some("")), page(&["never"], None)],
            &[""],
        ))
        .await;

        let voices = fetch_voices(&link_to(&server), &signer()).await.unwrap();

        assert_eq!(voices.len(), 1);
        assert_eq!(request_count(&server).await, 1);
    }

    #[tokio::test]
    async fn listing_accepts_the_page_cap_and_fails_one_page_past_it() {
        for (total_pages, expected_ok) in [(MAX_VOICE_PAGES, true), (MAX_VOICE_PAGES + 1, false)] {
            let server = serving(page_chain(total_pages)).await;

            let result = fetch_voices(&unpaced_link_to(&server), &signer()).await;

            match result {
                Ok(voices) => {
                    assert!(expected_ok, "{total_pages}");
                    assert_eq!(voices.len(), MAX_VOICE_PAGES);
                }
                Err(error) => {
                    assert!(!expected_ok, "{total_pages}: {error}");
                    assert!(
                        matches!(&error, PollyError::Http(detail) if detail == LISTING_PAGE_LIMIT),
                        "{error}"
                    );
                }
            }
            assert_eq!(
                request_count(&server).await,
                MAX_VOICE_PAGES,
                "{total_pages}"
            );
        }
    }

    #[tokio::test]
    async fn listing_body_cap_spans_all_pages_up_to_the_byte() {
        let cap = usize::try_from(CallClass::Catalog.body_cap()).unwrap();
        let first_len = cap / 2;
        for (second_len, expected_ok) in [(cap - first_len, true), (cap - first_len + 1, false)] {
            let server = serving(chain(
                vec![
                    padded_page(Some("t"), first_len),
                    padded_page(None, second_len),
                ],
                &["t"],
            ))
            .await;

            let result = fetch_voices(&link_to(&server), &signer()).await;

            match result {
                Ok(_) => assert!(expected_ok, "{second_len}"),
                Err(error) => {
                    assert!(!expected_ok, "{second_len}: {error}");
                    assert!(
                        matches!(&error, PollyError::Http(detail) if detail.contains(&cap.to_string())),
                        "{error}"
                    );
                }
            }
            assert_eq!(request_count(&server).await, 2);
        }
    }

    #[tokio::test]
    async fn a_failing_middle_page_fails_the_whole_listing() {
        let no_hint = format!("RateLimited({NO_HINT_WAIT_SECS})");
        for (failure, expected) in [
            (ResponseTemplate::new(500), "Http".to_owned()),
            (ResponseTemplate::new(403), "Unauthorized".to_owned()),
            (ResponseTemplate::new(429), no_hint.clone()),
            (
                ResponseTemplate::new(200).set_body_string("{"),
                "Decode".to_owned(),
            ),
        ] {
            let server = serving(chain(
                vec![page(&["a1"], Some("t2")), failure, page(&["c1"], None)],
                &["t2", "t3"],
            ))
            .await;

            let error = fetch_voices(&link_to(&server), &signer())
                .await
                .unwrap_err();

            assert_eq!(outcome(&error), expected);
            assert_eq!(request_count(&server).await, 2);
        }
    }

    #[tokio::test]
    async fn pages_finishing_just_inside_the_catalog_budget_complete_the_listing() {
        let mut budget = SHORT_CATALOG_BUDGET;
        loop {
            let server = serving(chain(
                vec![
                    page(&["a1"], Some("t2")),
                    page(&["b1"], None).set_delay(budget * 3 / 4),
                ],
                &["t2"],
            ))
            .await;
            let link = link_with_catalog_budget(&server, budget);
            let started = Instant::now();

            let result = tokio::time::timeout(HANG_GUARD, fetch_voices(&link, &signer()))
                .await
                .expect("the listing must give up on its own");

            if started.elapsed() < budget {
                assert_eq!(result.unwrap().len(), 2);
                return;
            }
            budget *= 2;
            assert!(budget < HANG_GUARD, "the listing never fit its budget");
        }
    }

    #[tokio::test]
    async fn pages_outlasting_the_catalog_budget_together_fail_the_listing() {
        let mut budget = SHORT_CATALOG_BUDGET;
        loop {
            let server = serving(chain(
                vec![
                    page(&["a1"], Some("t2")).set_delay(budget / 2),
                    page(&["b1"], None).set_delay(HANG_GUARD * 10),
                ],
                &["t2"],
            ))
            .await;
            let link = link_with_catalog_budget(&server, budget);
            let started = Instant::now();

            let result = tokio::time::timeout(HANG_GUARD, fetch_voices(&link, &signer()))
                .await
                .expect("the listing must give up on its own");

            let waited = started.elapsed();
            if request_count(&server).await == 2 && waited < budget * 3 / 2 {
                assert!(
                    matches!(&result, Err(PollyError::Http(detail)) if detail == LISTING_BUDGET_ELAPSED),
                    "{result:?}"
                );
                assert!(waited >= budget, "waited {waited:?}");
                return;
            }
            budget *= 2;
            assert!(budget < HANG_GUARD, "the second page never went out");
        }
    }
}
