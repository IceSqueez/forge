mod delete_message;
mod send_message;

use std::sync::Arc;

use forge_platform_core::PlatformError;
use forge_registry::{RegistryError, SubActionRegistry};
use futures::future::BoxFuture;

pub use delete_message::DeleteMessageRunner;
pub use send_message::SendMessageRunner;

use crate::send::KickSendChat;

pub fn register_kick_sub_actions(
    reg: &mut SubActionRegistry,
    client: Arc<KickSendChat>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    broadcaster_user_id: u64,
) -> Result<(), RegistryError> {
    reg.register(Box::new(SendMessageRunner::new(
        Arc::clone(&client),
        Arc::clone(&token_source),
        broadcaster_user_id,
    )))?;
    reg.register(Box::new(DeleteMessageRunner::new(client, token_source)))?;
    Ok(())
}
