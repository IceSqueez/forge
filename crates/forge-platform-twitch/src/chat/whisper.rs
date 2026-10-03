use thiserror::Error;

use crate::helix::{HelixError, HelixMethod, HelixRequest, HelixTransport};
use crate::sub_actions::identity::resolve_user_id;

const WHISPERS_PATH: &str = "/helix/whispers";
const MAX_WHISPER_CHARS: usize = 500;

#[derive(Debug, Error)]
pub(crate) enum WhisperError {
    #[error("whisper recipient is empty")]
    EmptyRecipient,
    #[error("whisper message is empty")]
    EmptyMessage,
    #[error("whisper message exceeds {MAX_WHISPER_CHARS}-character limit")]
    MessageTooLong,
    #[error(transparent)]
    Helix(#[from] HelixError),
}

pub(crate) async fn send_whisper(
    transport: &dyn HelixTransport,
    from_user_id: &str,
    to_user_login: &str,
    message: &str,
) -> Result<(), WhisperError> {
    if to_user_login.is_empty() {
        return Err(WhisperError::EmptyRecipient);
    }
    if message.is_empty() {
        return Err(WhisperError::EmptyMessage);
    }
    if message.chars().count() > MAX_WHISPER_CHARS {
        return Err(WhisperError::MessageTooLong);
    }
    let to_user_id = resolve_user_id(transport, to_user_login).await?;
    let request = HelixRequest::new(HelixMethod::Post, WHISPERS_PATH)
        .query("from_user_id", from_user_id)
        .query("to_user_id", to_user_id)
        .body(serde_json::json!({ "message": message }));
    transport.execute(request).await?;
    Ok(())
}
