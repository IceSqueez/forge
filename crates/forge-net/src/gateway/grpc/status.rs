use percent_encoding::percent_decode;
use reqwest::header::HeaderMap;

use super::super::classify::excerpt_of;
use super::super::failure::BodyExcerpt;

const GRPC_STATUS: &str = "grpc-status";
const GRPC_MESSAGE: &str = "grpc-message";
const OK_CODE: u32 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrpcStatus {
    code: u32,
    message: BodyExcerpt,
}

impl GrpcStatus {
    pub fn code(&self) -> u32 {
        self.code
    }

    pub fn is_ok(&self) -> bool {
        self.code == OK_CODE
    }

    pub fn message(&self) -> &BodyExcerpt {
        &self.message
    }
}

pub(super) enum StatusField {
    Absent,
    Malformed,
    Present(GrpcStatus),
}

pub(super) fn status_in(headers: &HeaderMap, secrets: &[&str]) -> StatusField {
    let Some(raw) = headers.get(GRPC_STATUS) else {
        return StatusField::Absent;
    };
    let Some(code) = raw
        .to_str()
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok())
    else {
        return StatusField::Malformed;
    };
    let decoded: Vec<u8> = headers
        .get(GRPC_MESSAGE)
        .map(|value| percent_decode(value.as_bytes()).collect())
        .unwrap_or_default();
    StatusField::Present(GrpcStatus {
        code,
        message: excerpt_of(&decoded, secrets),
    })
}
