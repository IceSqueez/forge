mod send_message;

use std::sync::Arc;

use forge_registry::{RegistryError, SubActionRegistry};

pub use send_message::SendMessageRunner;

use crate::send_chat::YoutubeSendChat;

pub fn register_youtube_sub_actions(
    reg: &mut SubActionRegistry,
    sender: Arc<YoutubeSendChat>,
) -> Result<(), RegistryError> {
    reg.register(Box::new(SendMessageRunner::new(sender)))?;
    Ok(())
}
