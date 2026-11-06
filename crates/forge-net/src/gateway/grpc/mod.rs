mod details;
mod framing;
mod request;
mod status;
mod stream;

use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::CONTENT_TYPE;
use tokio_util::sync::CancellationToken;

pub use request::GrpcRequest;
pub use stream::{GrpcItem, GrpcStream};

use super::classify::{ERROR_BODY_READ_CAP, excerpt_of};
use super::failure::{GatewayFailure, InvalidRequest};
use super::request::{BodyRepr, GatewayRequest, RequestBody};
use super::response_body::ResponseBody;
use super::{Gateway, collect_headers, invalid};
use crate::body_read::read_capped;
use status::GRPC_STATUS;
use stream::StreamParts;

const GRPC_CONTENT_TYPE: &str = "application/grpc";
const CONTENT_SUBTYPE_MARK: char = '+';
const PARAMETER_MARK: char = ';';

impl Gateway {
    pub async fn open_grpc_stream(
        &self,
        request: GrpcRequest,
        cancel: CancellationToken,
    ) -> Result<GrpcStream, GatewayFailure> {
        let GrpcRequest {
            mut call,
            message,
            message_cap,
            read_idle,
        } = request;
        let read_idle = read_idle.unwrap_or(self.inner.profile.timings.streaming_read_idle);
        let context = self.failure_context(&call);
        let opened = match framing::encode(&message) {
            Some(framed) => {
                call.body = RequestBody(BodyRepr::Raw {
                    content_type: Some(GRPC_CONTENT_TYPE.to_owned()),
                    bytes: framed,
                });
                tokio::select! {
                    biased;
                    () = cancel.cancelled() => Err(GatewayFailure::Cancelled),
                    opened = self.open_grpc(&call, read_idle) => opened,
                }
            }
            None => Err(invalid(InvalidRequest::MessageTooLarge)),
        };
        match opened {
            Ok(response) => Ok(GrpcStream::new(StreamParts {
                gateway: self.clone(),
                call,
                context,
                response,
                message_cap,
                read_idle,
                cancel,
            })),
            Err(failure) => {
                self.report(&call, context, &failure);
                Err(failure)
            }
        }
    }

    async fn open_grpc(
        &self,
        call: &GatewayRequest,
        read_idle: Duration,
    ) -> Result<reqwest::Response, GatewayFailure> {
        let client = self.grpc_client()?;
        let first = self.first_hop(call)?;
        let builder = self.build_hop(&client, call, &first)?;
        self.acquire_budget(call).await?;
        let response = self
            .follow(&client, call, first, builder, Some(read_idle))
            .await?;
        let status = response.status();
        let offers_grpc = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(is_grpc_content_type);
        if offers_grpc && (status == StatusCode::OK || response.headers().contains_key(GRPC_STATUS))
        {
            return Ok(response);
        }
        if status == StatusCode::OK || status == StatusCode::NOT_FOUND {
            let body = tokio::time::timeout(
                read_idle,
                read_capped(ResponseBody::new(response), ERROR_BODY_READ_CAP),
            )
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
            return Err(GatewayFailure::ProtocolNotOffered {
                excerpt: excerpt_of(&body, &call.secrets()),
            });
        }
        let headers = collect_headers(&response);
        Err(self
            .refusal(call, status, &headers, ResponseBody::new(response))
            .await)
    }
}

fn is_grpc_content_type(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value.strip_prefix(GRPC_CONTENT_TYPE).is_some_and(|rest| {
        rest.is_empty() || rest.starts_with([CONTENT_SUBTYPE_MARK, PARAMETER_MARK])
    })
}
