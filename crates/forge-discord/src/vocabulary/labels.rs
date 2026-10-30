use forge_types::MessageCode;

use crate::vocabulary::INTEGRATION_ID;

pub(crate) const EDIT_MESSAGE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_edit_message");
pub(crate) const EMBED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_embed");
pub(crate) const ERROR: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_error");
pub(crate) const OK: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_ok");
pub(crate) const POST_EMBED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_post_embed");
pub(crate) const POST_TEXT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_post_text");
pub(crate) const READY: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_ready");
pub(crate) const RECENT_POSTS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_recent_posts");
pub(crate) const TEST_WEBHOOK: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_test_webhook");
pub(crate) const WEBHOOKS: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_webhooks");
