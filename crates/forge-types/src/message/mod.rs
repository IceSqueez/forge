mod arg;
mod catalog;
mod code;
mod common;
mod detail;
mod reference;
mod user_text;
mod verbatim;

pub use arg::{FiniteFloat, MessageArg};
pub use catalog::{MessageCatalog, MessageCatalogError};
pub use code::{MESSAGE_KEY_PREFIX, MessageCode, MessageCodeError};
pub use common::{
    COMMON_MESSAGES, COMMON_NAMESPACE, INTEGRATION_DISABLED, NO_CHAT_PLATFORM_ENABLED,
    NO_WHISPER_PLATFORM_ENABLED, UNKNOWN_CHAT_TARGET, UPSTREAM_FAILURE, WHISPERS_UNSUPPORTED,
};
pub use detail::{MAX_UPSTREAM_DETAIL_CHARS, UpstreamDetail};
pub use reference::MessageRef;
pub use user_text::UserText;
pub use verbatim::VerbatimText;
