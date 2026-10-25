use std::fmt;

use bytes::Bytes;
use serde::Serialize;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use super::call_class::CallClass;
use super::expected_refusal::ExpectedRefusal;
use super::secret::Secret;

const DEFAULT_WEIGHT: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Clone)]
pub(crate) enum FieldValue {
    Plain(String),
    Secret(Secret),
}

impl FieldValue {
    pub(crate) fn as_str(&self) -> &str {
        match self {
            Self::Plain(value) => value,
            Self::Secret(secret) => secret.expose(),
        }
    }

    pub(crate) fn secret(&self) -> Option<&str> {
        match self {
            Self::Plain(_) => None,
            Self::Secret(secret) => Some(secret.expose()),
        }
    }
}

#[derive(Clone)]
pub enum Credential {
    Bearer(Secret),
    Scheme { scheme: String, token: Secret },
    ApiKey { header: String, key: Secret },
}

impl Credential {
    pub(crate) fn secret(&self) -> &str {
        match self {
            Self::Bearer(token) | Self::Scheme { token, .. } => token.expose(),
            Self::ApiKey { key, .. } => key.expose(),
        }
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bearer(_) => f.write_str("Bearer"),
            Self::Scheme { scheme, .. } => f.debug_tuple("Scheme").field(scheme).finish(),
            Self::ApiKey { .. } => f.write_str("ApiKey"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("request body could not be encoded as JSON")]
pub struct RequestEncodeError;

#[derive(Clone, Default)]
pub struct FormBody {
    pub(crate) fields: Vec<(String, FieldValue)>,
}

impl FormBody {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn field(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields
            .push((name.into(), FieldValue::Plain(value.into())));
        self
    }

    pub fn secret_field(mut self, name: impl Into<String>, value: Secret) -> Self {
        self.fields.push((name.into(), FieldValue::Secret(value)));
        self
    }
}

#[derive(Clone)]
pub(crate) enum PartContent {
    Text(String),
    File {
        file_name: String,
        content_type: String,
        bytes: Bytes,
    },
}

#[derive(Clone, Default)]
pub struct MultipartBody {
    pub(crate) parts: Vec<(String, PartContent)>,
}

impl MultipartBody {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.parts
            .push((name.into(), PartContent::Text(value.into())));
        self
    }

    pub fn file(
        mut self,
        name: impl Into<String>,
        file_name: impl Into<String>,
        content_type: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        self.parts.push((
            name.into(),
            PartContent::File {
                file_name: file_name.into(),
                content_type: content_type.into(),
                bytes: Bytes::from(bytes.into()),
            },
        ));
        self
    }
}

#[derive(Clone, Default)]
pub(crate) enum BodyRepr {
    #[default]
    Empty,
    Json(Bytes),
    Form(FormBody),
    Raw {
        content_type: String,
        bytes: Bytes,
    },
    Multipart(MultipartBody),
}

#[derive(Clone, Default)]
pub struct RequestBody(pub(crate) BodyRepr);

impl RequestBody {
    pub fn empty() -> Self {
        Self(BodyRepr::Empty)
    }

    pub fn json<T: Serialize + ?Sized>(value: &T) -> Result<Self, RequestEncodeError> {
        serde_json::to_vec(value)
            .map(|encoded| Self(BodyRepr::Json(Bytes::from(encoded))))
            .map_err(|_| RequestEncodeError)
    }

    pub fn raw(content_type: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        Self(BodyRepr::Raw {
            content_type: content_type.into(),
            bytes: Bytes::from(bytes.into()),
        })
    }

    fn kind(&self) -> &'static str {
        match self.0 {
            BodyRepr::Empty => "empty",
            BodyRepr::Json(_) => "json",
            BodyRepr::Form(_) => "form",
            BodyRepr::Raw { .. } => "raw",
            BodyRepr::Multipart(_) => "multipart",
        }
    }
}

impl From<FormBody> for RequestBody {
    fn from(form: FormBody) -> Self {
        Self(BodyRepr::Form(form))
    }
}

impl From<MultipartBody> for RequestBody {
    fn from(multipart: MultipartBody) -> Self {
        Self(BodyRepr::Multipart(multipart))
    }
}

impl fmt::Debug for RequestBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind())
    }
}

#[derive(Clone)]
pub(crate) enum Target {
    Path(String),
    Absolute(String),
}

#[derive(Clone)]
pub struct GatewayRequest {
    pub(crate) method: Method,
    pub(crate) target: Target,
    pub(crate) query: Vec<(String, FieldValue)>,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: RequestBody,
    pub(crate) weight: u32,
    pub(crate) class: Option<CallClass>,
    pub(crate) credential: Option<Credential>,
    pub(crate) label: Option<&'static str>,
    pub(crate) cancel: Option<CancellationToken>,
    pub(crate) expected_refusals: Vec<ExpectedRefusal>,
    pub(crate) masked: Vec<Secret>,
    pub(crate) follow_redirects: bool,
    pub(crate) wait_for_limiter: bool,
}

impl GatewayRequest {
    pub fn new(method: Method, path: impl Into<String>) -> Self {
        Self::with_target(method, Target::Path(path.into()))
    }

    pub fn absolute(method: Method, url: impl Into<String>) -> Self {
        Self::with_target(method, Target::Absolute(url.into()))
    }

    pub fn get(path: impl Into<String>) -> Self {
        Self::new(Method::Get, path)
    }

    pub fn post(path: impl Into<String>) -> Self {
        Self::new(Method::Post, path)
    }

    pub fn put(path: impl Into<String>) -> Self {
        Self::new(Method::Put, path)
    }

    pub fn patch(path: impl Into<String>) -> Self {
        Self::new(Method::Patch, path)
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Self::new(Method::Delete, path)
    }

    fn with_target(method: Method, target: Target) -> Self {
        Self {
            method,
            target,
            query: Vec::new(),
            headers: Vec::new(),
            body: RequestBody::empty(),
            weight: DEFAULT_WEIGHT,
            class: None,
            credential: None,
            label: None,
            cancel: None,
            expected_refusals: Vec::new(),
            masked: Vec::new(),
            follow_redirects: true,
            wait_for_limiter: true,
        }
    }

    pub fn query(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.query
            .push((name.into(), FieldValue::Plain(value.into())));
        self
    }

    pub fn secret_query(mut self, name: impl Into<String>, value: Secret) -> Self {
        self.query.push((name.into(), FieldValue::Secret(value)));
        self
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn body(mut self, body: impl Into<RequestBody>) -> Self {
        self.body = body.into();
        self
    }

    pub fn weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }

    pub fn class(mut self, class: CallClass) -> Self {
        self.class = Some(class);
        self
    }

    pub fn bearer(self, token: Secret) -> Self {
        self.credential(Credential::Bearer(token))
    }

    pub fn authorization(self, scheme: impl Into<String>, token: Secret) -> Self {
        self.credential(Credential::Scheme {
            scheme: scheme.into(),
            token,
        })
    }

    pub fn api_key(self, header: impl Into<String>, key: Secret) -> Self {
        self.credential(Credential::ApiKey {
            header: header.into(),
            key,
        })
    }

    pub fn credential(mut self, credential: Credential) -> Self {
        self.credential = Some(credential);
        self
    }

    pub fn label(mut self, label: &'static str) -> Self {
        self.label = Some(label);
        self
    }

    pub fn cancel_on(mut self, token: CancellationToken) -> Self {
        self.cancel = Some(token);
        self
    }

    pub fn expect_refusal(mut self, refusal: ExpectedRefusal) -> Self {
        self.expected_refusals.push(refusal);
        self
    }

    pub fn mask(mut self, secret: Secret) -> Self {
        self.masked.push(secret);
        self
    }

    pub fn follow_redirects(mut self, follow: bool) -> Self {
        self.follow_redirects = follow;
        self
    }

    pub fn without_limiter_wait(mut self) -> Self {
        self.wait_for_limiter = false;
        self
    }

    pub(crate) fn secrets(&self) -> Vec<&str> {
        let query = self.query.iter().filter_map(|(_, value)| value.secret());
        let form = match &self.body.0 {
            BodyRepr::Form(form) => form
                .fields
                .iter()
                .filter_map(|(_, value)| value.secret())
                .collect(),
            BodyRepr::Empty | BodyRepr::Json(_) | BodyRepr::Raw { .. } | BodyRepr::Multipart(_) => {
                Vec::new()
            }
        };
        self.credential
            .iter()
            .map(Credential::secret)
            .chain(query)
            .chain(form)
            .chain(self.masked.iter().map(Secret::expose))
            .filter(|secret| !secret.is_empty())
            .collect()
    }
}

impl fmt::Debug for GatewayRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GatewayRequest")
            .field("method", &self.method)
            .field("label", &self.label)
            .field("query_pairs", &self.query.len())
            .field("headers", &self.headers.len())
            .field("body", &self.body)
            .field("weight", &self.weight)
            .field("class", &self.class)
            .field("credential", &self.credential)
            .finish_non_exhaustive()
    }
}
