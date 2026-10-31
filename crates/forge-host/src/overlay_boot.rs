use std::sync::Arc;

use forge_net::TaskOwner;
use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    ActionsService, Catalog, DonationIngest, DonationOverlayAudience, EventBus, MaterializePass,
    OverlayConnectFanout, OverlayConnectListener, OverlaySampleSource, OverlaySamples,
    OverlayServiceHandle, RuntimeError, spawn_latest_overlay_feed,
};
use forge_server::DeliveryHub;
use forge_storage::DataProvider;
use forge_transfer::sweep_abandoned_overlay_imports;

use crate::boot_warnings::{BootWarning, BootWarnings};
use crate::host_handles::OverlayHandles;

pub(crate) fn build_overlay_kinds(warnings: &mut BootWarnings) -> Arc<OverlayKindRegistry> {
    let mut kinds = OverlayKindRegistry::new();
    if let Err(e) = register_builtin_kinds(&mut kinds) {
        tracing::error!(error = %e, "overlay type registration failed");
        warnings.record(BootWarning::OverlayTypes);
    }
    Arc::new(kinds)
}

pub(crate) struct OverlayInputs<'a> {
    pub(crate) backend: &'a Arc<dyn DataProvider>,
    pub(crate) bus: &'a Arc<EventBus>,
    pub(crate) services: &'a TaskOwner,
    pub(crate) catalog: &'a Arc<Catalog>,
    pub(crate) sub_actions: &'a Arc<SubActionRegistry>,
    pub(crate) triggers: &'a Arc<TriggerRegistry>,
    pub(crate) donations: &'a Arc<DonationIngest>,
    pub(crate) delivery_hub: &'a DeliveryHub,
    pub(crate) service: OverlayServiceHandle,
    pub(crate) kinds: Arc<OverlayKindRegistry>,
}

pub(crate) async fn start_overlays(
    inputs: OverlayInputs<'_>,
) -> (OverlayHandles, Result<MaterializePass, RuntimeError>) {
    let OverlayInputs {
        backend,
        bus,
        services,
        catalog,
        sub_actions,
        triggers,
        donations,
        delivery_hub,
        service,
        kinds,
    } = inputs;
    let actions = Arc::new(ActionsService::new(
        backend.action_repo(),
        backend.queue_repo(),
        backend.history_repo(),
        backend.trigger_instance_repo(),
    ));
    let samples: Arc<dyn OverlaySampleSource> = Arc::new(OverlaySamples::new(
        Arc::clone(&actions),
        Arc::clone(sub_actions),
        Arc::clone(triggers),
    ));
    spawn_latest_overlay_feed(bus, service.clone(), services);
    let donation_audience = DonationOverlayAudience::new(
        Arc::clone(catalog),
        service.clone(),
        Arc::clone(sub_actions),
    );
    let mut connect_listeners = vec![Arc::new(service.clone()) as Arc<dyn OverlayConnectListener>];
    if let Some(waker) = donations.catch_up_waker() {
        donation_audience.forward_changes_to(&waker, services);
        connect_listeners.push(Arc::new(waker) as Arc<dyn OverlayConnectListener>);
    }
    delivery_hub
        .set_overlay_connect_listener(Arc::new(OverlayConnectFanout::new(connect_listeners)));
    donations.spawn_catch_up_release(Arc::new(donation_audience), services);
    match sweep_abandoned_overlay_imports(backend.overlay_repo().as_ref(), &service.root().await)
        .await
    {
        Ok(0) => {}
        Ok(removed) => tracing::info!(removed, "abandoned overlay imports removed"),
        Err(error) => tracing::warn!(%error, "abandoned overlay imports could not be swept"),
    }
    let pass = service.materialize_all(samples.as_ref()).await;
    match &pass {
        Ok(pass) => tracing::info!(
            materialized = pass.materialized,
            unavailable = pass.unavailable,
            failed = pass.failed,
            "overlay pages materialized"
        ),
        Err(e) => tracing::error!(error = %e, "overlay materialization pass failed"),
    }
    (
        OverlayHandles {
            service,
            samples,
            actions,
            kinds,
        },
        pass,
    )
}
