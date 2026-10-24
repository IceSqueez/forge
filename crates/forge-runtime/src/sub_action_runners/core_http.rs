use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_registry::{
    ConfigReason, DurationBounds, FormField, RegistryError, RunContext, StepTimer,
    SubActionCategory, SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_storage::{SettingsRepo, get_bool_setting, reserved_keys};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

use super::json_object_input::{EMPTY_JSON_OBJECT, read_json_object, validate_json_object};
use crate::egress::{EgressClient, EgressRequest, EgressResponse, HttpMethod};

const TIMEOUT_MIN_MS: i64 = 100;
const TIMEOUT_MAX_MS: i64 = 60_000;

const HEADERS_KEY: &str = "headers";
const QUERY_PARAMS_KEY: &str = "query_params";

const HTTP_STATUS_CODE_VAR: &str = "http.status_code";
const HTTP_HEADERS_VAR: &str = "http.headers";
const HTTP_BODY_VAR: &str = "http.body";

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
        get_bool_setting(
            self.settings.as_ref(),
            reserved_keys::CORE_HTTP_ALLOW_LOCAL,
            false,
        )
        .await
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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("url".to_owned(), Variant::String(String::new()));
        cfg.insert(
            HEADERS_KEY.to_owned(),
            Variant::String(EMPTY_JSON_OBJECT.to_owned()),
        );
        cfg.insert(
            QUERY_PARAMS_KEY.to_owned(),
            Variant::String(EMPTY_JSON_OBJECT.to_owned()),
        );
        cfg.insert("timeout_ms".to_owned(), Variant::Int(10_000));
        cfg.insert("follow_redirects".to_owned(), Variant::Bool(true));
        cfg.insert(
            "parse_response_as".to_owned(),
            Variant::String("json".to_owned()),
        );
        if self.descriptor().has_body {
            cfg.insert("body".to_owned(), Variant::String(String::new()));
            cfg.insert(
                "content_type".to_owned(),
                Variant::String("application/json".to_owned()),
            );
        }
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        let mut fields = vec![FormField::Text {
            key: "url",
            label: "URL",
            placeholder: "https://api.example.com/endpoint",
        }];
        if self.descriptor().has_body {
            fields.push(FormField::TextArea {
                key: "body",
                label: "Body",
            });
            fields.push(FormField::Select {
                key: "content_type",
                label: "Content Type",
                options: &[
                    "application/json",
                    "application/x-www-form-urlencoded",
                    "text/plain",
                    "multipart/form-data",
                ],
            });
        }
        fields.push(FormField::TextArea {
            key: HEADERS_KEY,
            label: "Headers (JSON object)",
        });
        fields.push(FormField::TextArea {
            key: QUERY_PARAMS_KEY,
            label: "Query Parameters (JSON object)",
        });
        fields.push(FormField::Duration {
            key: "timeout_ms",
            label: "Timeout",
            bounds: DurationBounds::in_millis(TIMEOUT_MIN_MS, TIMEOUT_MAX_MS),
        });
        fields.push(FormField::Toggle {
            key: "follow_redirects",
            label: "Follow Redirects",
        });
        fields.push(FormField::Select {
            key: "parse_response_as",
            label: "Parse Response As",
            options: &["text", "json", "ignore"],
        });
        fields
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let url_ok = config.str("url").is_some_and(|s| !s.trim().is_empty());
        if !url_ok {
            return Err(RegistryError::invalid_field("url", ConfigReason::Required));
        }
        validate_json_object(config, HEADERS_KEY)?;
        validate_json_object(config, QUERY_PARAMS_KEY)
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

        let url_template = config.str("url").unwrap_or_default();
        let url = ctx.arg_stack.interpolate(url_template);

        let (mut headers, query_params) = match (
            config_string_map(config, HEADERS_KEY),
            config_string_map(config, QUERY_PARAMS_KEY),
        ) {
            (Ok(headers), Ok(query_params)) => (
                interpolate_map(headers, ctx.arg_stack),
                interpolate_map(query_params, ctx.arg_stack),
            ),
            (Err(reason), _) | (_, Err(reason)) => return (timer.failed(reason), None),
        };

        let timeout_ms = config
            .int("timeout_ms")
            .unwrap_or(10_000)
            .clamp(TIMEOUT_MIN_MS, TIMEOUT_MAX_MS) as u64;
        let follow_redirects = config.bool("follow_redirects").unwrap_or(true);
        let parse_as = config.str("parse_response_as").unwrap_or("json").to_owned();

        let (body, content_type) = if descriptor.has_body {
            let body_template = config.str("body").unwrap_or_default();
            let body = ctx.arg_stack.interpolate(body_template);
            let has_explicit_content_type = headers
                .keys()
                .any(|k| k.eq_ignore_ascii_case("content-type"));
            let content_type = config
                .str("content_type")
                .filter(|s| !s.is_empty() && !has_explicit_content_type)
                .map(|s| s.to_owned());
            (Some(body), content_type)
        } else {
            (None, None)
        };

        if content_type.is_some() {
            headers.retain(|k, _| !k.eq_ignore_ascii_case("content-type"));
        }

        let request = EgressRequest {
            method: self.method,
            url,
            headers,
            query_params,
            body,
            content_type,
            timeout: Duration::from_millis(timeout_ms),
            follow_redirects,
            allow_local: self.allow_local().await,
        };

        let (outcome, updated_stack) = match self.client.send(request).await {
            Ok(response) => {
                let stack = apply_response(ctx.arg_stack, response, &parse_as);
                (SubActionOutcome::Success, Some(stack))
            }
            Err(e) => (SubActionOutcome::Failed(e.to_string()), None),
        };

        (timer.finish(outcome), updated_stack)
    }
}

fn interpolate_map(
    raw: BTreeMap<String, String>,
    arg_stack: &ArgStack,
) -> BTreeMap<String, String> {
    raw.into_iter()
        .map(|(key, value)| (key, arg_stack.interpolate(&value)))
        .collect()
}

fn config_string_map(
    config: &SubActionConfig,
    key: &str,
) -> Result<BTreeMap<String, String>, String> {
    read_json_object(config, key).map(|map| {
        map.iter()
            .map(|(k, v)| (k.clone(), variant_plain_string(v)))
            .collect()
    })
}

fn variant_plain_string(value: &Variant) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn apply_response(arg_stack: &ArgStack, response: EgressResponse, parse_as: &str) -> ArgStack {
    let header_map = response
        .headers
        .into_iter()
        .map(|(k, v)| (k, Variant::String(v)))
        .collect();

    let stack = arg_stack
        .clone()
        .set(
            HTTP_STATUS_CODE_VAR.to_owned(),
            Variant::Int(response.status as i64),
        )
        .set(HTTP_HEADERS_VAR.to_owned(), Variant::Object(header_map));

    match parse_as {
        "ignore" => stack,
        "json" => {
            let parsed = serde_json::from_str::<serde_json::Value>(&response.body)
                .ok()
                .and_then(|v| Variant::from_json(v).ok());
            let body = parsed.unwrap_or(Variant::String(response.body));
            stack.set(HTTP_BODY_VAR.to_owned(), body)
        }
        _ => stack.set(HTTP_BODY_VAR.to_owned(), Variant::String(response.body)),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::test_support::{Sandboxed, sandboxed_backend};
    use forge_events::{Event, EventPublisher};
    use forge_registry::ConfigProblem;
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
            HEADERS_KEY.to_owned(),
            Variant::String(r#"{"x-key": "v1"}"#.to_owned()),
        );
        cfg.insert(
            QUERY_PARAMS_KEY.to_owned(),
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
            (HEADERS_KEY, "object{0}"),
            (HEADERS_KEY, "null"),
            (QUERY_PARAMS_KEY, "[1]"),
            (QUERY_PARAMS_KEY, r#"{"a": null}"#),
        ] {
            let mut cfg = url_cfg(server.uri());
            cfg.insert(key.to_owned(), Variant::String(value.to_owned()));
            let (tel, stack) = run(&runner(HttpMethod::Get, &dp), &cfg).await;
            assert!(
                matches!(&tel.outcome, SubActionOutcome::Failed(m) if m.starts_with(key)),
                "{key}={value}: {:?}",
                tel.outcome
            );
            assert!(stack.is_none(), "{key}={value}");
        }
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn validate_config_rejects_non_object_headers_and_query_params() {
        let dp = backend().await;
        for key in [HEADERS_KEY, QUERY_PARAMS_KEY] {
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
