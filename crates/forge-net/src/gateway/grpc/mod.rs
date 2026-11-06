mod framing;
mod request;
mod status;
mod stream;

use std::time::Duration;

use reqwest::StatusCode;
use tokio_util::sync::CancellationToken;

pub use request::GrpcRequest;
pub use status::GrpcStatus;
pub use stream::{GrpcItem, GrpcStream};

use super::failure::{GatewayFailure, InvalidRequest};
use super::request::{BodyRepr, GatewayRequest, RequestBody};
use super::response_body::ResponseBody;
use super::{Gateway, collect_headers, invalid};
use stream::StreamParts;

const GRPC_CONTENT_TYPE: &str = "application/grpc";

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
        if status != StatusCode::OK {
            let headers = collect_headers(&response);
            return Err(self
                .refusal(call, status, &headers, ResponseBody::new(response))
                .await);
        }
        Ok(response)
    }
}
