use std::sync::Arc;

use forge_overlay::OverlayKindRegistry;
use forge_registry::{FormSchemaSource, SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    OverlaySampleSource, OverlayServiceHandle, QueueSchedulerHandle, ScriptRegistry,
};
use forge_storage::{DataProvider, SettingsCatalog};
use forge_transfer::{ClosureRegistries, PackInventory, load_pack_inventory};
use tokio::runtime::Handle;

use crate::actions_screen::overlay_schema::OverlayContentSchema;
use crate::restore_problem::RestoreProblem;
use crate::runtime_handles::RuntimeHandles;

#[derive(Clone)]
pub struct PackServices {
    pub backend: Arc<dyn DataProvider>,
    pub settings_catalog: Arc<SettingsCatalog>,
    pub sub_actions: Arc<SubActionRegistry>,
    pub triggers: Arc<TriggerRegistry>,
    pub overlay_kinds: Arc<OverlayKindRegistry>,
    pub overlays: OverlayServiceHandle,
    pub overlay_samples: Arc<dyn OverlaySampleSource>,
    pub scheduler: QueueSchedulerHandle,
    pub scripts: Arc<ScriptRegistry>,
    pub rt_handle: Handle,
}

#[derive(Clone)]
pub struct PackSource {
    pub inventory: Arc<PackInventory>,
    schema: Arc<OverlayContentSchema>,
}

impl PackServices {
    pub fn of(handles: &RuntimeHandles) -> Self {
        Self {
            backend: Arc::clone(&handles.backend),
            settings_catalog: Arc::clone(&handles.settings_catalog),
            sub_actions: Arc::clone(&handles.sub_action_registry),
            triggers: Arc::clone(&handles.trigger_registry),
            overlay_kinds: Arc::clone(&handles.overlay_kinds),
            overlays: handles.overlays.clone(),
            overlay_samples: Arc::clone(&handles.overlay_samples),
            scheduler: handles.scheduler.clone(),
            scripts: Arc::clone(&handles.script_registry),
            rt_handle: handles.rt_handle.clone(),
        }
    }

    pub fn registries<'a>(&'a self, source: &'a PackSource) -> ClosureRegistries<'a> {
        ClosureRegistries {
            sub_actions: &self.sub_actions,
            triggers: &self.triggers,
            overlays: &self.overlay_kinds,
            schema: Some(source.schema.as_ref() as &(dyn FormSchemaSource + Sync)),
        }
    }

    pub async fn load_source(&self) -> Result<PackSource, String> {
        let inventory = load_pack_inventory(self.backend.as_ref())
            .await
            .map_err(|error| RestoreProblem::of(&error).plain_text())?;
        let overlays = self
            .backend
            .overlay_repo()
            .list()
            .await
            .map_err(|error| error.to_string())?;
        let kind_by_identity = overlays
            .into_iter()
            .map(|overlay| (overlay.id.to_string(), overlay.kind_id))
            .collect();
        let schema = OverlayContentSchema::new(Arc::clone(&self.overlay_kinds))
            .with_identities(kind_by_identity);
        Ok(PackSource {
            inventory: Arc::new(inventory),
            schema: Arc::new(schema),
        })
    }
}
