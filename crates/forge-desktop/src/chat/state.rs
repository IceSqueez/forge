use super::viewer_actions::ViewerTarget;
use super::viewer_follow::FollowLookups;
use super::viewer_tts::TtsVoiceHost;
use super::{ReplyTarget, UserMenuTarget, ViewerHistoryHost};
use crate::chat_author::AuthorKey;
use crate::chat_drawer::ViewerDirectory;
use crate::chat_viewer_messages::ViewerMessages;
use crate::navigable_list::ListCursor;
use forge_components::{Confirm, SearchState, TextInput};
use forge_runtime::ActionEngineHandle;
use forge_speak_queue::SpeakQueueHandle;
use forge_storage::{ChatHistoryRepo, VoiceAliasRepo};
use forge_types::Shared;
use gpui::{Entity, FocusHandle, Pixels, Point, Subscription, UniformListScrollHandle};
use std::sync::Arc;

pub(crate) struct ChatServices {
    pub(crate) rt_handle: tokio::runtime::Handle,
    pub(crate) action_engine: ActionEngineHandle,
    pub(crate) chat_history_repo: Arc<dyn ChatHistoryRepo>,
    pub(crate) voice_alias_repo: Arc<dyn VoiceAliasRepo>,
    pub(crate) speak: Option<SpeakQueueHandle>,
    pub(crate) bot_accounts: Shared<Vec<String>>,
}

pub(super) struct DrawerState {
    pub(super) drawer_width: Pixels,
    pub(super) drawer_search: SearchState,
    pub(super) drawer_menu_open: Option<Point<Pixels>>,
    pub(super) selected_viewer: Option<AuthorKey>,
    pub(super) follows: FollowLookups,
    pub(super) viewer_messages: Entity<ViewerMessages>,
    pub(super) viewer_history: Option<ViewerHistoryHost>,
    pub(super) tts_voice: Option<TtsVoiceHost>,
    pub(super) pending_ban: Confirm<ViewerTarget>,
    pub(super) viewers: ViewerDirectory,
    pub(super) drawer_keys: Vec<AuthorKey>,
    pub(super) drawer_list_keys: ListCursor<AuthorKey>,
    pub(super) drawer_list_scroll: UniformListScrollHandle,
    pub(super) whisper_open: bool,
    pub(super) whisper_input: Entity<TextInput>,
}

pub(super) struct ReplyMenuState {
    pub(super) reply_target: Option<ReplyTarget>,
    pub(super) reply_input: Entity<TextInput>,
    pub(super) user_menu: Option<UserMenuTarget>,
    pub(super) menu_focus: FocusHandle,
    pub(super) menu_focus_restore: Option<FocusHandle>,
}

pub(super) struct ChatSubscriptions {
    pub(super) _feed_obs: Subscription,
    pub(super) _timeline_sub: Subscription,
    pub(super) _drawer_search_sub: Subscription,
    pub(super) _whisper_sub: Subscription,
    pub(super) _reply_sub: Subscription,
    pub(super) _viewer_messages_obs: Subscription,
    pub(super) _lifecycle_obs: Option<Subscription>,
}
