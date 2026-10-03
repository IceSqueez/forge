use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::DONATION_RECEIVED_KIND;
use forge_registry::SubActionRegistry;
use forge_storage::OverlayId;
use forge_types::LATEST_DONATION_SLOT;
use tracing::warn;

use crate::catalog::Catalog;
use crate::donations::catch_up::{CatchUpWaker, DonationAudience};
use crate::overlay_service::{OverlayServiceError, OverlayServiceHandle};
use crate::sub_action_runners::{OverlaySendTarget, overlay_send_targets};

pub struct DonationOverlayAudience {
    catalog: Arc<Catalog>,
    overlays: OverlayServiceHandle,
    sub_actions: Arc<SubActionRegistry>,
}

impl DonationOverlayAudience {
    pub fn new(
        catalog: Arc<Catalog>,
        overlays: OverlayServiceHandle,
        sub_actions: Arc<SubActionRegistry>,
    ) -> Self {
        Self {
            catalog,
            overlays,
            sub_actions,
        }
    }

    pub fn forward_changes_to(&self, waker: &CatchUpWaker) {
        let mut catalog_changes = self.catalog.changes();
        let catalog_waker = waker.clone();
        tokio::spawn(async move {
            while catalog_changes.changed().await.is_some() {
                catalog_waker.wake();
            }
        });
        let mut overlay_changes = self.overlays.definition_changes();
        let overlay_waker = waker.clone();
        tokio::spawn(async move {
            while overlay_changes.changed().await {
                overlay_waker.wake();
            }
        });
    }

    async fn donation_overlays(&self) -> Result<Vec<OverlayId>, OverlayServiceError> {
        let snapshot = self.catalog.current().await?;
        let targeted: HashSet<String> = snapshot
            .binding_indexes_for_kinds([DONATION_RECEIVED_KIND])
            .into_iter()
            .filter_map(|index| snapshot.binding(index))
            .flat_map(|binding| {
                overlay_send_targets(&binding.action.sub_actions, &self.sub_actions)
            })
            .filter_map(|target| match target {
                OverlaySendTarget::Overlay(identity) => Some(identity),
                OverlaySendTarget::Unresolved(_) => None,
            })
            .collect();
        Ok(self
            .overlays
            .enabled_overlays()
            .await?
            .into_iter()
            .filter(|overlay| {
                targeted.contains(overlay.id.as_str())
                    || overlay.latest_slot.as_deref() == Some(LATEST_DONATION_SLOT)
            })
            .map(|overlay| overlay.id)
            .collect())
    }
}

#[async_trait]
impl DonationAudience for DonationOverlayAudience {
    async fn is_listening(&self) -> bool {
        let overlays = match self.donation_overlays().await {
            Ok(overlays) => overlays,
            Err(error) => {
                warn!(%error, "donation overlays unreadable; catch-up donations stay held");
                return false;
            }
        };
        if overlays.is_empty() {
            return true;
        }
        for overlay in &overlays {
            if self.overlays.receivers(overlay).await.sources > 0 {
                return true;
            }
        }
        false
    }
}
