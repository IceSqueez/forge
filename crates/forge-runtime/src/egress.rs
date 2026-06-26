use std::collections::BTreeMap;
use std::net::IpAddr;
use std::time::Duration;

use forge_platform_core::is_private_or_special;
use tokio::sync::Semaphore;

const MAX_RESPONSE_BYTES: usize = 1_048_576;
const MAX_REDIRECT_HOPS: u32 = 10;
const MAX_CONCURRENT_REQUESTS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

pub struct EgressRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub query_params: BTreeMap<String, String>,
    pub body: Option<String>,
    pub content_type: Option<String>,
    pub timeout: Duration,
    pub follow_redirects: bool,
    /// When false (default) the SSRF denylist rejects loopback/private/link-local
    /// targets; when true the denylist is bypassed so LAN/localhost URLs reach the
    /// network. Read per-execution from settings so the toggle is hot.
    pub allow_local: bool,
}

pub struct EgressResponse {
    pub status: u16,
    pub body: String,
    pub headers: BTreeMap<String, String>,
    pub truncated: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum EgressError {
    #[error("invalid url")]
    InvalidUrl,
    #[error("only http and https urls are allowed")]
    SchemeNotAllowed,
    #[error("blocked: target resolves to a private or local address")]
    BlockedAddress,
    #[error("host did not resolve")]
    UnresolvableHost,
    #[error("too many redirects")]
    TooManyRedirects,
    #[error("request timed out")]
    Timeout,
    #[error("network error: {0}")]
    Network(String),
}

/// Cross-cutting HTTP egress for `core.http.*` sub-actions. Owns its own SSRF
/// denylist and per-request budget instead of routing through the platform
/// `TokenBucketRateLimiter`: that limiter's buckets are keyed to documented
/// per-platform API quotas, and an arbitrary user-authored URL maps to no
/// platform bucket. The global semaphore caps concurrent in-flight egress
/// requests across the whole runtime.
pub struct EgressClient {
    client: reqwest::Client,
    concurrency: Semaphore,
}

impl EgressClient {
    pub fn new() -> Result<Self, EgressError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("forge/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| EgressError::Network(e.without_url().to_string()))?;
        Ok(Self {
            client,
            concurrency: Semaphore::new(MAX_CONCURRENT_REQUESTS),
        })
    }

    /// Validates the URL against the SSRF denylist, sends it, and follows
    /// redirects manually so every hop is re-validated against the same
    /// classifier. The host's resolved IPs are re-checked post-resolution before
    /// each request leaves — a literal IP or any DNS result inside private space
    /// is rejected unless `allow_local` is set.
    pub async fn send(&self, req: EgressRequest) -> Result<EgressResponse, EgressError> {
        let _permit = self
            .concurrency
            .acquire()
            .await
            .map_err(|_| EgressError::Network("egress pool closed".to_owned()))?;

        let mut url = reqwest::Url::parse(&req.url).map_err(|_| EgressError::InvalidUrl)?;
        if !req.query_params.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (k, v) in &req.query_params {
                pairs.append_pair(k, v);
            }
        }

        let mut method = req.method;
        let mut body = req.body.clone();
        let mut hops = 0u32;

        loop {
            validate_url(&url, req.allow_local).await?;
            let response = self.dispatch(&url, method, &req, body.as_deref()).await?;
            let status = response.status();

            if req.follow_redirects && status.is_redirection() {
                if hops >= MAX_REDIRECT_HOPS {
                    return Err(EgressError::TooManyRedirects);
                }
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok());
                let Some(location) = location else {
                    return read_response(response).await;
                };
                let next = url.join(location).map_err(|_| EgressError::InvalidUrl)?;
                if matches!(status.as_u16(), 301..=303) {
                    method = HttpMethod::Get;
                    body = None;
                }
                url = next;
                hops += 1;
                continue;
            }

            return read_response(response).await;
        }
    }

    async fn dispatch(
        &self,
        url: &reqwest::Url,
        method: HttpMethod,
        req: &EgressRequest,
        body: Option<&str>,
    ) -> Result<reqwest::Response, EgressError> {
        let mut builder = self
            .client
            .request(to_reqwest_method(method), url.clone())
            .timeout(req.timeout);
        for (k, v) in &req.headers {
            builder = builder.header(k, v);
        }
        if let Some(content_type) = &req.content_type {
            builder = builder.header(reqwest::header::CONTENT_TYPE, content_type);
        }
        if let Some(body) = body {
            builder = builder.body(body.to_owned());
        }
        builder.send().await.map_err(map_send_error)
    }
}

async fn validate_url(url: &reqwest::Url, allow_local: bool) -> Result<(), EgressError> {
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err(EgressError::SchemeNotAllowed),
    }
    if allow_local {
        return Ok(());
    }
    let host = url.host_str().ok_or(EgressError::InvalidUrl)?;
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_or_special(ip) {
            return Err(EgressError::BlockedAddress);
        }
        return Ok(());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let resolved = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| EgressError::UnresolvableHost)?;
    let mut any = false;
    for addr in resolved {
        any = true;
        if is_private_or_special(addr.ip()) {
            return Err(EgressError::BlockedAddress);
        }
    }
    if any {
        Ok(())
    } else {
        Err(EgressError::UnresolvableHost)
    }
}

async fn read_response(response: reqwest::Response) -> Result<EgressResponse, EgressError> {
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_owned(), v.to_owned()))
        })
        .collect();

    let mut response = response;
    let mut buf: Vec<u8> = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = response.chunk().await.map_err(map_send_error)? {
        if buf.len() + chunk.len() > MAX_RESPONSE_BYTES {
            let remaining = MAX_RESPONSE_BYTES - buf.len();
            buf.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }
        buf.extend_from_slice(&chunk);
    }

    Ok(EgressResponse {
        status,
        body: String::from_utf8_lossy(&buf).into_owned(),
        headers,
        truncated,
    })
}

fn map_send_error(e: reqwest::Error) -> EgressError {
    if e.is_timeout() {
        EgressError::Timeout
    } else {
        EgressError::Network(e.without_url().to_string())
    }
}

fn to_reqwest_method(method: HttpMethod) -> reqwest::Method {
    match method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Post => reqwest::Method::POST,
        HttpMethod::Put => reqwest::Method::PUT,
        HttpMethod::Patch => reqwest::Method::PATCH,
        HttpMethod::Delete => reqwest::Method::DELETE,
    }
}
