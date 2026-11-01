use crate::shared::option_choices::ChoiceOption;
use forge_host::donation_services::DonationServices;
use forge_platform_core::IntegrationCategory;
use futures_util::StreamExt as _;
use gpui::{App, Entity};

use crate::integrations::platforms::{PlatformConnectivity, apply_connection};
use forge_host::integration_catalog::declarations;

pub(crate) fn donation_provider_options() -> Vec<ChoiceOption> {
    declarations()
        .into_iter()
        .filter(|declaration| declaration.category == IntegrationCategory::DONATIONS)
        .map(|declaration| ChoiceOption::new(declaration.id.as_str(), declaration.brand_name))
        .collect()
}

pub(crate) fn mirror_donation_connectivity(
    services: &DonationServices,
    connectivity: &Entity<PlatformConnectivity>,
    cx: &mut App,
) {
    for service in services.all() {
        let id = service.id().clone();
        let status = service.status();
        let mut deltas = service.health().stream();
        let connectivity = connectivity.clone();
        apply_connection(&connectivity, &id, status.as_ref(), cx);
        cx.spawn(async move |cx| {
            while deltas.next().await.is_some() {
                cx.update(|cx| apply_connection(&connectivity, &id, status.as_ref(), cx));
            }
        })
        .detach();
    }
}
