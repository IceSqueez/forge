use std::fmt;
use std::time::Duration;

use thiserror::Error;
use tokio_util::sync::CancellationToken;

use super::failure::{GatewayFailure, TransportFailure, TransportKind};
use super::request::GatewayRequest;
use super::response_body::{IdleBoundedBody, IdleBoundedError};
use super::{Gateway, collect_headers, send_failure};
use crate::body_read::{BodyProgress, BodySink, StreamReadError, read_streaming};
use crate::header_lookup::HeaderLookup;

pub const STREAMING_CONNECT_BUDGET: Duration = Duration::from_secs(10);
pub const STREAMING_READ_IDLE_BUDGET: Duration = Duration::from_secs(30);

const BODY_IDLE_ELAPSED: &str = "response body stalled past the read-idle time budget";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StreamFailure<E> {
    #[error(transparent)]
    Gateway(GatewayFailure),
    #[error("response body consumer rejected a chunk")]
    Sink(E),
}

impl<E> From<GatewayFailure> for StreamFailure<E> {
    fn from(failure: GatewayFailure) -> Self {
        Self::Gateway(failure)
    }
}

pub struct StreamedResponse {
    status: u16,
    headers: Vec<(String, String)>,
    received: u64,
}

impl StreamedResponse {
    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn received(&self) -> u64 {
        self.received
    }
}

impl HeaderLookup for StreamedResponse {
    fn header_value(&self, name: &str) -> Option<&str> {
        self.headers.as_slice().header_value(name)
    }
}

impl fmt::Debug for StreamedResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamedResponse")
            .field("status", &self.status)
            .field("headers", &self.headers.len())
            .field("received", &self.received)
            .finish()
    }
}

impl Gateway {
    pub async fn send_streaming<S, P>(
        &self,
        mut request: GatewayRequest,
        cap: u64,
        sink: &mut S,
        on_progress: P,
        cancel: &CancellationToken,
    ) -> Result<StreamedResponse, StreamFailure<S::Error>>
    where
        S: BodySink,
        P: FnMut(BodyProgress) + Send,
    {
        let context = self.failure_context(&request);
        let request_cancel = request.cancel.take().unwrap_or_default();
        let outcome = tokio::select! {
            biased;
            () = cancel.cancelled() => Err(StreamFailure::Gateway(GatewayFailure::Cancelled)),
            () = request_cancel.cancelled() => Err(StreamFailure::Gateway(GatewayFailure::Cancelled)),
            outcome = self.stream(&request, cap, sink, on_progress, cancel) => outcome,
        };
        if let Err(StreamFailure::Gateway(failure)) = &outcome {
            self.report(&request, context, failure);
        }
        outcome
    }

    async fn stream<S, P>(
        &self,
        request: &GatewayRequest,
        cap: u64,
        sink: &mut S,
        on_progress: P,
        cancel: &CancellationToken,
    ) -> Result<StreamedResponse, StreamFailure<S::Error>>
    where
        S: BodySink,
        P: FnMut(BodyProgress) + Send,
    {
        let client = self.streaming_client()?;
        let first = self.first_hop(request)?;
        let first_builder = self.build_hop(&client, request, &first)?;
        self.acquire_budget(request).await?;
        let response = self
            .follow(
                &client,
                request,
                first,
                first_builder,
                Some(self.inner.profile.timings.streaming_read_idle),
            )
            .await?;
        let status = response.status();
        let headers = collect_headers(&response);
        let body = IdleBoundedBody::new(response, self.inner.profile.timings.streaming_read_idle);
        if !status.is_success() && !request.any_status_as_data {
            return Err(self.refusal(request, status, &headers, body).await.into());
        }
        let received = read_streaming(body, cap, sink, on_progress, cancel)
            .await
            .map_err(|error| match error {
                StreamReadError::TooLarge { cap } => {
                    StreamFailure::Gateway(GatewayFailure::ResponseTooLarge { cap })
                }
                StreamReadError::Source(error) => StreamFailure::Gateway(body_failure(error)),
                StreamReadError::Sink(error) => StreamFailure::Sink(error),
                StreamReadError::Cancelled => StreamFailure::Gateway(GatewayFailure::Cancelled),
            })?;
        Ok(StreamedResponse {
            status: status.as_u16(),
            headers,
            received,
        })
    }
}

fn body_failure(error: IdleBoundedError) -> GatewayFailure {
    match error {
        IdleBoundedError::Source(error) => send_failure(error),
        IdleBoundedError::Idle => GatewayFailure::Transport(TransportFailure::new(
            TransportKind::Timeout,
            BODY_IDLE_ELAPSED,
        )),
    }
}
