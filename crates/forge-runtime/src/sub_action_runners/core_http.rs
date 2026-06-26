use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::{GlobalsRepo, SettingsRepo, reserved_keys};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::egress::{EgressClient, EgressRequest, EgressResponse, HttpMethod};

pub struct CoreHttpRunner {
    method: HttpMethod,
    globals: Arc<dyn GlobalsRepo>,
    settings: Arc<dyn SettingsRepo>,
    client: Arc<EgressClient>,
}

impl CoreHttpRunner {
    pub fn new(
        method: HttpMethod,
        globals: Arc<dyn GlobalsRepo>,
        settings: Arc<dyn SettingsRepo>,
        client: Arc<EgressClient>,
    ) -> Self {
        Self {
            method,
            globals,
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

    async fn interpolate(&self, template: &str, arg_stack: &ArgStack) -> String {
        super::interpolate::interpolate_with_globals(template, arg_stack, self.globals.as_ref())
            .await
    }

    async fn interpolate_map(
        &self,
        raw: BTreeMap<String, String>,
        arg_stack: &ArgStack,
    ) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for (key, value) in raw {
            let resolved = self.interpolate(&value, arg_stack).await;
            out.insert(key, resolved);
        }
        out
    }

    async fn allow_local(&self) -> bool {
        self.settings
            .get_string(reserved_keys::CORE_HTTP_ALLOW_LOCAL_KEY)
            .await
            .ok()
            .flatten()
            .map(|s| s.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
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
        cfg.insert("headers".to_owned(), Variant::Object(BTreeMap::new()));
        cfg.insert("query_params".to_owned(), Variant::Object(BTreeMap::new()));
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
            key: "headers",
            label: "Headers (JSON object)",
        });
        fields.push(FormField::TextArea {
            key: "query_params",
            label: "Query Parameters (JSON object)",
        });
        fields.push(FormField::Integer {
            key: "timeout_ms",
            label: "Timeout (ms)",
            min: 100,
            max: 60_000,
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
        let url_ok = config
            .get("url")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.trim().is_empty());
        if url_ok {
            Ok(())
        } else {
            Err(RegistryError::UnknownKindId(format!(
                "{}: url is required",
                self.descriptor().id
            )))
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let descriptor = self.descriptor();

        let url_template = config
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let url = self.interpolate(url_template, ctx.arg_stack).await;

        let mut headers = self
            .interpolate_map(config_string_map(config, "headers"), ctx.arg_stack)
            .await;
        let query_params = self
            .interpolate_map(config_string_map(config, "query_params"), ctx.arg_stack)
            .await;

        let timeout_ms = config
            .get("timeout_ms")
            .and_then(|v| v.as_int())
            .unwrap_or(10_000)
            .clamp(100, 60_000) as u64;
        let follow_redirects = config
            .get("follow_redirects")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let parse_as = config
            .get("parse_response_as")
            .and_then(|v| v.as_str())
            .unwrap_or("json")
            .to_owned();

        let (body, content_type) = if descriptor.has_body {
            let body_template = config
                .get("body")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let body = self.interpolate(body_template, ctx.arg_stack).await;
            let has_explicit_content_type = headers
                .keys()
                .any(|k| k.eq_ignore_ascii_case("content-type"));
            let content_type = config
                .get("content_type")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty() && !has_explicit_content_type)
                .map(|s| s.to_owned());
            (Some(body), content_type)
        } else {
            (None, None)
        };

        // Strip any caller-supplied Content-Type header once it has been promoted to
        // the dedicated field so the request never carries it twice.
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

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: descriptor.id.to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            updated_stack,
        )
    }
}

fn config_string_map(config: &SubActionConfig, key: &str) -> BTreeMap<String, String> {
    match config.get(key) {
        Some(Variant::Object(map)) => map
            .iter()
            .map(|(k, v)| (k.clone(), variant_plain_string(v)))
            .collect(),
        Some(Variant::String(s)) if !s.trim().is_empty() => {
            serde_json::from_str::<serde_json::Value>(s)
                .ok()
                .and_then(|v| match v {
                    serde_json::Value::Object(obj) => Some(
                        obj.iter()
                            .map(|(k, val)| (k.clone(), json_plain_string(val)))
                            .collect(),
                    ),
                    _ => None,
                })
                .unwrap_or_default()
        }
        _ => BTreeMap::new(),
    }
}

fn variant_plain_string(value: &Variant) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn json_plain_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
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
            "http.status_code".to_owned(),
            Variant::Int(response.status as i64),
        )
        .set("http.headers".to_owned(), Variant::Object(header_map));

    match parse_as {
        "ignore" => stack,
        "json" => {
            let parsed = serde_json::from_str::<serde_json::Value>(&response.body)
                .ok()
                .and_then(|v| Variant::from_json(v).ok());
            let body = parsed.unwrap_or(Variant::String(response.body));
            stack.set("http.body".to_owned(), body)
        }
        _ => stack.set("http.body".to_owned(), Variant::String(response.body)),
    }
}
