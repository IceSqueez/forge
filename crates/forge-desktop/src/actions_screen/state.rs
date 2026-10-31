use super::{ActionFormHost, HistoryModalHost, PackExportHost};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{ActionsService, EventBus, QueueSchedulerHandle};
use forge_soundboard::ClipLibrary;
use forge_speak_queue::SpeakQueueHandle;
use forge_storage::{
    ActionRepo, GlobalsRepo, OverlayRepo, QueueRepo, ScriptRepo, TriggerInstanceRepo,
};
use std::sync::Arc;

pub(crate) struct ActionsDeps {
    pub(crate) action_repo: Arc<dyn ActionRepo>,
    pub(crate) queue_repo: Arc<dyn QueueRepo>,
    pub(crate) actions_service: Arc<ActionsService>,
    pub(crate) trigger_instance_repo: Arc<dyn TriggerInstanceRepo>,
    pub(crate) script_repo: Arc<dyn ScriptRepo>,
    pub(crate) clip_library: Arc<ClipLibrary>,
    pub(crate) globals_repo: Arc<dyn GlobalsRepo>,
    pub(crate) overlay_repo: Arc<dyn OverlayRepo>,
    pub(crate) speak: Option<SpeakQueueHandle>,
    pub(crate) sub_action_registry: Arc<SubActionRegistry>,
    pub(crate) trigger_registry: Arc<TriggerRegistry>,
    pub(crate) rt_handle: tokio::runtime::Handle,
    pub(crate) bus: Arc<EventBus>,
    pub(crate) scheduler: QueueSchedulerHandle,
}

pub(super) struct ScreenDeps {
    pub(super) queue_repo: Arc<dyn QueueRepo>,
    pub(super) actions_service: Arc<ActionsService>,
    pub(super) trigger_registry: Arc<TriggerRegistry>,
    pub(super) rt_handle: tokio::runtime::Handle,
}

#[derive(Default)]
pub(super) struct ScreenModals {
    pub(super) action_form: Option<ActionFormHost>,
    pub(super) history: Option<HistoryModalHost>,
    pub(super) pack_export: Option<PackExportHost>,
}
