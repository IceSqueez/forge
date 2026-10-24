use std::fmt;

use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::header_lookup::HeaderLookup;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecodeCategory {
    Syntax,
    Data,
    Eof,
    Io,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("response body is not the expected JSON ({category:?} at line {line}, column {column})")]
pub struct ResponseDecodeError {
    pub category: DecodeCategory,
    pub line: usize,
    pub column: usize,
}

impl ResponseDecodeError {
    fn from_serde(error: &serde_json::Error) -> Self {
        let category = match error.classify() {
            serde_json::error::Category::Syntax => DecodeCategory::Syntax,
            serde_json::error::Category::Data => DecodeCategory::Data,
            serde_json::error::Category::Eof => DecodeCategory::Eof,
            serde_json::error::Category::Io => DecodeCategory::Io,
        };
        Self {
            category,
            line: error.line(),
            column: error.column(),
        }
    }
}

pub struct GatewayResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl GatewayResponse {
    pub(crate) fn new(status: u16, headers: Vec<(String, String)>, body: Vec<u8>) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn into_body(self) -> Vec<u8> {
        self.body
    }

    pub fn json<T: DeserializeOwned>(&self) -> Result<T, ResponseDecodeError> {
        serde_json::from_slice(&self.body).map_err(|error| ResponseDecodeError::from_serde(&error))
    }
}

impl HeaderLookup for GatewayResponse {
    fn header_value(&self, name: &str) -> Option<&str> {
        self.headers.as_slice().header_value(name)
    }
}

impl fmt::Debug for GatewayResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GatewayResponse")
            .field("status", &self.status)
            .field("headers", &self.headers.len())
            .field("body_bytes", &self.body.len())
            .finish()
    }
}
