use super::*;

pub(super) fn publish_http_call(
    publisher: &dyn EventPublisher,
    method: ScriptHttpMethod,
    response: &HttpResponse,
    caused_by: EventId,
) {
    ScriptHttpCall {
        method,
        url_normalized: response.url_normalized.clone(),
        status: response.status,
        duration_ms: response.duration_ms,
        truncated: response.truncated,
    }
    .publish(publisher, EventSource::Rhai, Some(caused_by));
}

pub(super) fn build_http_module(
    client: Option<Arc<ScriptHttpClient>>,
    publisher: Arc<dyn EventPublisher>,
    caused_by: EventId,
) -> ApiModule {
    let mut m = ApiModule::default();
    let counter = Arc::new(AtomicU32::new(0));

    {
        let client = client.clone();
        let counter = Arc::clone(&counter);
        let publisher = Arc::clone(&publisher);
        m.register(
            ScriptFn {
                name: "get",
                params: ["url"],
                doc: None,
            },
            move |url: ImmutableString| -> Result<rhai::Map, Box<EvalAltResult>> {
                let client = available(&client, HTTP_UNAVAILABLE_REASON)?;
                let result = client.get(url.as_str(), &counter);
                if let Ok(ref resp) = result {
                    publish_http_call(publisher.as_ref(), ScriptHttpMethod::Get, resp, caused_by);
                }
                result
                    .map(http_response_to_rhai_map)
                    .map_err(http_error_to_rhai_error)
            },
        );
    }

    {
        let publisher = Arc::clone(&publisher);
        m.register(
            ScriptFn {
                name: "post",
                params: ["url", "body"],
                doc: None,
            },
            move |url: ImmutableString,
                  body: ImmutableString|
                  -> Result<rhai::Map, Box<EvalAltResult>> {
                let client = available(&client, HTTP_UNAVAILABLE_REASON)?;
                let result = client.post(url.as_str(), body.as_str(), &counter);
                if let Ok(ref resp) = result {
                    publish_http_call(publisher.as_ref(), ScriptHttpMethod::Post, resp, caused_by);
                }
                result
                    .map(http_response_to_rhai_map)
                    .map_err(http_error_to_rhai_error)
            },
        );
    }

    m
}

pub(super) fn http_response_to_rhai_map(resp: HttpResponse) -> rhai::Map {
    let mut map = rhai::Map::new();
    map.insert("status".into(), rhai::Dynamic::from(i64::from(resp.status)));
    map.insert("body".into(), rhai::Dynamic::from(resp.body));
    map.insert("truncated".into(), rhai::Dynamic::from(resp.truncated));
    map.insert(
        "duration_ms".into(),
        rhai::Dynamic::from(i64::try_from(resp.duration_ms).unwrap_or(i64::MAX)),
    );
    let mut headers_map = rhai::Map::new();
    for (k, v) in resp.headers {
        headers_map.insert(k.into(), rhai::Dynamic::from(v));
    }
    map.insert("headers".into(), rhai::Dynamic::from(headers_map));
    map
}

pub(super) fn http_error_to_rhai_error(e: ScriptError) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        e.to_string().into(),
        Position::NONE,
    ))
}
