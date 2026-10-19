use forge_runtime::MembershipOutcome;
use forge_storage::OverlayId;
use forge_transfer::{PackItemKind, PackWrittenItem};

use crate::pack_services::PackServices;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LiveTargets {
    pub queues: Vec<String>,
    pub scripts: bool,
    pub overlays: Vec<OverlayId>,
}

impl LiveTargets {
    pub(crate) fn of(written: &[PackWrittenItem]) -> Self {
        let mut targets = Self::default();
        for item in written {
            match item.key.kind {
                PackItemKind::Queue => targets.queues.push(item.local_id.clone()),
                PackItemKind::Script => targets.scripts = true,
                PackItemKind::Overlay => targets.overlays.push(OverlayId::new(&item.local_id)),
                PackItemKind::Media
                | PackItemKind::Global
                | PackItemKind::Clip
                | PackItemKind::Trigger
                | PackItemKind::Action => {}
            }
        }
        targets
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LiveRefresh {
    Applied,
    NeedsRestart,
}

pub(crate) async fn refresh_live(services: &PackServices, targets: LiveTargets) -> LiveRefresh {
    let queues = refresh_queues(services, &targets.queues).await;
    let scripts = !targets.scripts || refresh_scripts(services).await;
    let overlays = refresh_overlays(services, &targets.overlays).await;
    if queues && scripts && overlays {
        LiveRefresh::Applied
    } else {
        LiveRefresh::NeedsRestart
    }
}

async fn refresh_queues(services: &PackServices, ids: &[String]) -> bool {
    if ids.is_empty() {
        return true;
    }
    let queues = match services.backend.queue_repo().list().await {
        Ok(queues) => queues,
        Err(error) => {
            tracing::warn!(%error, "imported queues could not be read back");
            return false;
        }
    };
    let mut live = true;
    for id in ids {
        let Some(queue) = queues.iter().find(|queue| queue.id.to_string() == *id) else {
            tracing::warn!(queue = %id, "imported queue is missing after the import");
            live = false;
            continue;
        };
        let outcome = match services.scheduler.register(queue.clone()).await {
            Ok(MembershipOutcome::AlreadyRegistered) => {
                services.scheduler.reconfigure(queue.clone()).await
            }
            other => other,
        };
        match outcome {
            Ok(MembershipOutcome::Applied | MembershipOutcome::AlreadyRegistered) => {}
            Ok(MembershipOutcome::NotFound) => {
                tracing::warn!(queue = %id, "imported queue is unknown to the scheduler");
                live = false;
            }
            Err(error) => {
                tracing::warn!(queue = %id, %error, "imported queue could not go live");
                live = false;
            }
        }
    }
    live
}

async fn refresh_scripts(services: &PackServices) -> bool {
    match services.scripts.load_all(services.backend.as_ref()).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(%error, "scripts could not be reloaded after the pack import");
            false
        }
    }
}

async fn refresh_overlays(services: &PackServices, ids: &[OverlayId]) -> bool {
    let mut live = true;
    for id in ids {
        if let Err(error) = services.overlays.materialize(id).await {
            tracing::warn!(overlay = %id, %error, "imported overlay could not be regenerated");
            live = false;
        }
    }
    live
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use forge_storage::{OverlayId, ScriptRecord, ScriptRepo};
    use forge_transfer::{PackItemKey, PackItemKind, PackWrittenItem};
    use forge_types::{ScriptContract, ScriptId};
    use time::OffsetDateTime;

    use super::{LiveRefresh, LiveTargets, refresh_live};
    use crate::test_support::pack_flow::{PackWorld, pack_world, queue_named};
    use crate::test_support::runtime;

    const KNOWN_OVERLAY_KIND: &str = "overlay.blank";
    const RETIRED_OVERLAY_KIND: &str = "overlay.retired";

    fn written(kind: PackItemKind, local_id: &str) -> PackWrittenItem {
        PackWrittenItem {
            key: PackItemKey {
                kind,
                id: format!("pack-{local_id}"),
            },
            local_id: local_id.to_owned(),
            replaced: false,
        }
    }

    fn refresh(
        rt: &tokio::runtime::Runtime,
        world: &PackWorld,
        targets: LiveTargets,
    ) -> LiveRefresh {
        rt.block_on(refresh_live(&world.services, targets))
    }

    fn stored_overlay(rt: &tokio::runtime::Runtime, world: &PackWorld, kind_id: &str) -> OverlayId {
        rt.block_on(
            world
                .provider()
                .overlay_repo()
                .create("Imported", kind_id, 1),
        )
        .unwrap()
        .id
    }

    #[test]
    fn live_targets_route_queues_scripts_and_overlays_and_ignore_the_rest() {
        let items = [
            written(PackItemKind::Queue, "q1"),
            written(PackItemKind::Script, "s1"),
            written(PackItemKind::Overlay, "o1"),
            written(PackItemKind::Action, "a1"),
            written(PackItemKind::Trigger, "t1"),
            written(PackItemKind::Global, "g1"),
            written(PackItemKind::Clip, "c1"),
            written(PackItemKind::Media, "m1"),
        ];

        assert_eq!(
            LiveTargets::of(&items),
            LiveTargets {
                queues: vec!["q1".to_owned()],
                scripts: true,
                overlays: vec![OverlayId::new("o1")],
            }
        );
    }

    #[test]
    fn an_imported_queue_unknown_to_the_scheduler_is_registered() {
        let rt = runtime();
        let world = pack_world(&rt);
        let queue = queue_named("Imported queue");
        rt.block_on(world.provider().queue_repo().save(&queue))
            .unwrap();

        let live = refresh(
            &rt,
            &world,
            LiveTargets {
                queues: vec![queue.id.to_string()],
                ..LiveTargets::default()
            },
        );
        let states = rt
            .block_on(world.services.scheduler.queue_states())
            .unwrap();

        assert_eq!(
            (live, states.contains_key(&queue.id)),
            (LiveRefresh::Applied, true)
        );
    }

    #[test]
    fn a_replaced_queue_the_scheduler_already_runs_stays_live() {
        let rt = runtime();
        let world = pack_world(&rt);
        let mut queue = queue_named("Replaced queue");
        rt.block_on(world.services.scheduler.register(queue.clone()))
            .unwrap();
        queue.concurrency = 4;
        rt.block_on(world.provider().queue_repo().save(&queue))
            .unwrap();

        let live = refresh(
            &rt,
            &world,
            LiveTargets {
                queues: vec![queue.id.to_string()],
                ..LiveTargets::default()
            },
        );

        assert_eq!(live, LiveRefresh::Applied);
    }

    #[test]
    fn a_written_queue_missing_from_storage_needs_a_restart() {
        let rt = runtime();
        let world = pack_world(&rt);

        let live = refresh(
            &rt,
            &world,
            LiveTargets {
                queues: vec![queue_named("Vanished").id.to_string()],
                ..LiveTargets::default()
            },
        );

        assert_eq!(live, LiveRefresh::NeedsRestart);
    }

    #[test]
    fn a_written_script_reloads_the_script_registry() {
        let rt = runtime();
        let world = pack_world(&rt);
        let now = OffsetDateTime::now_utc();
        rt.block_on(ScriptRepo::save(
            world.provider().as_ref(),
            ScriptRecord {
                id: ScriptId::new(),
                name: "imported_greet".to_owned(),
                body: "let greeting = 1;".to_owned(),
                contract: ScriptContract::default(),
                body_hash: String::new(),
                enabled: true,
                created_at: now,
                last_modified: now,
            },
        ))
        .unwrap();

        let live = refresh(
            &rt,
            &world,
            LiveTargets {
                scripts: true,
                ..LiveTargets::default()
            },
        );
        let loaded = rt
            .block_on(world.services.scripts.get_by_name("imported_greet"))
            .is_some();

        assert_eq!((live, loaded), (LiveRefresh::Applied, true));
    }

    #[test]
    fn a_written_overlay_is_materialized_under_the_overlay_root() {
        let rt = runtime();
        let world = pack_world(&rt);
        let overlay = stored_overlay(&rt, &world, KNOWN_OVERLAY_KIND);

        let live = refresh(
            &rt,
            &world,
            LiveTargets {
                overlays: vec![overlay.clone()],
                ..LiveTargets::default()
            },
        );
        let page_dir = world.overlay_root().join(overlay.as_str());

        assert_eq!((live, page_dir.is_dir()), (LiveRefresh::Applied, true));
    }

    #[test]
    fn one_overlay_that_cannot_be_materialized_needs_a_restart_even_when_the_rest_went_live() {
        let rt = runtime();
        let world = pack_world(&rt);
        let queue = queue_named("Healthy queue");
        rt.block_on(world.provider().queue_repo().save(&queue))
            .unwrap();
        let healthy = stored_overlay(&rt, &world, KNOWN_OVERLAY_KIND);
        let retired = stored_overlay(&rt, &world, RETIRED_OVERLAY_KIND);

        let live = refresh(
            &rt,
            &world,
            LiveTargets {
                queues: vec![queue.id.to_string()],
                scripts: true,
                overlays: vec![retired, healthy.clone()],
            },
        );
        let healthy_dir = world.overlay_root().join(healthy.as_str());

        assert_eq!(
            (live, healthy_dir.is_dir()),
            (LiveRefresh::NeedsRestart, true)
        );
    }
}
