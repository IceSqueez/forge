use std::sync::Arc;

use forge_overlay::OverlayKindRegistry;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{ActionsService, EventBus, QueueScheduler};
use forge_soundboard::ClipLibrary;
use forge_storage::{
    ActionRepo, GlobalsRepo, MediaRepo, QueueRepo, ScriptRepo, SettingsRepo, SoundboardClipsRepo,
    TriggerInstanceRepo,
};
use gpui::Context;

use super::{StubEventLog, StubHistory, StubOverlays, stub_action_engine};
use crate::automation::actions_screen::{ActionsDeps, ScreenActionsView};

pub(crate) struct ActionsRepos {
    pub(crate) actions: Arc<dyn ActionRepo>,
    pub(crate) queues: Arc<dyn QueueRepo>,
    pub(crate) triggers: Arc<dyn TriggerInstanceRepo>,
    pub(crate) scripts: Arc<dyn ScriptRepo>,
    pub(crate) clips: Arc<dyn SoundboardClipsRepo>,
    pub(crate) media: Arc<dyn MediaRepo>,
    pub(crate) globals: Arc<dyn GlobalsRepo>,
    pub(crate) sub_actions: SubActionRegistry,
    pub(crate) trigger_kinds: TriggerRegistry,
}

pub(crate) fn actions_deps(rt: &tokio::runtime::Runtime, repos: ActionsRepos) -> ActionsDeps {
    let actions_service = Arc::new(ActionsService::new(
        Arc::clone(&repos.actions),
        Arc::clone(&repos.queues),
        Arc::new(StubHistory),
        Arc::clone(&repos.triggers),
    ));
    let (bus, scheduler) = rt.block_on(async {
        let bus = EventBus::new(Arc::new(StubEventLog));
        let engine = stub_action_engine(Arc::clone(&bus));
        let scheduler = QueueScheduler::spawn(engine, Arc::clone(&bus), Vec::new());
        (bus, scheduler)
    });
    ActionsDeps {
        action_repo: repos.actions,
        queue_repo: repos.queues,
        actions_service,
        trigger_instance_repo: repos.triggers,
        script_repo: repos.scripts,
        clip_library: Arc::new(ClipLibrary::new(
            repos.clips,
            repos.media,
            Arc::new(forge_events::test_support::NullPublisher),
        )),
        globals_repo: repos.globals,
        overlay_repo: Arc::new(StubOverlays),
        speak: None,
        sub_action_registry: Arc::new(repos.sub_actions),
        trigger_registry: Arc::new(repos.trigger_kinds),
        rt_handle: rt.handle().clone(),
        bus,
        scheduler,
    }
}

pub(crate) fn actions_screen(
    deps: ActionsDeps,
    settings: Arc<dyn SettingsRepo>,
    cx: &mut Context<ScreenActionsView>,
) -> ScreenActionsView {
    ScreenActionsView::new(
        deps,
        settings,
        Arc::new(OverlayKindRegistry::new()),
        None,
        cx,
    )
}
