use std::future::Future;
use std::time::Duration;

use bytes::Bytes;

use crate::body_read::BodySource;

pub(crate) struct ResponseBody(reqwest::Response);

impl ResponseBody {
    pub(crate) fn new(response: reqwest::Response) -> Self {
        Self(response)
    }
}

impl BodySource for ResponseBody {
    type Chunk = Bytes;
    type Error = reqwest::Error;

    fn declared_length(&self) -> Option<u64> {
        self.0.content_length()
    }

    fn next_chunk(&mut self) -> impl Future<Output = Result<Option<Bytes>, reqwest::Error>> + Send {
        self.0.chunk()
    }
}

pub(crate) enum IdleBoundedError {
    Source(reqwest::Error),
    Idle,
}

pub(crate) struct IdleBoundedBody {
    body: ResponseBody,
    idle_budget: Duration,
}

impl IdleBoundedBody {
    pub(crate) fn new(response: reqwest::Response, idle_budget: Duration) -> Self {
        Self {
            body: ResponseBody::new(response),
            idle_budget,
        }
    }
}

impl BodySource for IdleBoundedBody {
    type Chunk = Bytes;
    type Error = IdleBoundedError;

    fn declared_length(&self) -> Option<u64> {
        self.body.declared_length()
    }

    async fn next_chunk(&mut self) -> Result<Option<Bytes>, IdleBoundedError> {
        match tokio::time::timeout(self.idle_budget, self.body.next_chunk()).await {
            Ok(chunk) => chunk.map_err(IdleBoundedError::Source),
            Err(_) => Err(IdleBoundedError::Idle),
        }
    }
}
