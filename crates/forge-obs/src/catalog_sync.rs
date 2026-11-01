use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use forge_platform_core::HealthDelta;
use forge_types::RecoverRwLock;
use tokio::sync::broadcast;

use crate::catalog::write_lock;
use crate::catalog_revisions::CatalogRevisions;
use crate::error::ObsError;
use crate::health::HealthSnapshot;
use crate::item_ids::SceneItemIds;
use crate::mirror::{FetchedSnapshot, FetchedTopology, ObsMirror};
use crate::session::LiveSession;
use crate::source::SourceInfo;
use crate::stream_output::StreamOutputFeed;

const TOPOLOGY_FETCH_ATTEMPTS: u32 = 5;

pub(crate) async fn prime_session(
    session: &LiveSession,
    obs_version: &RwLock<Option<String>>,
    obs_ws_version: &RwLock<Option<String>>,
    sink: &CatalogSink<'_>,
) -> Result<(), ObsError> {
    match session
        .request("GetVersion", session.obs().general().version())
        .await
    {
        Ok(v) => {
            *write_lock(obs_version) = Some(v.obs_studio_version.to_string());
            *write_lock(obs_ws_version) = Some(v.obs_web_socket_version.to_string());
        }
        Err(e) if e.is_connection_loss() => return Err(e),
        Err(e) => tracing::warn!(error = %e, "failed to fetch OBS version"),
    }
    snapshot_catalog(session, sink).await
}

pub(crate) struct CatalogSink<'a> {
    pub(crate) mirror: &'a ObsMirror,
    pub(crate) health_state: &'a RwLock<HealthSnapshot>,
    pub(crate) health_tx: &'a broadcast::Sender<HealthDelta>,
}

#[derive(Clone)]
pub(crate) struct CatalogFeeds {
    pub(crate) mirror: Arc<ObsMirror>,
    pub(crate) catalog_revisions: Arc<CatalogRevisions>,
    pub(crate) health_state: Arc<RwLock<HealthSnapshot>>,
    pub(crate) health_tx: broadcast::Sender<HealthDelta>,
    pub(crate) stream_output: StreamOutputFeed,
}

pub(crate) fn tolerate_rejection<T>(outcome: Result<T, ObsError>) -> Result<Option<T>, ObsError> {
    match outcome {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.is_connection_loss() => Err(e),
        Err(_) => Ok(None),
    }
}

pub(crate) async fn snapshot_catalog(
    session: &LiveSession,
    sink: &CatalogSink<'_>,
) -> Result<(), ObsError> {
    for _ in 0..TOPOLOGY_FETCH_ATTEMPTS {
        if snapshot_catalog_once(session, sink).await? {
            return Ok(());
        }
    }
    tracing::warn!(
        "OBS catalog kept changing during every resync attempt; keeping the live mirror"
    );
    Ok(())
}

async fn snapshot_catalog_once(
    session: &LiveSession,
    sink: &CatalogSink<'_>,
) -> Result<bool, ObsError> {
    let CatalogSink {
        mirror,
        health_state,
        health_tx,
    } = *sink;
    use obws::requests::inputs::InputId;
    use obws::requests::scenes::SceneId;

    let obs = session.obs();
    let started_at = mirror.revision();

    let scenes: Vec<String> =
        tolerate_rejection(session.request("GetSceneList", obs.scenes().list()).await)?
            .map(|list| list.scenes.iter().map(|s| s.id.name.clone()).collect())
            .unwrap_or_default();

    let current_scene: Option<String> = tolerate_rejection(
        session
            .request(
                "GetCurrentProgramScene",
                obs.scenes().current_program_scene(),
            )
            .await,
    )?
    .map(|s| s.id.name);

    let current_preview_scene: Option<String> = tolerate_rejection(
        session
            .request(
                "GetCurrentPreviewScene",
                obs.scenes().current_preview_scene(),
            )
            .await,
    )?
    .map(|s| s.id.name);

    let mut sources_by_scene: HashMap<String, Vec<SourceInfo>> = HashMap::new();
    let mut db_cache: HashMap<String, f32> = HashMap::new();
    let mut fresh_ids = SceneItemIds::default();
    for scene in &scenes {
        let Some(items) = tolerate_rejection(
            session
                .request(
                    "GetSceneItemList",
                    obs.scene_items().list(SceneId::Name(scene)),
                )
                .await,
        )?
        else {
            continue;
        };
        let mut infos = Vec::with_capacity(items.len());
        for item in items {
            fresh_ids.insert(scene, item.id, &item.source_name);

            let visible = tolerate_rejection(
                session
                    .request(
                        "GetSceneItemEnabled",
                        obs.scene_items().enabled(SceneId::Name(scene), item.id),
                    )
                    .await,
            )?
            .unwrap_or(true);
            let locked = tolerate_rejection(
                session
                    .request(
                        "GetSceneItemLocked",
                        obs.scene_items().locked(SceneId::Name(scene), item.id),
                    )
                    .await,
            )?
            .unwrap_or(false);

            let kind = item.input_kind;
            let audio_db = if crate::catalog::is_audio_kind(kind.as_deref()) {
                match db_cache.get(&item.source_name) {
                    Some(db) => Some(*db),
                    None => {
                        let fetched = tolerate_rejection(
                            session
                                .request(
                                    "GetInputVolume",
                                    obs.inputs().volume(InputId::Name(&item.source_name)),
                                )
                                .await,
                        )?
                        .map(|v| v.db);
                        if let Some(db) = fetched {
                            db_cache.insert(item.source_name.clone(), db);
                        }
                        fetched
                    }
                }
            } else {
                None
            };
            infos.push(SourceInfo {
                name: item.source_name,
                visible,
                locked,
                audio_db,
                kind,
            });
        }
        sources_by_scene.insert(scene.clone(), infos);
    }

    let audio_inputs: Vec<String> = tolerate_rejection(
        session
            .request("GetInputList", obs.inputs().list(None))
            .await,
    )?
    .map(|inputs| inputs.into_iter().map(|i| i.id.name).collect())
    .unwrap_or_default();

    let stream_status = tolerate_rejection(
        session
            .request("GetStreamStatus", obs.streaming().status())
            .await,
    )?;
    let record_status = tolerate_rejection(
        session
            .request("GetRecordStatus", obs.recording().status())
            .await,
    )?;

    let replaced = mirror.replace_snapshot(
        started_at,
        FetchedSnapshot {
            scenes,
            audio_inputs,
            sources: sources_by_scene,
            current_scene,
            current_preview_scene,
            ids: fresh_ids,
        },
    );
    if !replaced {
        return Ok(false);
    }

    if let Some(status) = stream_status {
        let mut snapshot = health_state.write_recover();
        for delta in crate::events::apply_stream_status_update(&status, &mut snapshot) {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "having no subscribers is a normal state"
            )]
            let _ = health_tx.send(delta);
        }
    }
    if let Some(status) = record_status {
        let mut snapshot = health_state.write_recover();
        for delta in crate::events::apply_record_status_update(&status, &mut snapshot) {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "having no subscribers is a normal state"
            )]
            let _ = health_tx.send(delta);
        }
    }
    Ok(true)
}

pub(crate) async fn reconcile_catalog_topology(
    session: &LiveSession,
    mirror: &ObsMirror,
) -> Result<(), ObsError> {
    for _ in 0..TOPOLOGY_FETCH_ATTEMPTS {
        if reconcile_catalog_topology_once(session, mirror).await? {
            return Ok(());
        }
    }
    tracing::warn!(
        "OBS catalog kept changing during every reconcile attempt; keeping the live mirror"
    );
    Ok(())
}

async fn reconcile_catalog_topology_once(
    session: &LiveSession,
    mirror: &ObsMirror,
) -> Result<bool, ObsError> {
    use obws::requests::scenes::SceneId;

    let obs = session.obs();
    let started_at = mirror.revision();

    let scenes: Vec<String> =
        tolerate_rejection(session.request("GetSceneList", obs.scenes().list()).await)?
            .map(|list| list.scenes.iter().map(|s| s.id.name.clone()).collect())
            .unwrap_or_default();

    let fetched_current_scene: Option<String> = tolerate_rejection(
        session
            .request(
                "GetCurrentProgramScene",
                obs.scenes().current_program_scene(),
            )
            .await,
    )?
    .map(|s| s.id.name);

    let fetched_current_preview_scene: Option<String> = tolerate_rejection(
        session
            .request(
                "GetCurrentPreviewScene",
                obs.scenes().current_preview_scene(),
            )
            .await,
    )?
    .map(|s| s.id.name);

    let audio_inputs: Vec<String> = tolerate_rejection(
        session
            .request("GetInputList", obs.inputs().list(None))
            .await,
    )?
    .map(|inputs| inputs.into_iter().map(|i| i.id.name).collect())
    .unwrap_or_default();

    let mut fetched_topology: HashMap<String, Vec<(String, Option<String>)>> = HashMap::new();
    let mut fresh_ids = SceneItemIds::default();
    for scene in &scenes {
        let Some(items) = tolerate_rejection(
            session
                .request(
                    "GetSceneItemList",
                    obs.scene_items().list(SceneId::Name(scene)),
                )
                .await,
        )?
        else {
            continue;
        };
        let mut entries = Vec::with_capacity(items.len());
        for item in items {
            fresh_ids.insert(scene, item.id, &item.source_name);
            entries.push((item.source_name, item.input_kind));
        }
        fetched_topology.insert(scene.clone(), entries);
    }

    Ok(mirror.reconcile(
        started_at,
        FetchedTopology {
            scenes,
            audio_inputs,
            sources: fetched_topology,
            current_scene: fetched_current_scene,
            current_preview_scene: fetched_current_preview_scene,
        },
        fresh_ids,
    ))
}
