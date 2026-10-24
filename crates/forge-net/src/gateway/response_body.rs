use std::future::Future;

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
