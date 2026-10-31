use super::{ReplyTarget, UserMenuTarget};
use forge_components::TextInput;
use forge_runtime::ActionEngineHandle;
use forge_speak_queue::SpeakQueueHandle;
use forge_storage::{ChatHistoryRepo, VoiceAliasRepo};
use forge_types::Shared;
use gpui::{Entity, FocusHandle, Subscription};
use std::sync::Arc;

pub(crate) struct ChatServices {
    pub(crate) rt_handle: tokio::runtime::Handle,
    pub(crate) action_engine: ActionEngineHandle,
    pub(crate) chat_history_repo: Arc<dyn ChatHistoryRepo>,
    pub(crate) voice_alias_repo: Arc<dyn VoiceAliasRepo>,
    pub(crate) speak: Option<SpeakQueueHandle>,
    pub(crate) bot_accounts: Shared<Vec<String>>,
}

pub(super) struct ReplyMenuState {
    pub(super) reply_target: Option<ReplyTarget>,
    pub(super) reply_input: Entity<TextInput>,
    pub(super) user_menu: Option<UserMenuTarget>,
    pub(super) menu_focus: FocusHandle,
    pub(super) menu_focus_restore: Option<FocusHandle>,
}

pub(super) struct ChatSubscriptions {
    pub(super) _timeline_sub: Subscription,
    pub(super) _drawer_sub: Subscription,
    pub(super) _moderation_sub: Subscription,
    pub(super) _reply_sub: Subscription,
    pub(super) _lifecycle_obs: Option<Subscription>,
}
