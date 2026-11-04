use std::sync::Arc;

use forge_overlay::OverlayKindRegistry;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    OverlaySampleSource, OverlayServiceHandle, QueueSchedulerHandle, ScriptCatalog,
};
use forge_storage::{DataProvider, SettingsCatalog};
use forge_transfer::{ClosureRegistries, PackInventory, load_pack_inventory};
use tokio::runtime::Handle;

use crate::app::runtime_handles::RuntimeHandles;
use crate::shared::repo_failure::RepoFailure;

#[derive(Clone)]
pub(crate) struct PackServices {
    pub backend: Arc<dyn DataProvider>,
    pub settings_catalog: Arc<SettingsCatalog>,
    pub sub_actions: Arc<SubActionRegistry>,
    pub triggers: Arc<TriggerRegistry>,
    pub overlay_kinds: Arc<OverlayKindRegistry>,
    pub overlays: OverlayServiceHandle,
    pub overlay_samples: Arc<dyn OverlaySampleSource>,
    pub scheduler: QueueSchedulerHandle,
    pub scripts: Arc<ScriptCatalog>,
    pub rt_handle: Handle,
}

#[derive(Clone)]
pub(crate) struct PackSource {
    pub inventory: Arc<PackInventory>,
}

impl PackServices {
    pub(crate) fn of(handles: &RuntimeHandles) -> Self {
        Self {
            backend: Arc::clone(&handles.backend),
            settings_catalog: Arc::clone(&handles.settings_catalog),
            sub_actions: Arc::clone(&handles.automation.sub_action_registry),
            triggers: Arc::clone(&handles.automation.trigger_registry),
            overlay_kinds: Arc::clone(&handles.overlays.kinds),
            overlays: handles.overlays.service.clone(),
            overlay_samples: Arc::clone(&handles.overlays.samples),
            scheduler: handles.automation.scheduler.clone(),
            scripts: Arc::clone(&handles.automation.script_catalog),
            rt_handle: handles.rt_handle.clone(),
        }
    }

    pub(crate) fn registries(&self) -> ClosureRegistries<'_> {
        ClosureRegistries {
            sub_actions: &self.sub_actions,
            triggers: &self.triggers,
            overlays: &self.overlay_kinds,
        }
    }

    pub(crate) async fn load_source(&self) -> Result<PackSource, RepoFailure> {
        let inventory = load_pack_inventory(self.backend.as_ref())
            .await
            .map_err(RepoFailure::from)?;
        Ok(PackSource {
            inventory: Arc::new(inventory),
        })
    }
}
