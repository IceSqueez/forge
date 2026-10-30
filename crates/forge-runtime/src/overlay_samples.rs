use std::sync::Arc;

use async_trait::async_trait;
use forge_overlay::{SampleContext, SampleTrigger, sample_context};
use forge_registry::{
    KindPlatformContract, SubActionRegistry, TriggerRegistry, declared_variables,
};
use forge_storage::OverlayId;

use crate::actions::ActionsService;

#[async_trait]
pub trait OverlaySampleSource: Send + Sync {
    async fn sample(&self, id: &OverlayId) -> SampleContext;
}

pub struct OverlaySamples {
    actions: Arc<ActionsService>,
    sub_actions: Arc<SubActionRegistry>,
    triggers: Arc<TriggerRegistry>,
}

impl OverlaySamples {
    pub fn new(
        actions: Arc<ActionsService>,
        sub_actions: Arc<SubActionRegistry>,
        triggers: Arc<TriggerRegistry>,
    ) -> Self {
        Self {
            actions,
            sub_actions,
            triggers,
        }
    }
}

#[async_trait]
impl OverlaySampleSource for OverlaySamples {
    async fn sample(&self, id: &OverlayId) -> SampleContext {
        let feeds = match self.actions.overlay_feeds(id, &self.sub_actions).await {
            Ok(feeds) => feeds,
            Err(error) => {
                tracing::warn!(overlay = %id, %error, "what feeds this overlay is unreadable; its sample stays neutral");
                return SampleContext::neutral();
            }
        };

        let feeding: Vec<SampleTrigger> = feeds
            .iter()
            .flat_map(|feed| feed.triggers.iter())
            .map(|instance| feeding_trigger(&self.triggers, &instance.kind_id))
            .collect();
        sample_context(&feeding)
    }
}

fn feeding_trigger(triggers: &TriggerRegistry, kind_id: &str) -> SampleTrigger {
    let descriptor = triggers.get(kind_id);
    SampleTrigger {
        kind_id: kind_id.to_owned(),
        contract: descriptor.map_or(KindPlatformContract::Universal, |held| {
            held.platform_contract()
        }),
        variables: descriptor.and_then(declared_variables).unwrap_or_default(),
    }
}
