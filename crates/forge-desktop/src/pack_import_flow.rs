use std::path::PathBuf;

use forge_components::{PushToast, ToastKind, tr};
use forge_transfer::{discard_staged_pack, plan_pack_import, stage_pack};
use gpui::Context;

use crate::async_bridge;
use crate::pack_services::PackServices;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PackPlanCount {
    pub name: String,
    pub items: usize,
}

pub struct PackImportFlow {
    services: PackServices,
    data_dir: PathBuf,
    planning: bool,
}

impl PackImportFlow {
    pub fn new(services: PackServices, data_dir: PathBuf) -> Self {
        Self {
            services,
            data_dir,
            planning: false,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.planning
    }

    pub fn open(&mut self, bundle: PathBuf, cx: &mut Context<Self>) {
        if self.planning {
            return;
        }
        self.planning = true;
        let services = self.services.clone();
        let data_dir = self.data_dir.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move { plan_count(services, bundle, data_dir).await },
            |this, result, cx| this.planned(result, cx),
            cx,
        );
        cx.notify();
    }

    fn planned(&mut self, result: Result<PackPlanCount, String>, cx: &mut Context<Self>) {
        self.planning = false;
        match result {
            Ok(count) => cx.push_toast(
                ToastKind::Info,
                tr!(
                    "pack_import_planned",
                    name = count.name.as_str(),
                    count = count.items.to_string()
                ),
            ),
            Err(message) => {
                tracing::warn!(error = %message, "pack import planning failed");
                cx.push_toast(
                    ToastKind::Error,
                    tr!("home_import_failed", error = message.as_str()),
                );
            }
        }
        cx.notify();
    }
}

async fn plan_count(
    services: PackServices,
    bundle: PathBuf,
    data_dir: PathBuf,
) -> Result<PackPlanCount, String> {
    let staged = stage_pack(bundle, data_dir)
        .await
        .map_err(|error| error.to_string())?;
    let counted = services.load_source().await.map(|source| {
        let plan = plan_pack_import(&staged, &source.inventory, services.registries(&source));
        PackPlanCount {
            name: staged.manifest().name.clone(),
            items: plan.items().len(),
        }
    });
    if let Err(error) = discard_staged_pack(staged).await {
        tracing::warn!(%error, "staged pack could not be discarded");
    }
    counted
}
