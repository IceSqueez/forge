use std::path::PathBuf;
use std::sync::Arc;

use forge_components::{PushToast, ToastKind, tr};
use forge_transfer::{BundleKind, TransferError, read_bundle_kind};
use gpui::{App, AppContext, Context, Entity, EventEmitter, Global, Subscription};

use crate::async_bridge::{self, DIALOG_CANCELLED, DialogFilter};
use crate::backup_flow::{BackupFlow, BackupTarget};
use crate::home_stats::HomeStats;
use crate::import_busy::refuse_while_busy;
use crate::pack_export::PackExportFlow;
use crate::pack_import_flow::{PackImportFlow, PackImported};
use crate::pack_import_live::SetupLiveHandles;
use crate::pack_services::PackServices;
use crate::restore_flow::{LEGACY_ARCHIVE_DIALOG_EXTENSION, RestoreFlow};
use crate::restore_problem::RestoreProblem;
use crate::setup_export::SetupExportFlow;
use crate::setup_import_flow::{SetupImportFlow, SetupImported};
use crate::shell::refresh_dashboard_stats;

pub struct TransferFlows {
    pub restore: Entity<RestoreFlow>,
    pub backup: Entity<BackupFlow>,
    pub pack_export: Entity<PackExportFlow>,
    pub pack_import: Entity<PackImportFlow>,
    pub setup_export: Entity<SetupExportFlow>,
    pub setup_import: Entity<SetupImportFlow>,
    rt_handle: tokio::runtime::Handle,
    routing: bool,
    _subs: [Subscription; 8],
}

pub struct TransferRoute(pub Entity<TransferFlows>);

impl Global for TransferRoute {}

impl TransferFlows {
    pub fn new(
        stats: Entity<HomeStats>,
        pack: PackServices,
        data_dir: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        let rt_handle = pack.rt_handle.clone();
        let restore = cx.new(|_| RestoreFlow::new(rt_handle.clone(), data_dir.clone()));
        let backup = cx.new(|_| {
            BackupFlow::new(
                Arc::clone(&pack.backend),
                rt_handle.clone(),
                data_dir.clone(),
                BackupTarget::ChosenFile,
            )
        });
        let pack_export = cx.new(|_| PackExportFlow::new(pack.clone()));
        let pack_import = cx.new(|_| PackImportFlow::new(pack.clone(), data_dir.clone()));
        let setup_export = cx.new(|_| SetupExportFlow::new(pack.clone()));
        let setup_import = cx.new(|_| SetupImportFlow::new(pack.clone(), data_dir));
        let subs = [
            cx.observe(&restore, |_, _, cx| cx.notify()),
            cx.observe(&backup, |_, _, cx| cx.notify()),
            cx.observe(&pack_export, |_, _, cx| cx.notify()),
            cx.observe(&pack_import, |_, _, cx| cx.notify()),
            cx.observe(&setup_export, |_, _, cx| cx.notify()),
            cx.observe(&setup_import, |_, _, cx| cx.notify()),
            Self::refresh_stats_on::<PackImportFlow, PackImported>(&pack_import, &stats, &pack, cx),
            Self::refresh_stats_on::<SetupImportFlow, SetupImported>(
                &setup_import,
                &stats,
                &pack,
                cx,
            ),
        ];
        Self {
            restore,
            backup,
            pack_export,
            pack_import,
            setup_export,
            setup_import,
            rt_handle,
            routing: false,
            _subs: subs,
        }
    }

    #[must_use]
    pub fn with_setup_live(self, live: SetupLiveHandles, cx: &mut Context<Self>) -> Self {
        self.setup_import
            .update(cx, |setup_import, _| setup_import.set_live(live));
        self
    }

    pub fn route(cx: &App) -> Option<Entity<TransferFlows>> {
        cx.try_global::<TransferRoute>()
            .map(|route| route.0.clone())
    }

    fn refresh_stats_on<Flow, Done>(
        flow: &Entity<Flow>,
        stats: &Entity<HomeStats>,
        pack: &PackServices,
        cx: &mut Context<Self>,
    ) -> Subscription
    where
        Flow: EventEmitter<Done>,
        Done: 'static,
    {
        let stats = stats.clone();
        let backend = Arc::clone(&pack.backend);
        let triggers = Arc::clone(&pack.triggers);
        let rt_handle = pack.rt_handle.clone();
        cx.subscribe(flow, move |_, _, _: &Done, cx| {
            let stats = stats.clone();
            let backend = Arc::clone(&backend);
            let triggers = Arc::clone(&triggers);
            let rt_handle = rt_handle.clone();
            cx.spawn(async move |_, cx| {
                refresh_dashboard_stats(stats, backend, triggers, rt_handle, cx).await;
            })
            .detach();
        })
    }

    pub fn is_busy(&self, cx: &App) -> bool {
        self.routing
            || self.restore.read(cx).is_busy()
            || self.backup.read(cx).is_busy()
            || self.pack_export.read(cx).is_busy()
            || self.pack_import.read(cx).is_busy()
            || self.setup_export.read(cx).is_busy()
            || self.setup_import.read(cx).is_busy()
    }

    pub fn request_import(&mut self, cx: &mut Context<Self>) {
        if self.is_busy(cx) {
            refuse_while_busy(cx);
            return;
        }
        self.routing = true;
        let filter = DialogFilter {
            name: tr!("home_import_dialog_filter"),
            extensions: vec![
                forge_transfer::BUNDLE_EXTENSION,
                LEGACY_ARCHIVE_DIALOG_EXTENSION,
            ],
        };
        async_bridge::spawn_dialog(
            &self.rt_handle,
            async_bridge::pick_file(Some(filter)),
            |this, result, cx| this.import_picked(result, cx),
            cx,
        );
        cx.notify();
    }

    fn import_picked(&mut self, result: Result<PathBuf, String>, cx: &mut Context<Self>) {
        match result {
            Ok(path) => {
                let bundle = path.clone();
                async_bridge::run_async(
                    &self.rt_handle,
                    async move { read_bundle_kind(bundle).await },
                    move |this, kind, cx| this.route_import(path, kind, cx),
                    cx,
                );
            }
            Err(message) => {
                self.routing = false;
                if message != DIALOG_CANCELLED {
                    tracing::warn!(error = %message, "import file picker failed");
                    cx.push_toast(
                        ToastKind::Error,
                        tr!("home_import_failed", error = message.as_str()),
                    );
                }
            }
        }
        cx.notify();
    }

    pub fn route_import(
        &mut self,
        path: PathBuf,
        kind: Result<BundleKind, TransferError>,
        cx: &mut Context<Self>,
    ) {
        self.routing = false;
        match kind {
            Ok(BundleKind::Full) => self
                .restore
                .update(cx, |restore, cx| restore.open(path, cx)),
            Ok(BundleKind::Pack) => self
                .pack_import
                .update(cx, |pack_import, cx| pack_import.open(path, cx)),
            Ok(BundleKind::Setup) => self
                .setup_import
                .update(cx, |setup_import, cx| setup_import.open(path, cx)),
            Err(error) => {
                tracing::warn!(%error, "import bundle kind unreadable");
                cx.push_toast(ToastKind::Error, RestoreProblem::of(&error).message());
            }
        }
        cx.notify();
    }

    pub fn export_full_backup(&mut self, cx: &mut Context<Self>) {
        if self.is_busy(cx) {
            return;
        }
        self.backup.update(cx, |backup, cx| backup.start(cx));
        cx.notify();
    }

    pub fn export_pack(&mut self, cx: &mut Context<Self>) {
        if self.is_busy(cx) {
            return;
        }
        self.pack_export
            .update(cx, |pack_export, cx| pack_export.start(Vec::new(), cx));
        cx.notify();
    }

    pub fn export_setup(&mut self, cx: &mut Context<Self>) {
        if self.is_busy(cx) {
            return;
        }
        self.setup_export
            .update(cx, |setup_export, cx| setup_export.start(cx));
        cx.notify();
    }
}
