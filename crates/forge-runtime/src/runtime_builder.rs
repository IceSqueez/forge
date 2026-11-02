use std::sync::Arc;

use forge_events::EventPublisher;
use forge_net::TaskOwner;
use forge_overlay::OverlayKindRegistry;
use forge_registry::{RegistryError, SubActionRegistry, TriggerRegistry};
use forge_script::SpeakRequester;
use forge_storage::{ActionRepo, CatalogRevision, DataProvider};
use forge_types::{LatestValueReader, Queue, WallClock};

use crate::config::Config;
use crate::engine::action_cancel::ActionCancelRegistry;
use crate::engine::action_engine::{ActionEngineHandle, spawn_action_engine};
use crate::engine::frozen_triggers::FrozenTriggers;
use crate::engine::queue_scheduler::{QueueScheduler, QueueSchedulerHandle, QueueSchedulerStart};
use crate::engine::scheduled_runs::{
    CatchUpSettle, ScheduledRunsHandle, ScheduledRunsStart, ScheduledRunsStartParts,
    ScriptScheduling, reserve_scheduled_runs,
};
use crate::engine::trigger_evaluator::{TriggerEvaluatorHandle, spawn_trigger_evaluator};
use crate::event_flow::bus::EventBus;
use crate::overlays::overlay_media::OverlayMediaLibrary;
use crate::overlays::overlay_service::{
    OverlayFrameSink, OverlayServiceHandle, OverlayServiceParts,
};
use crate::services::catalog::Catalog;
use crate::services::date_names::DateNamesHandle;
use crate::services::latest::LatestValues;
use crate::services::script_catalog::{ScriptApiDeps, ScriptCatalog};
use crate::speech::speak_dispatcher::ShowSpeaker;
use crate::sub_action_runners::{
    CoreSubActionDeps, OsPorts, register_core_sub_actions, register_scheduled_run_sub_actions,
};
use forge_paths::AppPaths;

pub struct RuntimeWiringParts {
    pub bus: Arc<EventBus>,
    pub provider: Arc<dyn DataProvider>,
    pub latest_values: LatestValues,
    pub script_speech: Option<Arc<dyn SpeakRequester>>,
    pub overlay_speech: Option<Arc<dyn ShowSpeaker>>,
    pub overlay_kinds: Arc<OverlayKindRegistry>,
    pub overlay_frames: Arc<dyn OverlayFrameSink>,
    pub clock: Arc<dyn WallClock>,
    pub date_names: DateNamesHandle,
    pub tasks: TaskOwner,
    pub paths: AppPaths,
    pub config: Config,
    pub os: OsPorts,
}

pub struct RuntimeWiring {
    bus: Arc<EventBus>,
    provider: Arc<dyn DataProvider>,
    scripts: Arc<ScriptCatalog>,
    cancel_registry: Arc<ActionCancelRegistry>,
    overlays: OverlayServiceHandle,
    date_names: DateNamesHandle,
    scheduler: QueueSchedulerHandle,
    scheduler_start: QueueSchedulerStart,
    scheduled_runs: ScheduledRunsHandle,
    scheduled_runs_start: ScheduledRunsStart,
    sub_actions: SubActionRegistry,
    triggers: TriggerRegistry,
    frozen_triggers: FrozenTriggers,
    paths: AppPaths,
    config: Config,
    os: OsPorts,
}

impl RuntimeWiring {
    pub fn new(parts: RuntimeWiringParts) -> Self {
        let RuntimeWiringParts {
            bus,
            provider,
            latest_values,
            script_speech,
            overlay_speech,
            overlay_kinds,
            overlay_frames,
            clock,
            date_names,
            tasks,
            paths,
            config,
            os,
        } = parts;
        let (scheduler, scheduler_start) = QueueScheduler::reserve();
        let (scheduled_runs, scheduled_runs_start) =
            reserve_scheduled_runs(provider.scheduled_run_repo(), clock);
        let latest: Arc<dyn LatestValueReader> = Arc::new(latest_values);
        if script_speech.is_none() {
            tracing::warn!("no speak dispatcher available; scripts cannot speak");
        }
        let scripts = Arc::new(ScriptCatalog::with_parts(ScriptApiDeps {
            speak_requester: script_speech,
            latest_values: Some(Arc::clone(&latest)),
            scheduling: Some(ScriptScheduling::new(
                scheduled_runs.clone(),
                provider.action_repo(),
            )),
        }));
        let overlays = OverlayServiceHandle::new(OverlayServiceParts {
            repo: provider.overlay_repo(),
            settings: provider.settings_repo(),
            kinds: overlay_kinds,
            bus: Arc::clone(&bus),
            frames: overlay_frames,
            media: Some(OverlayMediaLibrary::new(
                provider.media_repo(),
                provider.soundboard_clips_repo(),
            )),
            speaker: overlay_speech,
            latest: Some(latest),
            date_names: date_names.clone(),
            tasks,
            default_root: paths.overlays_dir(),
        });
        Self {
            bus,
            provider,
            scripts,
            cancel_registry: Arc::new(ActionCancelRegistry::new()),
            overlays,
            date_names,
            scheduler,
            scheduler_start,
            scheduled_runs,
            scheduled_runs_start,
            sub_actions: SubActionRegistry::new(),
            triggers: TriggerRegistry::new(),
            frozen_triggers: FrozenTriggers::default(),
            paths,
            config,
            os,
        }
    }

    pub fn scripts(&self) -> &Arc<ScriptCatalog> {
        &self.scripts
    }

    pub fn chat_avatars(&self) -> crate::ChatAvatars {
        self.overlays.chat_avatars()
    }

    pub fn sub_actions_mut(&mut self) -> &mut SubActionRegistry {
        &mut self.sub_actions
    }

    pub fn triggers_mut(&mut self) -> &mut TriggerRegistry {
        &mut self.triggers
    }

    pub fn registries_mut(&mut self) -> (&mut SubActionRegistry, &mut TriggerRegistry) {
        (&mut self.sub_actions, &mut self.triggers)
    }

    pub fn register_core_sub_actions(&mut self) -> Result<(), RegistryError> {
        register_core_sub_actions(
            &mut self.sub_actions,
            CoreSubActionDeps {
                globals: self.provider.globals_repo(),
                user_globals: self.provider.user_globals_repo(),
                scripts: Arc::clone(&self.scripts),
                publisher: Arc::clone(&self.bus) as Arc<dyn EventPublisher>,
                settings: self.provider.settings_repo(),
                scheduler: self.scheduler.clone(),
                trigger_instances: self.provider.trigger_instance_repo(),
                trigger_kinds: self.frozen_triggers.clone(),
                actions: self.provider.action_repo(),
                script_repo: self.provider.script_repo(),
                cancel_registry: Arc::clone(&self.cancel_registry),
                overlays: self.overlays.clone(),
                config: self.config.clone(),
                assets_root: self.paths.assets_dir(),
                os: self.os.clone(),
            },
        )
    }

    pub fn register_scheduled_run_sub_actions(&mut self) -> Result<(), RegistryError> {
        register_scheduled_run_sub_actions(&mut self.sub_actions, self.scheduled_runs.clone())
    }

    pub async fn freeze(self) -> FrozenRuntime {
        let Self {
            bus,
            provider,
            scripts,
            cancel_registry,
            overlays,
            date_names,
            scheduler,
            scheduler_start,
            scheduled_runs,
            scheduled_runs_start,
            sub_actions,
            triggers,
            frozen_triggers,
            paths: _,
            config,
            os: _,
        } = self;
        let sub_action_registry = Arc::new(sub_actions);
        let trigger_registry = Arc::new(triggers);
        frozen_triggers.freeze(Arc::clone(&trigger_registry));
        bus.declare_lanes(&trigger_registry);
        let trigger_defaults_stored =
            store_trigger_defaults(provider.as_ref(), &trigger_registry).await;
        let stored_queues = load_queues(provider.as_ref()).await;
        let queues_loaded = stored_queues.is_some();

        let catalog = Catalog::from_provider(provider.as_ref());
        let action_engine = spawn_action_engine(crate::ActionEngineParts {
            bus: Arc::clone(&bus),
            catalog: Arc::clone(&catalog),
            actions: provider.action_repo(),
            history: provider.history_repo(),
            sub_action_registry: Arc::clone(&sub_action_registry),
            cancel_registry,
            date_names,
            config: config.clone(),
        });
        scheduler_start.start(
            action_engine.clone(),
            Arc::clone(&bus),
            stored_queues.unwrap_or_default(),
        );
        let trigger_evaluator = spawn_trigger_evaluator(
            Arc::clone(&bus),
            Arc::clone(&trigger_registry),
            Arc::clone(&catalog),
            scheduler.clone(),
            Config::default(),
        );
        let scheduled_runs_worker = ScheduledRunsWorker {
            start: scheduled_runs_start,
            revision: provider.scheduled_run_revision(),
            catalog: Arc::clone(&catalog),
            actions: provider.action_repo(),
            queues: scheduler.clone(),
            bus,
        };
        FrozenRuntime {
            catalog,
            scripts,
            overlays,
            sub_action_registry,
            trigger_registry,
            action_engine,
            scheduler,
            trigger_evaluator,
            scheduled_runs,
            scheduled_runs_worker,
            trigger_defaults_stored,
            queues_loaded,
        }
    }
}

async fn store_trigger_defaults(provider: &dyn DataProvider, triggers: &TriggerRegistry) -> bool {
    let repo = provider.trigger_instance_repo();
    let mut all_stored = true;
    for descriptor in triggers.all() {
        if let Err(e) = repo
            .upsert_default(descriptor.id(), descriptor.label())
            .await
        {
            tracing::error!(
                kind_id = %descriptor.id(),
                error = %e,
                "default trigger kind could not be stored"
            );
            all_stored = false;
        }
    }
    all_stored
}

async fn load_queues(provider: &dyn DataProvider) -> Option<Vec<Queue>> {
    match provider.queue_repo().list().await {
        Ok(queues) => Some(queues),
        Err(e) => {
            tracing::warn!(error = %e, "failed to load queues on boot, starting empty");
            None
        }
    }
}

pub struct FrozenRuntime {
    pub catalog: Arc<Catalog>,
    pub scripts: Arc<ScriptCatalog>,
    pub overlays: OverlayServiceHandle,
    pub sub_action_registry: Arc<SubActionRegistry>,
    pub trigger_registry: Arc<TriggerRegistry>,
    pub action_engine: ActionEngineHandle,
    pub scheduler: QueueSchedulerHandle,
    pub trigger_evaluator: TriggerEvaluatorHandle,
    pub scheduled_runs: ScheduledRunsHandle,
    pub scheduled_runs_worker: ScheduledRunsWorker,
    pub trigger_defaults_stored: bool,
    pub queues_loaded: bool,
}

pub struct ScheduledRunsWorker {
    start: ScheduledRunsStart,
    revision: CatalogRevision,
    catalog: Arc<Catalog>,
    actions: Arc<dyn ActionRepo>,
    queues: QueueSchedulerHandle,
    bus: Arc<EventBus>,
}

impl ScheduledRunsWorker {
    pub fn start(self, catch_up: CatchUpSettle) {
        let Self {
            start,
            revision,
            catalog,
            actions,
            queues,
            bus,
        } = self;
        start.start(ScheduledRunsStartParts {
            revision,
            catalog,
            actions,
            queues,
            bus,
            catch_up,
        });
    }
}
