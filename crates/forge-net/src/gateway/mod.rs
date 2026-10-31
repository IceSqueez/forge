mod call_class;
mod classify;
mod expected_refusal;
mod failure;
mod observer;
mod profile;
mod request;
mod resolved_target;
mod response;
mod response_body;
mod streaming;
mod target;
mod timings;
mod user_directed;

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use reqwest::header::{
    AUTHORIZATION, CONTENT_TYPE, COOKIE, HeaderMap, HeaderName, HeaderValue, LOCATION,
    PROXY_AUTHORIZATION,
};
use reqwest::multipart::{Form, Part};
use reqwest::{StatusCode, Url};

pub use call_class::CallClass;
pub use expected_refusal::ExpectedRefusal;
pub use failure::{
    BodyExcerpt, GatewayFailure, InvalidRequest, RedirectRefusal, TransportFailure, TransportKind,
};
pub use observer::{FailureContext, FailureObserver};
pub use profile::{EgressKind, GatewayProfile};
pub use request::{
    Credential, FormBody, GatewayRequest, Method, MultipartBody, RequestBody, RequestEncodeError,
};
pub use resolved_target::ResolvedTarget;
pub use response::{DecodeCategory, GatewayResponse, ResponseDecodeError};
pub use streaming::{StreamFailure, StreamedResponse};
pub use target::GatewayBuildError;
pub use timings::GatewayTimings;

use crate::address_guard::{PublicAddressResolver, host_rejection_cause};
use crate::body_read::{BodySource, CappedReadError, TruncatedBody, read_capped, read_truncated};
use crate::rate_limit::{
    RateLimitUsage, RateLimiter, acquire_or_wait_within, acquire_without_wait,
};
use classify::{ERROR_BODY_READ_CAP, excerpt_of, status_failure, throttle_for};
use request::{BodyRepr, FieldValue, PartContent, Target};
use response_body::ResponseBody;

const USER_AGENT: &str = concat!("forge/", env!("CARGO_PKG_VERSION"));
const JSON_CONTENT_TYPE: &str = "application/json";
const BEARER_PREFIX: &str = "Bearer ";
const MAX_REDIRECT_HOPS: usize = 10;
const TOTAL_BUDGET_ELAPSED: &str = "total time budget elapsed";
const HTTP_SCHEME: &str = "http";
const HTTPS_SCHEME: &str = "https";
const HEADERS_IDLE_ELAPSED: &str = "no response within the read-idle time budget";

#[derive(Clone)]
pub struct Gateway {
    inner: Arc<Inner>,
}

struct Inner {
    client: reqwest::Client,
    streaming_client: Mutex<Option<reqwest::Client>>,
    base: Option<Url>,
    limiter: Arc<dyn RateLimiter>,
    profile: GatewayProfile,
    observer: Option<Arc<dyn FailureObserver>>,
}

struct Hop {
    url: Url,
    method: Method,
    keep_body: bool,
    forward_request_headers: bool,
    apply_query: bool,
}

impl Gateway {
    pub fn new(
        base_url: &str,
        limiter: Arc<dyn RateLimiter>,
        profile: GatewayProfile,
        observer: Option<Arc<dyn FailureObserver>>,
    ) -> Result<Self, GatewayBuildError> {
        let base = target::validate_base(base_url, &profile)?;
        Self::build(Some(base), limiter, profile, observer)
    }

    pub fn without_base(
        limiter: Arc<dyn RateLimiter>,
        profile: GatewayProfile,
        observer: Option<Arc<dyn FailureObserver>>,
    ) -> Result<Self, GatewayBuildError> {
        if !profile.accepts_absolute_urls() {
            return Err(GatewayBuildError::BaseRequired);
        }
        Self::build(None, limiter, profile, observer)
    }

    fn build(
        base: Option<Url>,
        limiter: Arc<dyn RateLimiter>,
        profile: GatewayProfile,
        observer: Option<Arc<dyn FailureObserver>>,
    ) -> Result<Self, GatewayBuildError> {
        let client = build_client(&profile, profile.default_class.connect_budget())
            .map_err(|failure| GatewayBuildError::ClientUnavailable(failure.cause().to_owned()))?;
        Ok(Self {
            inner: Arc::new(Inner {
                client,
                streaming_client: Mutex::new(None),
                base,
                limiter,
                profile,
                observer,
            }),
        })
    }

    pub fn profile(&self) -> GatewayProfile {
        self.inner.profile
    }

    pub fn limiter_usage(&self) -> RateLimitUsage {
        self.inner.limiter.usage()
    }

    pub fn resolve_path(&self, path: &str) -> Result<ResolvedTarget, GatewayFailure> {
        target::resolve_path(self.inner.base.as_ref(), path)
            .map(|url| ResolvedTarget::from_url(&url))
    }

    pub async fn send(
        &self,
        mut request: GatewayRequest,
    ) -> Result<GatewayResponse, GatewayFailure> {
        let context = self.failure_context(&request);
        let class = context.class;
        let outcome = match request.cancel.take() {
            Some(token) => tokio::select! {
                biased;
                () = token.cancelled() => Err(GatewayFailure::Cancelled),
                outcome = self.execute(&request, class) => outcome,
            },
            None => self.execute(&request, class).await,
        };
        if let Err(failure) = &outcome {
            self.report(&request, context, failure);
        }
        outcome
    }

    fn failure_context(&self, request: &GatewayRequest) -> FailureContext {
        FailureContext {
            method: request.method,
            class: request.class.unwrap_or(self.inner.profile.default_class),
            label: request.label,
        }
    }

    fn report(&self, request: &GatewayRequest, context: FailureContext, failure: &GatewayFailure) {
        if matches!(failure, GatewayFailure::Cancelled)
            || request
                .expected_refusals
                .iter()
                .any(|refusal| refusal.matches(failure))
        {
            return;
        }
        if let Some(observer) = &self.inner.observer {
            observer.on_failure(failure, context);
        }
    }

    async fn execute(
        &self,
        request: &GatewayRequest,
        class: CallClass,
    ) -> Result<GatewayResponse, GatewayFailure> {
        let client = &self.inner.client;
        let first = self.first_hop(request)?;
        let first_builder = self.build_hop(client, request, &first)?;
        self.acquire_budget(request).await?;
        let budget = request
            .total_budget
            .unwrap_or(self.inner.profile.timings.total_budget(class));
        tokio::time::timeout(budget, async {
            let response = self
                .follow(client, request, first, first_builder, None)
                .await?;
            self.finish(request, class, response).await
        })
        .await
        .unwrap_or_else(|_| {
            Err(GatewayFailure::Transport(TransportFailure::new(
                TransportKind::Timeout,
                TOTAL_BUDGET_ELAPSED,
            )))
        })
    }

    async fn acquire_budget(&self, request: &GatewayRequest) -> Result<(), GatewayFailure> {
        let limiter = self.inner.limiter.as_ref();
        let refused = if request.wait_for_limiter {
            acquire_or_wait_within(
                limiter,
                request.weight,
                self.inner.profile.timings.max_throttle_wait,
            )
            .await
            .map_err(|_| None)
        } else {
            acquire_without_wait(limiter, request.weight).await
        };
        refused.map_err(|retry_after| GatewayFailure::BudgetExhausted { retry_after })
    }

    async fn follow(
        &self,
        client: &reqwest::Client,
        request: &GatewayRequest,
        first: Hop,
        first_builder: reqwest::RequestBuilder,
        idle_budget: Option<Duration>,
    ) -> Result<reqwest::Response, GatewayFailure> {
        let mut hop = first;
        let mut builder = first_builder;
        let mut followed = 0_usize;
        loop {
            let response = match idle_budget {
                Some(budget) => {
                    tokio::time::timeout(budget, builder.send())
                        .await
                        .map_err(|_| {
                            GatewayFailure::Transport(TransportFailure::new(
                                TransportKind::Timeout,
                                HEADERS_IDLE_ELAPSED,
                            ))
                        })?
                }
                None => builder.send().await,
            }
            .map_err(send_failure)?;
            let next = if request.follow_redirects {
                self.redirect_hop(&hop, &response)?
            } else {
                None
            };
            let Some(next) = next else {
                return Ok(response);
            };
            followed += 1;
            if followed > MAX_REDIRECT_HOPS {
                return Err(redirect_refused(RedirectRefusal::TooManyHops));
            }
            builder = self.build_hop(client, request, &next)?;
            hop = next;
        }
    }

    async fn finish(
        &self,
        request: &GatewayRequest,
        class: CallClass,
        response: reqwest::Response,
    ) -> Result<GatewayResponse, GatewayFailure> {
        let status = response.status();
        let headers = collect_headers(&response);
        if status.is_success() || request.any_status_as_data {
            let effective_cap = request
                .body_cap
                .map_or(class.body_cap(), |cap| cap.min(class.body_cap()));
            let body = ResponseBody::new(response);
            let TruncatedBody { bytes, truncated } = if request.truncate_oversized_body {
                read_truncated(body, effective_cap)
                    .await
                    .map_err(send_failure)?
            } else {
                let bytes =
                    read_capped(body, effective_cap)
                        .await
                        .map_err(|error| match error {
                            CappedReadError::TooLarge { cap } => {
                                GatewayFailure::ResponseTooLarge { cap }
                            }
                            CappedReadError::Source(error) => send_failure(error),
                        })?;
                TruncatedBody {
                    bytes,
                    truncated: false,
                }
            };
            return Ok(GatewayResponse::new(
                status.as_u16(),
                headers,
                bytes,
                truncated,
            ));
        }
        Err(self
            .refusal(request, status, &headers, ResponseBody::new(response))
            .await)
    }

    async fn refusal<B: BodySource>(
        &self,
        request: &GatewayRequest,
        status: StatusCode,
        headers: &[(String, String)],
        body: B,
    ) -> GatewayFailure {
        let throttle = throttle_for(status, headers, &self.inner.profile, SystemTime::now());
        if let Some(throttle) = throttle {
            self.inner
                .limiter
                .observe_remote_throttle(throttle.wait)
                .await;
        }
        let body = read_capped(body, ERROR_BODY_READ_CAP)
            .await
            .unwrap_or_default();
        let excerpt = excerpt_of(&body, &request.secrets());
        if request.class.unwrap_or(self.inner.profile.default_class) != CallClass::OAuth {
            tracing::debug!(
                status = status.as_u16(),
                body = excerpt.as_str(),
                "upstream refused the request"
            );
        }
        match throttle {
            Some(throttle) => GatewayFailure::RateLimited {
                status: status.as_u16(),
                retry_after: throttle.wait,
                scope: throttle.scope,
                excerpt,
            },
            None => status_failure(status, excerpt),
        }
    }

    fn streaming_client(&self) -> Result<reqwest::Client, GatewayFailure> {
        let mut slot = self
            .inner
            .streaming_client
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(client) = slot.as_ref() {
            return Ok(client.clone());
        }
        let client = build_client(
            &self.inner.profile,
            self.inner.profile.timings.streaming_connect,
        )
        .map_err(GatewayFailure::Transport)?;
        *slot = Some(client.clone());
        Ok(client)
    }

    fn first_hop(&self, request: &GatewayRequest) -> Result<Hop, GatewayFailure> {
        let url = match &request.target {
            Target::Path(path) => target::resolve_path(self.inner.base.as_ref(), path)?,
            Target::Absolute(raw) => target::resolve_absolute(raw, &self.inner.profile)?,
        };
        Ok(Hop {
            url,
            method: request.method,
            keep_body: true,
            forward_request_headers: true,
            apply_query: true,
        })
    }

    fn redirect_hop(
        &self,
        hop: &Hop,
        response: &reqwest::Response,
    ) -> Result<Option<Hop>, GatewayFailure> {
        let keeps_method = match response.status() {
            StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND | StatusCode::SEE_OTHER => false,
            StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT => true,
            _ => return Ok(None),
        };
        let Some(location) = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
        else {
            return Ok(None);
        };
        let url = hop
            .url
            .join(location)
            .map_err(|_| redirect_refused(RedirectRefusal::MalformedTarget))?;
        if !matches!(url.scheme(), HTTP_SCHEME | HTTPS_SCHEME) {
            return Err(redirect_refused(RedirectRefusal::UnsupportedScheme));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(redirect_refused(RedirectRefusal::EmbeddedCredentials));
        }
        target::check_hop(&url, &self.inner.profile).map_err(|failure| match failure {
            GatewayFailure::AddressRefused(rejection) => GatewayFailure::AddressRefused(rejection),
            GatewayFailure::InvalidRequest(InvalidRequest::MalformedUrl) => {
                redirect_refused(RedirectRefusal::MalformedTarget)
            }
            _ => redirect_refused(RedirectRefusal::UnsupportedScheme),
        })?;
        let same_origin = url.origin() == hop.url.origin();
        Ok(Some(Hop {
            url,
            method: if keeps_method {
                hop.method
            } else {
                Method::Get
            },
            keep_body: keeps_method && hop.keep_body,
            forward_request_headers: hop.forward_request_headers && same_origin,
            apply_query: false,
        }))
    }

    fn build_hop(
        &self,
        client: &reqwest::Client,
        request: &GatewayRequest,
        hop: &Hop,
    ) -> Result<reqwest::RequestBuilder, GatewayFailure> {
        if request.credential.is_some() && !self.inner.profile.attaches_credentials() {
            return Err(invalid(InvalidRequest::CredentialNotAllowed));
        }
        if !request.user_headers.is_empty() && !self.inner.profile.accepts_user_headers() {
            return Err(invalid(InvalidRequest::UserHeaderNotAllowed));
        }
        let mut builder = client.request(reqwest_method(hop.method), hop.url.clone());
        if hop.apply_query && !request.query.is_empty() {
            let pairs: Vec<(&str, &str)> = request
                .query
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect();
            builder = builder.query(&pairs);
        }
        if hop.keep_body {
            builder = attach_body(builder, &request.body)?;
        }
        if hop.forward_request_headers {
            let mut headers = HeaderMap::new();
            for (name, value) in &request.headers {
                let (name, value) = plain_header(name, value)?;
                headers.append(name, value);
            }
            for (name, value) in &request.user_headers {
                let (name, value) = user_header(name, value)?;
                headers.append(name, value);
            }
            if let Some(credential) = &request.credential {
                let (name, value) = credential_header(credential)?;
                headers.insert(name, value);
            }
            builder = builder.headers(headers);
        }
        Ok(builder)
    }
}

impl fmt::Debug for Gateway {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Gateway")
            .field("profile", &self.inner.profile)
            .finish_non_exhaustive()
    }
}

fn build_client(
    profile: &GatewayProfile,
    connect_budget: Duration,
) -> Result<reqwest::Client, TransportFailure> {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(USER_AGENT)
        .connect_timeout(connect_budget);
    if profile.guards_addresses() {
        builder = builder
            .dns_resolver(Arc::new(PublicAddressResolver))
            .no_proxy();
    }
    builder.build().map_err(TransportFailure::from_reqwest)
}

fn send_failure(error: reqwest::Error) -> GatewayFailure {
    match host_rejection_cause(&error) {
        Some(rejection) => GatewayFailure::AddressRefused(rejection),
        None => GatewayFailure::Transport(TransportFailure::from_reqwest(error)),
    }
}

fn redirect_refused(refusal: RedirectRefusal) -> GatewayFailure {
    GatewayFailure::Transport(TransportFailure::redirect_refused(refusal))
}

fn collect_headers(response: &reqwest::Response) -> Vec<(String, String)> {
    response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_owned(), value.to_owned()))
        })
        .collect()
}

fn reqwest_method(method: Method) -> reqwest::Method {
    match method {
        Method::Get => reqwest::Method::GET,
        Method::Post => reqwest::Method::POST,
        Method::Put => reqwest::Method::PUT,
        Method::Patch => reqwest::Method::PATCH,
        Method::Delete => reqwest::Method::DELETE,
    }
}

fn invalid(reason: InvalidRequest) -> GatewayFailure {
    GatewayFailure::InvalidRequest(reason)
}

fn plain_header(name: &str, value: &str) -> Result<(HeaderName, HeaderValue), GatewayFailure> {
    let name = HeaderName::from_bytes(name.as_bytes())
        .map_err(|_| invalid(InvalidRequest::InvalidHeader))?;
    if name == AUTHORIZATION || name == PROXY_AUTHORIZATION || name == COOKIE {
        return Err(invalid(InvalidRequest::SensitiveHeader));
    }
    let value = HeaderValue::from_str(value).map_err(|_| invalid(InvalidRequest::InvalidHeader))?;
    Ok((name, value))
}

fn user_header(
    name: &str,
    value: &FieldValue,
) -> Result<(HeaderName, HeaderValue), GatewayFailure> {
    let name = HeaderName::from_bytes(name.as_bytes())
        .map_err(|_| invalid(InvalidRequest::InvalidHeader))?;
    let mut header = HeaderValue::from_str(value.as_str())
        .map_err(|_| invalid(InvalidRequest::InvalidHeader))?;
    header.set_sensitive(value.secret().is_some());
    Ok((name, header))
}

fn credential_header(credential: &Credential) -> Result<(HeaderName, HeaderValue), GatewayFailure> {
    let (name, raw) = match credential {
        Credential::Bearer(token) => (AUTHORIZATION, format!("{BEARER_PREFIX}{}", token.expose())),
        Credential::Scheme { scheme, token } => {
            if scheme.is_empty() || scheme.contains(char::is_whitespace) {
                return Err(invalid(InvalidRequest::InvalidHeader));
            }
            (AUTHORIZATION, format!("{scheme} {}", token.expose()))
        }
        Credential::ApiKey { header, key } => {
            let name = HeaderName::from_bytes(header.as_bytes())
                .map_err(|_| invalid(InvalidRequest::InvalidHeader))?;
            if name == AUTHORIZATION || name == PROXY_AUTHORIZATION {
                return Err(invalid(InvalidRequest::SensitiveHeader));
            }
            (name, key.expose().to_owned())
        }
    };
    let mut value =
        HeaderValue::from_str(&raw).map_err(|_| invalid(InvalidRequest::InvalidHeader))?;
    value.set_sensitive(true);
    Ok((name, value))
}

fn attach_body(
    builder: reqwest::RequestBuilder,
    body: &RequestBody,
) -> Result<reqwest::RequestBuilder, GatewayFailure> {
    Ok(match &body.0 {
        BodyRepr::Empty => builder,
        BodyRepr::Json(bytes) => builder
            .header(CONTENT_TYPE, HeaderValue::from_static(JSON_CONTENT_TYPE))
            .body(bytes.clone()),
        BodyRepr::Form(form) => {
            let pairs: Vec<(&str, &str)> = form
                .fields
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect();
            builder.form(&pairs)
        }
        BodyRepr::Raw {
            content_type: Some(content_type),
            bytes,
        } => builder
            .header(
                CONTENT_TYPE,
                HeaderValue::from_str(content_type)
                    .map_err(|_| invalid(InvalidRequest::InvalidContentType))?,
            )
            .body(bytes.clone()),
        BodyRepr::Raw {
            content_type: None,
            bytes,
        } => builder.body(bytes.clone()),
        BodyRepr::Multipart(multipart) => builder.multipart(multipart_form(multipart)?),
    })
}

fn multipart_form(multipart: &MultipartBody) -> Result<Form, GatewayFailure> {
    let mut form = Form::new();
    for (name, content) in &multipart.parts {
        form = match content {
            PartContent::Text(value) => form.text(name.clone(), value.clone()),
            PartContent::File {
                file_name,
                content_type,
                bytes,
            } => {
                let part = Part::stream(bytes.clone())
                    .file_name(file_name.clone())
                    .mime_str(content_type)
                    .map_err(|_| invalid(InvalidRequest::InvalidContentType))?;
                form.part(name.clone(), part)
            }
        };
    }
    Ok(form)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::RecordingRateLimiter;
    use wiremock::matchers::any;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn redirect_answer(location: &str) -> (MockServer, reqwest::Response) {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(
                ResponseTemplate::new(StatusCode::FOUND.as_u16())
                    .insert_header("location", location),
            )
            .mount(&server)
            .await;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let response = client.get(server.uri()).send().await.unwrap();
        (server, response)
    }

    fn public_hop() -> Hop {
        Hop {
            url: Url::parse("https://public.example/start").unwrap(),
            method: Method::Get,
            keep_body: true,
            forward_request_headers: true,
            apply_query: true,
        }
    }

    #[tokio::test]
    async fn every_redirect_hop_is_screened_by_the_literal_guard_only_when_local_is_refused() {
        for (allow_local, location, refused) in [
            (false, "http://10.0.0.1/x", true),
            (false, "http://127.0.0.1/x", true),
            (false, "http://[::ffff:127.0.0.1]/x", true),
            (false, "https://[64:ff9b::a00:1]/x", true),
            (false, "https://203.0.113.9/x", false),
            (false, "http://localhost/x", false),
            (true, "http://10.0.0.1/x", false),
            (true, "http://[::ffff:127.0.0.1]/x", false),
            (true, "https://[64:ff9b::a00:1]/x", false),
        ] {
            let gateway = Gateway::without_base(
                Arc::new(RecordingRateLimiter::granting()),
                GatewayProfile::user_directed(allow_local),
                None,
            )
            .unwrap();
            let (_server, response) = redirect_answer(location).await;

            let next = gateway.redirect_hop(&public_hop(), &response);

            match next {
                Err(failure) => {
                    assert!(refused, "{allow_local} {location}: {failure:?}");
                    assert_eq!(
                        failure,
                        GatewayFailure::AddressRefused(crate::HostRejection::PrivateOrSpecial),
                        "{location}"
                    );
                }
                Ok(hop) => {
                    assert!(!refused, "{allow_local} {location} was followed");
                    let hop = hop.unwrap();
                    assert!(!hop.forward_request_headers, "{location}");
                }
            }
        }
    }
}
