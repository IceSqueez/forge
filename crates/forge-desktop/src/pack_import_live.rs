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
