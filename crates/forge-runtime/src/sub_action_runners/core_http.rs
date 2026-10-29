use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigReason, ConfigSpec, DurationBounds,
    DurationField, JsonObjectField, RunContext, StepTimer, SubActionCategory, SubActionIo,
    SubActionRunner, TextEditor, TextField, ToggleField, TypedField,
};
use forge_storage::{SettingsRepo, read_setting_or_default};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

use crate::egress::{EgressClient, EgressRequest, EgressResponse, HttpMethod};
use crate::settings;

const TIMEOUT_MIN_MS: i64 = 100;
const TIMEOUT_MAX_MS: i64 = 60_000;
const DEFAULT_TIMEOUT_MS: i64 = 10_000;
const EMPTY_JSON_OBJECT: &str = "{}";
const CONTENT_TYPE_HEADER: &str = "content-type";

const HTTP_STATUS_CODE_VAR: &str = "http.status_code";
const HTTP_HEADERS_VAR: &str = "http.headers";
const HTTP_BODY_VAR: &str = "http.body";

#[derive(Clone, Copy)]
enum ContentType {
    Json,
    FormUrlEncoded,
    PlainText,
    Multipart,
}

impl ConfigChoice for ContentType {
    const ALL: &'static [Self] = &[
        Self::Json,
        Self::FormUrlEncoded,
        Self::PlainText,
        Self::Multipart,
    ];
    const WIRE_NAMES: &'static [&'static str] = &[
        "application/json",
        "application/x-www-form-urlencoded",
        "text/plain",
        "multipart/form-data",
    ];

    fn wire_name(self) -> &'static str {
        match self {
            Self::Json => "application/json",
            Self::FormUrlEncoded => "application/x-www-form-urlencoded",
            Self::PlainText => "text/plain",
            Self::Multipart => "multipart/form-data",
        }
    }
}

#[derive(Clone, Copy)]
enum ParseResponseAs {
    Text,
    Json,
    Ignore,
}

impl ConfigChoice for ParseResponseAs {
    const ALL: &'static [Self] = &[Self::Text, Self::Json, Self::Ignore];
    const WIRE_NAMES: &'static [&'static str] = &["text", "json", "ignore"];

    fn wire_name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Ignore => "ignore",
        }
    }
}

const URL: TextField = TextField {
    key: "url",
    label: "URL",
    editor: TextEditor::Line {
        placeholder: "https://api.example.com/endpoint",
    },
    default: "",
    required: true,
};

const BODY: TextField = TextField {
    key: "body",
    label: "Body",
    editor: TextEditor::Area,
    default: "",
    required: false,
};

const CONTENT_TYPE: ChoiceField<ContentType> = ChoiceField {
    key: "content_type",
    label: "Content Type",
    default: ContentType::Json,
};

const HEADERS: JsonObjectField = JsonObjectField {
    key: "headers",
    label: "Headers (JSON object)",
    default: EMPTY_JSON_OBJECT,
};

const QUERY_PARAMS: JsonObjectField = JsonObjectField {
    key: "query_params",
    label: "Query Parameters (JSON object)",
    default: EMPTY_JSON_OBJECT,
};

const TIMEOUT: DurationField = DurationField {
    key: "timeout_ms",
    label: "Timeout",
    default: DEFAULT_TIMEOUT_MS,
    bounds: DurationBounds::in_millis(TIMEOUT_MIN_MS, TIMEOUT_MAX_MS),
};

const FOLLOW_REDIRECTS: ToggleField = ToggleField {
    key: "follow_redirects",
    label: "Follow Redirects",
    default: true,
};

const PARSE_RESPONSE_AS: ChoiceField<ParseResponseAs> = ChoiceField {
    key: "parse_response_as",
    label: "Parse Response As",
    default: ParseResponseAs::Json,
};

const WITHOUT_BODY: ConfigSpec = ConfigSpec::new(&[
    &URL,
    &HEADERS,
    &QUERY_PARAMS,
    &TIMEOUT,
    &FOLLOW_REDIRECTS,
    &PARSE_RESPONSE_AS,
]);

const WITH_BODY: ConfigSpec = ConfigSpec::new(&[
    &URL,
    &BODY,
    &CONTENT_TYPE,
    &HEADERS,
    &QUERY_PARAMS,
    &TIMEOUT,
    &FOLLOW_REDIRECTS,
    &PARSE_RESPONSE_AS,
]);

struct HttpCall {
    url: String,
    headers: BTreeMap<String, String>,
    query_params: BTreeMap<String, String>,
    payload: Option<(String, ContentType)>,
    timeout: Duration,
    follow_redirects: bool,
    parse_as: ParseResponseAs,
}

fn read_call(
    config: &SubActionConfig,
    args: &ArgStack,
    has_body: bool,
) -> Result<HttpCall, ConfigProblem> {
    let payload = if has_body {
        Some((BODY.read(config, args)?, CONTENT_TYPE.read(config, args)?))
    } else {
        None
    };
    Ok(HttpCall {
        url: URL.read(config, args)?,
        headers: interpolated_strings(&HEADERS, HEADERS.read(config, args)?, args)?,
        query_params: interpolated_strings(&QUERY_PARAMS, QUERY_PARAMS.read(config, args)?, args)?,
        payload,
        timeout: TIMEOUT.read(config, args)?,
        follow_redirects: FOLLOW_REDIRECTS.read(config, args)?,
        parse_as: PARSE_RESPONSE_AS.read(config, args)?,
    })
}

pub struct CoreHttpRunner {
    method: HttpMethod,
    settings: Arc<dyn SettingsRepo>,
    client: Arc<EgressClient>,
}

impl CoreHttpRunner {
    pub fn new(
        method: HttpMethod,
        settings: Arc<dyn SettingsRepo>,
        client: Arc<EgressClient>,
    ) -> Self {
        Self {
            method,
            settings,
            client,
        }
    }

    fn descriptor(&self) -> HttpDescriptor {
        match self.method {
            HttpMethod::Get => HttpDescriptor {
                id: "core.http.get",
                label: "HTTP GET",
                summary: "Fetch a URL; stores status, body and headers in `http.*`",
                search_text: "http get fetch request url api webhook rest download",
                has_body: false,
            },
            HttpMethod::Post => HttpDescriptor {
                id: "core.http.post",
                label: "HTTP POST",
                summary: "Send a POST request with a body; stores response in `http.*`",
                search_text: "http post request url api webhook rest send body",
                has_body: true,
            },
            HttpMethod::Put => HttpDescriptor {
                id: "core.http.put",
                label: "HTTP PUT",
                summary: "Send a PUT request with a body; stores response in `http.*`",
                search_text: "http put request url api webhook rest send body update",
                has_body: true,
            },
            HttpMethod::Patch => HttpDescriptor {
                id: "core.http.patch",
                label: "HTTP PATCH",
                summary: "Send a PATCH request with a body; stores response in `http.*`",
                search_text: "http patch request url api webhook rest send body modify",
                has_body: true,
            },
            HttpMethod::Delete => HttpDescriptor {
                id: "core.http.delete",
                label: "HTTP DELETE",
                summary: "Send a DELETE request; stores response in `http.*`",
                search_text: "http delete request url api webhook rest remove",
                has_body: true,
            },
        }
    }

    async fn allow_local(&self) -> bool {
        read_setting_or_default(self.settings.as_ref(), &settings::CORE_HTTP_ALLOW_LOCAL).await
    }

    async fn request(&self, call: HttpCall) -> EgressRequest {
        let HttpCall {
            url,
            mut headers,
            query_params,
            payload,
            timeout,
            follow_redirects,
            ..
        } = call;
        let has_explicit_content_type = headers
            .keys()
            .any(|key| key.eq_ignore_ascii_case(CONTENT_TYPE_HEADER));
        let (body, content_type) = match payload {
            Some((body, content_type)) => (
                Some(body),
                (!has_explicit_content_type).then(|| content_type.wire_name().to_owned()),
            ),
            None => (None, None),
        };
        if content_type.is_some() {
            headers.retain(|key, _| !key.eq_ignore_ascii_case(CONTENT_TYPE_HEADER));
        }
        EgressRequest {
            method: self.method,
            url,
            headers,
            query_params,
            body,
            content_type,
            timeout,
            follow_redirects,
            allow_local: self.allow_local().await,
        }
    }
}

struct HttpDescriptor {
    id: &'static str,
    label: &'static str,
    summary: &'static str,
    search_text: &'static str,
    has_body: bool,
}

#[async_trait]
impl SubActionRunner for CoreHttpRunner {
    fn id(&self) -> &str {
        self.descriptor().id
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Http
    }

    fn label(&self) -> &str {
        self.descriptor().label
    }

    fn summary(&self) -> &str {
        self.descriptor().summary
    }

    fn search_text(&self) -> &str {
        self.descriptor().search_text
    }

    fn icon_name(&self) -> &str {
        "globe"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        if self.descriptor().has_body {
            Some(WITH_BODY)
        } else {
            Some(WITHOUT_BODY)
        }
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            fixed_outputs: vec![
                HTTP_STATUS_CODE_VAR.to_owned(),
                HTTP_HEADERS_VAR.to_owned(),
                HTTP_BODY_VAR.to_owned(),
            ],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let descriptor = self.descriptor();
        let timer = StepTimer::start(ctx, descriptor.id);
        let call = match read_call(config, ctx.arg_stack, descriptor.has_body) {
            Ok(call) => call,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let parse_as = call.parse_as;
        let request = self.request(call).await;

        let (outcome, updated_stack) = match self.client.send(request).await {
            Ok(response) => {
                let stack = apply_response(ctx.arg_stack, response, parse_as);
                (SubActionOutcome::Success, Some(stack))
            }
            Err(e) => (SubActionOutcome::Failed(e.to_string()), None),
        };

        (timer.finish(outcome), updated_stack)
    }
}

fn interpolated_strings(
    field: &JsonObjectField,
    map: serde_json::Map<String, serde_json::Value>,
    arg_stack: &ArgStack,
) -> Result<BTreeMap<String, String>, ConfigProblem> {
    map.into_iter()
        .map(|(key, value)| {
            let text = match value {
                serde_json::Value::String(text) => text,
                serde_json::Value::Null => {
                    return Err(ConfigProblem::field(
                        field.key,
                        ConfigReason::InvalidFormat(format!("'{key}' has no value")),
                    ));
                }
                other => other.to_string(),
            };
            Ok((key, arg_stack.interpolate(&text)))
        })
        .collect()
}

fn apply_response(
    arg_stack: &ArgStack,
    response: EgressResponse,
    parse_as: ParseResponseAs,
) -> ArgStack {
    let header_map = response
        .headers
        .into_iter()
        .map(|(k, v)| (k, Variant::String(v)))
        .collect();

    let stack = arg_stack
        .clone()
        .set(
            HTTP_STATUS_CODE_VAR.to_owned(),
            Variant::Int(i64::from(response.status)),
        )
        .set(HTTP_HEADERS_VAR.to_owned(), Variant::Object(header_map));

    match parse_as {
        ParseResponseAs::Ignore => stack,
        ParseResponseAs::Json => {
            let parsed = serde_json::from_str::<serde_json::Value>(&response.body)
                .ok()
                .and_then(|v| Variant::from_json(v).ok());
            let body = parsed.unwrap_or(Variant::String(response.body));
            stack.set(HTTP_BODY_VAR.to_owned(), body)
        }
        ParseResponseAs::Text => {
            stack.set(HTTP_BODY_VAR.to_owned(), Variant::String(response.body))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{Sandboxed, sandboxed_backend};
    use forge_events::{Event, EventPublisher};
    use forge_registry::ConfigProblem;
    use forge_registry::ConfigReason;
    use forge_registry::RegistryError;
    use forge_storage::{SettingsRepo, reserved_keys};
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::EventId;
    use wiremock::matchers::{body_string, header, method, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    async fn backend() -> Sandboxed<Arc<SqliteBackend>> {
        sandboxed_backend([0x11; 32]).await.map(Arc::new)
    }

    fn runner(method: HttpMethod, dp: &Arc<SqliteBackend>) -> CoreHttpRunner {
        CoreHttpRunner::new(
            method,
            Arc::clone(dp) as Arc<dyn SettingsRepo>,
            Arc::new(EgressClient::new().unwrap()),
        )
    }

    async fn run(
        runner: &CoreHttpRunner,
        cfg: &SubActionConfig,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        runner.execute(cfg, &ctx).await
    }

    #[tokio::test]
    async fn get_success_writes_status_body_and_headers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-trace", "abc")
                    .set_body_string("body-text"),
            )
            .mount(&server)
            .await;
        let dp = backend().await;
        dp.set_string(reserved_keys::CORE_HTTP_ALLOW_LOCAL, "true")
            .await
            .unwrap();

        let mut cfg = SubActionConfig::new();
        cfg.insert("url".to_owned(), Variant::String(server.uri()));
        cfg.insert(
            "parse_response_as".to_owned(),
            Variant::String("text".to_owned()),
        );

        let (tel, stack) = run(&runner(HttpMethod::Get, &dp), &cfg).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        let stack = stack.unwrap();
        assert_eq!(
            stack.get("http.status_code").and_then(|v| v.as_int()),
            Some(200)
        );
        assert_eq!(
            stack.get("http.body").and_then(|v| v.as_str()),
            Some("body-text")
        );
        match stack.get("http.headers") {
            Some(Variant::Object(h)) => {
                assert_eq!(h.get("x-trace").and_then(|v| v.as_str()), Some("abc"));
            }
            other => panic!("expected http.headers object, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn post_success_marshals_body_and_writes_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_string("the-payload"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;
        let dp = backend().await;
        dp.set_string(reserved_keys::CORE_HTTP_ALLOW_LOCAL, "true")
            .await
            .unwrap();

        let mut cfg = SubActionConfig::new();
        cfg.insert("url".to_owned(), Variant::String(server.uri()));
        cfg.insert("body".to_owned(), Variant::String("the-payload".to_owned()));
        cfg.insert(
            "parse_response_as".to_owned(),
            Variant::String("ignore".to_owned()),
        );

        let (tel, stack) = run(&runner(HttpMethod::Post, &dp), &cfg).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(
            stack
                .unwrap()
                .get("http.status_code")
                .and_then(|v| v.as_int()),
            Some(201)
        );
    }

    #[tokio::test]
    async fn error_status_still_succeeds_and_writes_status_parsed_body_and_headers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(404)
                    .insert_header("x-trace", "missing-1")
                    .set_body_string(r#"{"error":"not found"}"#),
            )
            .mount(&server)
            .await;
        let dp = local_backend().await;
        let mut cfg = SubActionConfig::new();
        cfg.insert("url".to_owned(), Variant::String(server.uri()));

        let (tel, stack) = run(&runner(HttpMethod::Get, &dp), &cfg).await;

        assert_eq!(tel.outcome, SubActionOutcome::Success);
        let stack = stack.unwrap();
        assert_eq!(
            stack.get(HTTP_STATUS_CODE_VAR).and_then(|v| v.as_int()),
            Some(404)
        );
        match stack.get(HTTP_BODY_VAR) {
            Some(Variant::Object(body)) => {
                assert_eq!(
                    body.get("error").and_then(|v| v.as_str()),
                    Some("not found")
                );
            }
            other => panic!("expected a parsed json body, got {other:?}"),
        }
        match stack.get(HTTP_HEADERS_VAR) {
            Some(Variant::Object(headers)) => {
                assert_eq!(
                    headers.get("x-trace").and_then(|v| v.as_str()),
                    Some("missing-1")
                );
            }
            other => panic!("expected http.headers object, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn configured_content_type_is_sent_only_by_methods_that_carry_a_body() {
        for (http_method, expected) in [
            (HttpMethod::Get, None),
            (HttpMethod::Delete, Some("text/plain")),
            (HttpMethod::Post, Some("text/plain")),
            (HttpMethod::Put, Some("text/plain")),
        ] {
            let server = MockServer::start().await;
            Mock::given(wiremock::matchers::any())
                .respond_with(ResponseTemplate::new(200))
                .mount(&server)
                .await;
            let dp = local_backend().await;
            let mut cfg = url_cfg(server.uri());
            cfg.insert(
                "content_type".to_owned(),
                Variant::String("text/plain".to_owned()),
            );
            cfg.insert("body".to_owned(), Variant::String("<a/>".to_owned()));

            let (tel, _) = run(&runner(http_method, &dp), &cfg).await;

            assert_eq!(tel.outcome, SubActionOutcome::Success, "{http_method:?}");
            let seen = server.received_requests().await.unwrap();
            let sent = seen[0]
                .headers
                .get("content-type")
                .map(|value| value.to_str().unwrap().to_owned());
            assert_eq!(sent.as_deref(), expected, "{http_method:?}");
        }
    }

    #[tokio::test]
    async fn ssrf_rejected_request_fails_with_no_output_vars() {
        let dp = backend().await;
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "url".to_owned(),
            Variant::String("http://169.254.169.254/latest/meta-data/".to_owned()),
        );

        let (tel, stack) = run(&runner(HttpMethod::Get, &dp), &cfg).await;
        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
        assert!(
            stack.is_none(),
            "rejected request must publish no http.* vars"
        );
    }

    #[tokio::test]
    async fn failed_request_error_does_not_leak_token_header_or_query() {
        let dp = backend().await;
        let mut headers = BTreeMap::new();
        headers.insert(
            "Authorization".to_owned(),
            Variant::String("Bearer SECRET-TOKEN-XYZ".to_owned()),
        );
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "url".to_owned(),
            Variant::String("http://192.0.2.1/v1/resource?access_token=QUERY-SECRET".to_owned()),
        );
        cfg.insert("headers".to_owned(), Variant::Object(headers));
        cfg.insert("timeout_ms".to_owned(), Variant::Int(100));

        let (tel, stack) = run(&runner(HttpMethod::Get, &dp), &cfg).await;
        let SubActionOutcome::Failed(msg) = tel.outcome else {
            panic!("expected Failed, got {:?}", tel.outcome);
        };
        assert!(stack.is_none());
        assert!(
            !msg.contains("SECRET-TOKEN-XYZ"),
            "leaked auth header: {msg}"
        );
        assert!(!msg.contains("QUERY-SECRET"), "leaked query token: {msg}");
        assert!(!msg.contains("192.0.2.1"), "leaked target host: {msg}");
    }

    async fn local_backend() -> Sandboxed<Arc<SqliteBackend>> {
        let dp = backend().await;
        dp.set_string(reserved_keys::CORE_HTTP_ALLOW_LOCAL, "true")
            .await
            .unwrap();
        dp
    }

    fn url_cfg(url: String) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("url".to_owned(), Variant::String(url));
        cfg.insert(
            "parse_response_as".to_owned(),
            Variant::String("ignore".to_owned()),
        );
        cfg
    }

    #[tokio::test]
    async fn string_json_headers_and_query_params_reach_the_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("x-key", "v1"))
            .and(query_param("q", "2"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let dp = local_backend().await;
        let mut cfg = url_cfg(server.uri());
        cfg.insert(
            HEADERS.key.to_owned(),
            Variant::String(r#"{"x-key": "v1"}"#.to_owned()),
        );
        cfg.insert(
            QUERY_PARAMS.key.to_owned(),
            Variant::String(r#"{"q": 2}"#.to_owned()),
        );

        let (_, stack) = run(&runner(HttpMethod::Get, &dp), &cfg).await;
        assert_eq!(
            stack.and_then(|s| s.get(HTTP_STATUS_CODE_VAR).and_then(|v| v.as_int())),
            Some(200)
        );
    }

    #[tokio::test]
    async fn invalid_headers_or_query_params_fail_without_sending_a_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let dp = local_backend().await;
        for (key, value) in [
            (HEADERS.key, "object{0}"),
            (HEADERS.key, "null"),
            (QUERY_PARAMS.key, "[1]"),
        ] {
            let mut cfg = url_cfg(server.uri());
            cfg.insert(key.to_owned(), Variant::String(value.to_owned()));
            let (tel, stack) = run(&runner(HttpMethod::Get, &dp), &cfg).await;
            assert!(
                matches!(&tel.outcome, SubActionOutcome::Failed(m) if m.contains(&format!("'{key}'"))),
                "{key}={value}: {:?}",
                tel.outcome
            );
            assert!(stack.is_none(), "{key}={value}");
        }
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_null_header_or_query_param_value_fails_instead_of_sending_the_text_null() {
        for key in [HEADERS.key, QUERY_PARAMS.key] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(200))
                .mount(&server)
                .await;
            let dp = local_backend().await;
            let mut cfg = url_cfg(server.uri());
            cfg.insert(key.to_owned(), Variant::String(r#"{"a": null}"#.to_owned()));

            let (tel, _) = run(&runner(HttpMethod::Get, &dp), &cfg).await;

            assert!(
                matches!(&tel.outcome, SubActionOutcome::Failed(m) if m.contains(&format!("'{key}'"))),
                "{key}: {:?}",
                tel.outcome
            );
            assert!(
                server.received_requests().await.unwrap().is_empty(),
                "{key}"
            );
        }
    }

    #[tokio::test]
    async fn validate_config_rejects_non_object_headers_and_query_params() {
        let dp = backend().await;
        for key in [HEADERS.key, QUERY_PARAMS.key] {
            let mut cfg = url_cfg("https://example.com".to_owned());
            cfg.insert(key.to_owned(), Variant::String("object{0}".to_owned()));
            let err = runner(HttpMethod::Get, &dp)
                .validate_config(&cfg)
                .unwrap_err();
            assert!(
                matches!(
                    &err,
                    RegistryError::InvalidConfig(ConfigProblem {
                        field: Some(field),
                        reason: ConfigReason::InvalidFormat(_),
                    }) if field == key
                ),
                "{key}: {err:?}"
            );
        }
    }
}
