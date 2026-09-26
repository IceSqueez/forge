use async_trait::async_trait;

use crate::backend::AwakeBackend;
use crate::{Aspect, AwakeError};

pub(crate) struct UnsupportedBackend;

#[async_trait]
impl AwakeBackend for UnsupportedBackend {
    async fn acquire(&mut self, _aspect: Aspect) -> Result<(), AwakeError> {
        Err(AwakeError::Unsupported)
    }

    async fn release(&mut self, _aspect: Aspect) {}

    fn is_held(&self, _aspect: Aspect) -> bool {
        false
    }
}
