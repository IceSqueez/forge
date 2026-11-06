use std::fmt;
use std::time::Duration;

use bytes::Bytes;
use forge_types::Secret;

use super::super::expected_refusal::ExpectedRefusal;
use super::super::request::{Credential, GatewayRequest};

const TE_HEADER: &str = "te";
const TE_TRAILERS: &str = "trailers";

pub struct GrpcRequest {
    pub(super) call: GatewayRequest,
    pub(super) message: Bytes,
    pub(super) message_cap: u64,
    pub(super) read_idle: Option<Duration>,
}

impl GrpcRequest {
    pub fn new(path: impl Into<String>, message: impl Into<Vec<u8>>, message_cap: u64) -> Self {
        Self {
            call: GatewayRequest::post(path)
                .follow_redirects(false)
                .header(TE_HEADER, TE_TRAILERS),
            message: Bytes::from(message.into()),
            message_cap,
            read_idle: None,
        }
    }

    pub fn bearer(self, token: Secret) -> Self {
        self.credential(Credential::Bearer(token))
    }

    pub fn credential(mut self, credential: Credential) -> Self {
        self.call = self.call.credential(credential);
        self
    }

    pub fn metadata(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.call = self.call.header(name, value);
        self
    }

    pub fn label(mut self, label: &'static str) -> Self {
        self.call = self.call.label(label);
        self
    }

    pub fn expect_refusal(mut self, refusal: ExpectedRefusal) -> Self {
        self.call = self.call.expect_refusal(refusal);
        self
    }

    pub fn mask(mut self, secret: Secret) -> Self {
        self.call = self.call.mask(secret);
        self
    }

    pub fn without_limiter_wait(mut self) -> Self {
        self.call = self.call.without_limiter_wait();
        self
    }

    pub fn read_idle(mut self, budget: Duration) -> Self {
        self.read_idle = Some(budget);
        self
    }
}

impl fmt::Debug for GrpcRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GrpcRequest")
            .field("call", &self.call)
            .field("message", &self.message.len())
            .field("message_cap", &self.message_cap)
            .field("read_idle", &self.read_idle)
            .finish()
    }
}
