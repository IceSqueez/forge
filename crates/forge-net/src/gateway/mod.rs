mod call_class;
mod classify;
mod failure;
mod observer;
mod profile;
mod request;
mod resolved_target;
mod response;
mod response_body;
mod secret;
mod target;

use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

use reqwest::header::{
    AUTHORIZATION, CONTENT_TYPE, COOKIE, HeaderMap, HeaderName, HeaderValue, LOCATION,
    PROXY_AUTHORIZATION,
};
use reqwest::multipart::{Form, Part};
use reqwest::{StatusCode, Url};

pub use call_class::CallClass;
pub use failure::{BodyExcerpt, GatewayFailure, InvalidRequest, TransportFailure, TransportKind};
pub use observer::{FailureContext, FailureObserver};
pub use profile::{EgressKind, GatewayProfile};
pub use request::{
    Credential, FormBody, GatewayRequest, Method, MultipartBody, RequestBody, RequestEncodeError,
};
pub use resolved_target::ResolvedTarget;
pub use response::{DecodeCategory, GatewayResponse, ResponseDecodeError};
pub use secret::Secret;
pub use target::GatewayBuildError;

use crate::address_guard::{PublicAddressResolver, host_rejection_cause};
use crate::body_read::{CappedReadError, read_capped};
use crate::rate_limit::{RateLimitUsage, RateLimiter, acquire_or_wait};
use classify::{ERROR_BODY_READ_CAP, excerpt_of, status_failure, throttle_for};
use request::{BodyRepr, PartContent, Target};
use response_body::ResponseBody;

const USER_AGENT: &str = concat!("forge/", env!("CARGO_PKG_VERSION"));
const JSON_CONTENT_TYPE: &str = "application/json";
const BEARER_PREFIX: &str = "Bearer ";
const MAX_REDIRECT_HOPS: usize = 10;
const TOTAL_BUDGET_ELAPSED: &str = "total time budget elapsed";
const TOO_MANY_REDIRECTS: &str = "too many redirects";
const REDIRECT_TARGET_MALFORMED: &str = "redirect target is not a valid URL";
const REDIRECT_TARGET_DISALLOWED: &str = "redirect target is not allowed";

#[derive(Clone)]
pub struct Gateway {
    inner: Arc<Inner>,
}

struct Inner {
    client: reqwest::Client,
    base: Url,
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
        let client = build_client(&profile)?;
        Ok(Self {
            inner: Arc::new(Inner {
                client,
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
        target::resolve_path(&self.inner.base, path).map(|url| ResolvedTarget::from_url(&url))
    }

    pub async fn send(
        &self,
        mut request: GatewayRequest,
    ) -> Result<GatewayResponse, GatewayFailure> {
        let class = request.class.unwrap_or(self.inner.profile.default_class);
        let context = FailureContext {
            method: request.method,
            class,
            label: request.label,
        };
        let outcome = match request.cancel.take() {
            Some(token) => tokio::select! {
                biased;
                () = token.cancelled() => Err(GatewayFailure::Cancelled),
                outcome = self.execute(&request, class) => outcome,
            },
            None => self.execute(&request, class).await,
        };
        if let Err(failure) = &outcome
            && !matches!(failure, GatewayFailure::Cancelled)
            && let Some(observer) = &self.inner.observer
        {
            observer.on_failure(failure, context);
        }
        outcome
    }

    async fn execute(
        &self,
        request: &GatewayRequest,
        class: CallClass,
    ) -> Result<GatewayResponse, GatewayFailure> {
        let first = self.first_hop(request)?;
        let first_builder = self.build_hop(request, &first)?;
        acquire_or_wait(self.inner.limiter.as_ref(), request.weight)
            .await
            .map_err(|_| GatewayFailure::BudgetExhausted)?;
        tokio::time::timeout(
            class.total_budget(),
            self.exchange(request, class, first, first_builder),
        )
        .await
        .unwrap_or_else(|_| {
            Err(GatewayFailure::Transport(TransportFailure::new(
                TransportKind::Timeout,
                TOTAL_BUDGET_ELAPSED,
            )))
        })
    }

    async fn exchange(
        &self,
        request: &GatewayRequest,
        class: CallClass,
        first: Hop,
        first_builder: reqwest::RequestBuilder,
    ) -> Result<GatewayResponse, GatewayFailure> {
        let mut hop = first;
        let mut builder = first_builder;
        let mut followed = 0_usize;
        loop {
            let response = builder.send().await.map_err(send_failure)?;
            let Some(next) = self.redirect_hop(&hop, &response)? else {
                return self.finish(request, class, response).await;
            };
            followed += 1;
            if followed > MAX_REDIRECT_HOPS {
                return Err(redirect_refused(TOO_MANY_REDIRECTS));
            }
            builder = self.build_hop(request, &next)?;
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
        if status.is_success() {
            let body = read_capped(ResponseBody::new(response), class.body_cap())
                .await
                .map_err(|error| match error {
                    CappedReadError::TooLarge { cap } => GatewayFailure::ResponseTooLarge { cap },
                    CappedReadError::Source(error) => send_failure(error),
                })?;
            return Ok(GatewayResponse::new(status.as_u16(), headers, body));
        }
        if let Some(throttle) =
            throttle_for(status, &headers, &self.inner.profile, SystemTime::now())
        {
            self.inner
                .limiter
                .observe_remote_throttle(throttle.wait)
                .await;
            let body = read_capped(ResponseBody::new(response), ERROR_BODY_READ_CAP)
                .await
                .unwrap_or_default();
            return Err(GatewayFailure::RateLimited {
                status: status.as_u16(),
                retry_after: throttle.wait,
                scope: throttle.scope,
                excerpt: excerpt_of(&body, &request.secrets()),
            });
        }
        let body = read_capped(ResponseBody::new(response), ERROR_BODY_READ_CAP)
            .await
            .unwrap_or_default();
        Err(status_failure(
            status,
            excerpt_of(&body, &request.secrets()),
        ))
    }

    fn first_hop(&self, request: &GatewayRequest) -> Result<Hop, GatewayFailure> {
        let url = match &request.target {
            Target::Path(path) => target::resolve_path(&self.inner.base, path)?,
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
            .map_err(|_| redirect_refused(REDIRECT_TARGET_MALFORMED))?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err(redirect_refused(REDIRECT_TARGET_DISALLOWED));
        }
        target::check_hop(&url, &self.inner.profile).map_err(|failure| match failure {
            GatewayFailure::AddressRefused(rejection) => GatewayFailure::AddressRefused(rejection),
            _ => redirect_refused(REDIRECT_TARGET_DISALLOWED),
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
        request: &GatewayRequest,
        hop: &Hop,
    ) -> Result<reqwest::RequestBuilder, GatewayFailure> {
        let mut builder = self
            .inner
            .client
            .request(reqwest_method(hop.method), hop.url.clone());
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

fn build_client(profile: &GatewayProfile) -> Result<reqwest::Client, GatewayBuildError> {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(USER_AGENT)
        .connect_timeout(profile.default_class.connect_budget());
    if profile.guards_addresses() {
        builder = builder
            .dns_resolver(Arc::new(PublicAddressResolver))
            .no_proxy();
    }
    builder.build().map_err(|error| {
        GatewayBuildError::ClientUnavailable(
            TransportFailure::from_reqwest(error).cause().to_owned(),
        )
    })
}

fn send_failure(error: reqwest::Error) -> GatewayFailure {
    match host_rejection_cause(&error) {
        Some(rejection) => GatewayFailure::AddressRefused(rejection),
        None => GatewayFailure::Transport(TransportFailure::from_reqwest(error)),
    }
}

fn redirect_refused(cause: &str) -> GatewayFailure {
    GatewayFailure::Transport(TransportFailure::new(TransportKind::Redirect, cause))
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
            content_type,
            bytes,
        } => builder
            .header(
                CONTENT_TYPE,
                HeaderValue::from_str(content_type)
                    .map_err(|_| invalid(InvalidRequest::InvalidContentType))?,
            )
            .body(bytes.clone()),
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
