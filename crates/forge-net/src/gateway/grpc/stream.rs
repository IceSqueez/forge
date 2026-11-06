use std::fmt;
use std::time::Duration;

use http_body_util::BodyExt;
use tokio_util::sync::CancellationToken;

use super::super::failure::{GatewayFailure, TransportFailure, TransportKind};
use super::super::observer::FailureContext;
use super::super::profile::GatewayProfile;
use super::super::request::GatewayRequest;
use super::super::streaming::BODY_IDLE_ELAPSED;
use super::super::{Gateway, send_failure};
use super::framing::{FramingError, MessageDecoder};
use super::status::{GrpcStatus, StatusField, status_in};

const MISSING_STATUS: &str = "response ended without a call status";
const MALFORMED_STATUS: &str = "call status is not a number";
const TRUNCATED_MESSAGE: &str = "response ended inside a message";
const COMPRESSED_MESSAGE: &str = "response message is compressed without a negotiated encoding";
const UNKNOWN_FLAG: &str = "response message carries an unknown frame flag";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrpcItem {
    Message(Vec<u8>),
    End(Result<(), GatewayFailure>),
}

enum State {
    Reading(reqwest::Body),
    Ending(Result<GrpcStatus, GatewayFailure>),
    Done,
}

enum Step {
    Message(Vec<u8>),
    End(GrpcStatus),
}

pub struct GrpcStream {
    gateway: Gateway,
    call: GatewayRequest,
    context: FailureContext,
    state: State,
    decoder: MessageDecoder,
    read_idle: Duration,
    cancel: CancellationToken,
}

pub(super) struct StreamParts {
    pub(super) gateway: Gateway,
    pub(super) call: GatewayRequest,
    pub(super) context: FailureContext,
    pub(super) response: reqwest::Response,
    pub(super) message_cap: u64,
    pub(super) read_idle: Duration,
    pub(super) cancel: CancellationToken,
}

impl GrpcStream {
    pub(super) fn new(parts: StreamParts) -> Self {
        let response: http::Response<reqwest::Body> = parts.response.into();
        let (head, body) = response.into_parts();
        let profile = parts.gateway.inner.profile;
        let state = match status_in(&head.headers, &parts.call.secrets(), &profile) {
            StatusField::Absent => State::Reading(body),
            StatusField::Malformed => State::Ending(Err(protocol_failure(MALFORMED_STATUS))),
            StatusField::Present(status) => State::Ending(Ok(status)),
        };
        Self {
            gateway: parts.gateway,
            call: parts.call,
            context: parts.context,
            state,
            decoder: MessageDecoder::new(parts.message_cap),
            read_idle: parts.read_idle,
            cancel: parts.cancel,
        }
    }

    pub async fn next(&mut self) -> Option<GrpcItem> {
        let end = match std::mem::replace(&mut self.state, State::Done) {
            State::Done => return None,
            State::Ending(end) => end,
            State::Reading(mut body) => {
                let profile = self.gateway.inner.profile;
                let step = tokio::select! {
                    biased;
                    () = self.cancel.cancelled() => Err(GatewayFailure::Cancelled),
                    step = read_step(&mut self.decoder, &mut body, self.read_idle, &self.call, &profile) => step,
                };
                match step {
                    Ok(Step::Message(message)) => {
                        self.state = State::Reading(body);
                        return Some(GrpcItem::Message(message));
                    }
                    Ok(Step::End(status)) => Ok(status),
                    Err(failure) => Err(failure),
                }
            }
        };
        let end = end.and_then(|status| status.into_outcome(&self.gateway.inner.profile));
        if let Err(failure) = &end {
            self.gateway.report(&self.call, self.context, failure);
        }
        Some(GrpcItem::End(end))
    }
}

impl fmt::Debug for GrpcStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GrpcStream")
            .field("call", &self.call)
            .field("finished", &matches!(self.state, State::Done))
            .finish_non_exhaustive()
    }
}

async fn read_step(
    decoder: &mut MessageDecoder,
    body: &mut reqwest::Body,
    read_idle: Duration,
    call: &GatewayRequest,
    profile: &GatewayProfile,
) -> Result<Step, GatewayFailure> {
    loop {
        if let Some(message) = decoder.next_message().map_err(framing_failure)? {
            return Ok(Step::Message(message));
        }
        let frame = tokio::time::timeout(read_idle, body.frame())
            .await
            .map_err(|_| {
                GatewayFailure::Transport(TransportFailure::new(
                    TransportKind::Timeout,
                    BODY_IDLE_ELAPSED,
                ))
            })?;
        let Some(frame) = frame else {
            return Err(protocol_failure(if decoder.holds_partial_message() {
                TRUNCATED_MESSAGE
            } else {
                MISSING_STATUS
            }));
        };
        let frame = match frame.map_err(send_failure)?.into_data() {
            Ok(data) => {
                decoder.push(&data);
                continue;
            }
            Err(frame) => frame,
        };
        let Ok(trailers) = frame.into_trailers() else {
            continue;
        };
        if decoder.holds_partial_message() {
            return Err(protocol_failure(TRUNCATED_MESSAGE));
        }
        return match status_in(&trailers, &call.secrets(), profile) {
            StatusField::Present(status) => Ok(Step::End(status)),
            StatusField::Absent => Err(protocol_failure(MISSING_STATUS)),
            StatusField::Malformed => Err(protocol_failure(MALFORMED_STATUS)),
        };
    }
}

fn framing_failure(error: FramingError) -> GatewayFailure {
    match error {
        FramingError::TooLarge { cap } => GatewayFailure::ResponseTooLarge { cap },
        FramingError::Compressed => protocol_failure(COMPRESSED_MESSAGE),
        FramingError::UnknownFlag => protocol_failure(UNKNOWN_FLAG),
    }
}

fn protocol_failure(cause: &'static str) -> GatewayFailure {
    GatewayFailure::Transport(TransportFailure::new(TransportKind::Protocol, cause))
}
