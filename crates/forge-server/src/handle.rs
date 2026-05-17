use crate::ServerError;

pub struct ServerHandle(tokio::task::JoinHandle<Result<(), ServerError>>);

impl ServerHandle {
    pub(crate) fn new(handle: tokio::task::JoinHandle<Result<(), ServerError>>) -> Self {
        Self(handle)
    }

    pub fn abort(&self) {
        self.0.abort();
    }

    pub async fn await_shutdown(self) -> Result<(), ServerError> {
        match self.0.await {
            Ok(result) => result,
            Err(join_err) if join_err.is_cancelled() => Ok(()),
            Err(join_err) => Err(ServerError::Io(std::io::Error::other(join_err))),
        }
    }
}
