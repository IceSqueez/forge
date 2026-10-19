use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    ActionCancelRegistry, EventBus, OverlayServiceHandle, QueueScheduler, ScriptRegistry,
    spawn_action_engine,
};
use forge_storage::paths::{CREDENTIALS_KEY_FILE_NAME, MEDIA_DIR_NAME, OVERLAYS_DIR_NAME};
use forge_storage::{DATABASE_FILE_NAME, DataProvider, SettingsRepo, reserved_keys};
use forge_storage_sqlite::SqliteBackend;
use forge_transfer::{
    PackExportRequest, PackImportPlan, PackMetadata, StagedPack, plan_pack_import,
    resolve_pack_closure, stage_pack, write_pack,
};
use forge_types::{Action, ActionId, Queue, QueueId};
use tempfile::TempDir;
use ulid::Ulid;

use super::{STORAGE_KEY, StubActions, StubEventLog, StubHistory, stub_catalog};
use crate::pack_services::PackServices;

pub(crate) const PACK_NAME: &str = "Alerts pack";

pub(crate) struct PackWorld {
    pub(crate) services: PackServices,
    dir: TempDir,
}

impl PackWorld {
    pub(crate) fn data_dir(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }

    pub(crate) fn overlay_root(&self) -> PathBuf {
        self.dir.path().join(OVERLAYS_DIR_NAME)
    }

    pub(crate) fn provider(&self) -> &Arc<dyn DataProvider> {
        &self.services.backend
    }

    pub(crate) fn with_runtime(&self, rt: &tokio::runtime::Runtime) -> PackServices {
        PackServices {
            rt_handle: rt.handle().clone(),
            ..self.services.clone()
        }
    }
}

pub(crate) fn pack_world(rt: &tokio::runtime::Runtime) -> PackWorld {
    rt.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join(DATABASE_FILE_NAME).display());
        let backend =
            SqliteBackend::open_for_test(&url, STORAGE_KEY, dir.path().join(MEDIA_DIR_NAME), None)
                .await
                .unwrap();
        std::fs::write(dir.path().join(CREDENTIALS_KEY_FILE_NAME), STORAGE_KEY).unwrap();
        let provider: Arc<dyn DataProvider> = Arc::new(backend);
        let overlay_root = dir.path().join(OVERLAYS_DIR_NAME);
        provider
            .set_string(
                reserved_keys::SERVER_OVERLAY_ROOT,
                &overlay_root.display().to_string(),
            )
            .await
            .unwrap();
        let mut kinds = OverlayKindRegistry::new();
        register_builtin_kinds(&mut kinds).unwrap();
        let kinds = Arc::new(kinds);
        let bus = EventBus::new(Arc::new(StubEventLog));
        let overlays = OverlayServiceHandle::new(
            provider.overlay_repo(),
            Arc::clone(&provider) as Arc<dyn SettingsRepo>,
            Arc::clone(&kinds),
            Arc::clone(&bus),
            None,
        );
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            stub_catalog(),
            Arc::new(StubActions),
            Arc::new(StubHistory),
            Arc::new(SubActionRegistry::new()),
            Arc::new(ActionCancelRegistry::new()),
        );
        let scheduler = QueueScheduler::spawn(engine, bus, Vec::new());
        PackWorld {
            services: PackServices {
                backend: provider,
                sub_actions: Arc::new(SubActionRegistry::new()),
                triggers: Arc::new(TriggerRegistry::new()),
                overlay_kinds: kinds,
                overlays,
                scheduler,
                scripts: Arc::new(ScriptRegistry::new()),
                rt_handle: rt.handle().clone(),
            },
            dir,
        }
    })
}

pub(crate) fn queue_named(name: &str) -> Queue {
    Queue {
        id: QueueId::new(),
        name: name.to_owned(),
        description: String::new(),
        concurrency: 1,
    }
}

pub(crate) fn seed_queued_action(
    rt: &tokio::runtime::Runtime,
    world: &PackWorld,
    action_name: &str,
    queue: &Queue,
) -> ActionId {
    let action = Action {
        queue_id: queue.id,
        ..super::action_running(action_name, Vec::new())
    };
    rt.block_on(async {
        world.provider().queue_repo().save(queue).await.unwrap();
        world.provider().action_repo().save(&action).await.unwrap();
    });
    action.id
}

pub(crate) fn export_pack(
    rt: &tokio::runtime::Runtime,
    world: &PackWorld,
    roots: &[ActionId],
) -> PathBuf {
    rt.block_on(async {
        let source = world.services.load_source().await.unwrap();
        let selection =
            resolve_pack_closure(&source.inventory, world.services.registries(&source), roots)
                .unwrap();
        write_pack(PackExportRequest {
            provider: Arc::clone(world.provider()),
            inventory: source.inventory.as_ref().clone(),
            selection,
            metadata: PackMetadata {
                name: PACK_NAME.to_owned(),
                author: String::new(),
                description: String::new(),
            },
            overlay_root: world.overlay_root(),
            destination: world
                .data_dir()
                .join(format!("{}.forgebundle", Ulid::generate())),
        })
        .await
        .unwrap()
    })
}

pub(crate) fn stage_into(
    rt: &tokio::runtime::Runtime,
    world: &PackWorld,
    bundle: &Path,
) -> (StagedPack, PackImportPlan) {
    rt.block_on(async {
        let staged = stage_pack(bundle.to_path_buf(), world.data_dir())
            .await
            .unwrap();
        let source = world.services.load_source().await.unwrap();
        let plan = plan_pack_import(
            &staged,
            &source.inventory,
            world.services.registries(&source),
        );
        (staged, plan)
    })
}
