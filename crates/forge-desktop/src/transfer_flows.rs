use std::path::PathBuf;
use std::sync::Arc;

use forge_components::{PushToast, ToastKind, tr};
use forge_transfer::{BundleKind, TransferError, read_bundle_kind};
use forge_types::ActionId;
use gpui::{AnyView, App, AppContext, Context, Entity, EventEmitter, Global, Subscription};

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
        self.start_backup(BackupTarget::ChosenFile, cx);
    }

    pub fn backup_to_backups_dir(&mut self, cx: &mut Context<Self>) {
        self.start_backup(BackupTarget::BackupsDir, cx);
    }

    fn start_backup(&mut self, target: BackupTarget, cx: &mut Context<Self>) {
        if self.refused_while_busy(cx) {
            return;
        }
        self.backup
            .update(cx, |backup, cx| backup.start(target, cx));
        cx.notify();
    }

    pub fn export_pack(&mut self, cx: &mut Context<Self>) {
        self.export_pack_of(Vec::new(), cx);
    }

    pub fn export_pack_of(&mut self, preselected: Vec<ActionId>, cx: &mut Context<Self>) {
        if self.refused_while_busy(cx) {
            return;
        }
        self.pack_export
            .update(cx, |pack_export, cx| pack_export.start(preselected, cx));
        cx.notify();
    }

    pub fn export_setup(&mut self, cx: &mut Context<Self>) {
        if self.refused_while_busy(cx) {
            return;
        }
        self.setup_export
            .update(cx, |setup_export, cx| setup_export.start(cx));
        cx.notify();
    }

    pub fn start_restore(&mut self, cx: &mut Context<Self>) {
        if self.refused_while_busy(cx) {
            return;
        }
        self.restore.update(cx, |restore, cx| restore.start(cx));
        cx.notify();
    }

    pub fn views(&self) -> [AnyView; 6] {
        [
            self.backup.clone().into(),
            self.pack_export.clone().into(),
            self.pack_import.clone().into(),
            self.setup_export.clone().into(),
            self.setup_import.clone().into(),
            self.restore.clone().into(),
        ]
    }

    fn refused_while_busy(&self, cx: &mut Context<Self>) -> bool {
        let busy = self.is_busy(cx);
        if busy {
            tracing::info!("transfer start ignored while another transfer runs");
            cx.push_toast(ToastKind::Warn, tr!("transfer_busy_already"));
        }
        busy
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use forge_components::{ToastKind, Toasts, tr};
    use forge_storage::Language;
    use forge_transfer::{BundleKind, read_bundle_kind, setup_staging_root};
    use forge_types::ActionId;
    use gpui::{AppContext, Entity, TestAppContext};

    use super::{TransferFlows, TransferRoute};
    use crate::event_loss::EventLoss;
    use crate::home::HomeView;
    use crate::home_stats::HomeStats;
    use crate::i18n::install_language;
    use crate::pack_services::PackServices;
    use crate::test_support::pack_flow::{
        PackWorld, export_pack, pack_world, queue_named, seed_queued_action,
    };
    use crate::test_support::{install_presentation, runtime};

    const SETTLE_DEADLINE: Duration = Duration::from_secs(30);

    struct Rig {
        rt: tokio::runtime::Runtime,
        world: PackWorld,
        services: PackServices,
        flows: Entity<TransferFlows>,
    }

    fn rig_with(cx: &mut TestAppContext, background_runs: bool) -> Rig {
        install_language(Language::En);
        install_presentation(cx);
        cx.update(|cx| cx.set_global(Toasts::new()));
        let rt = runtime();
        let world = pack_world(&rt);
        let services = if background_runs {
            world.services.clone()
        } else {
            let stopped = runtime();
            let detached = world.with_runtime(&stopped);
            drop(stopped);
            detached
        };
        let stats = cx.new(|_| HomeStats::new());
        let data_dir = world.data_dir();
        let pack = services.clone();
        let flows = cx.new(|cx| TransferFlows::new(stats, pack, data_dir, cx));
        Rig {
            rt,
            world,
            services,
            flows,
        }
    }

    fn stalled_rig(cx: &mut TestAppContext) -> Rig {
        rig_with(cx, false)
    }

    fn busy_flows(cx: &mut TestAppContext, rig: &Rig) -> (bool, bool, bool) {
        rig.flows.read_with(cx, |flows, cx| {
            (
                flows.restore.read(cx).is_busy(),
                flows.pack_import.read(cx).is_busy(),
                flows.setup_import.read(cx).is_busy(),
            )
        })
    }

    fn toasts(cx: &mut TestAppContext) -> Vec<(ToastKind, String)> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| {
                    (
                        toast.kind,
                        toast.message.replace(['\u{2068}', '\u{2069}'], ""),
                    )
                })
                .collect()
        })
    }

    type Start = fn(&mut TransferFlows, &mut gpui::Context<TransferFlows>);
    type Probe = fn(&TransferFlows, &gpui::App) -> bool;

    fn starts() -> [(&'static str, Start, Probe); 5] {
        [
            (
                "backup",
                |flows, cx| flows.export_full_backup(cx),
                |flows, cx| flows.backup.read(cx).is_busy(),
            ),
            (
                "export_pack",
                |flows, cx| flows.export_pack(cx),
                |flows, cx| flows.pack_export.read(cx).is_busy(),
            ),
            (
                "export_pack_of",
                |flows, cx| flows.export_pack_of(vec![ActionId::new()], cx),
                |flows, cx| flows.pack_export.read(cx).is_busy(),
            ),
            (
                "export_setup",
                |flows, cx| flows.export_setup(cx),
                |flows, cx| flows.setup_export.read(cx).is_busy(),
            ),
            (
                "start_restore",
                |flows, cx| flows.start_restore(cx),
                |flows, cx| flows.restore.read(cx).is_busy(),
            ),
        ]
    }

    fn entries(dir: &Path) -> usize {
        std::fs::read_dir(dir).map_or(0, |listing| listing.count())
    }

    #[gpui::test]
    fn each_bundle_kind_opens_its_own_import_flow(cx: &mut TestAppContext) {
        let mut routed = Vec::new();

        for kind in [BundleKind::Full, BundleKind::Pack, BundleKind::Setup] {
            let rig = stalled_rig(cx);
            rig.flows.update(cx, |flows, cx| {
                flows.route_import(PathBuf::from("picked.forgebundle"), Ok(kind), cx)
            });
            routed.push(busy_flows(cx, &rig));
        }

        assert_eq!(
            routed,
            vec![
                (true, false, false),
                (false, true, false),
                (false, false, true)
            ]
        );
    }

    #[gpui::test]
    fn a_json_file_is_refused_with_an_error_and_opens_no_flow(cx: &mut TestAppContext) {
        let rig = stalled_rig(cx);
        let dir = tempfile::tempdir().unwrap();
        let json = dir.path().join("action.json");
        std::fs::write(&json, br#"{"name":"Greeter"}"#).unwrap();
        let kind = rig.rt.block_on(read_bundle_kind(json.clone()));

        rig.flows
            .update(cx, |flows, cx| flows.route_import(json, kind, cx));

        let kinds: Vec<ToastKind> = toasts(cx).into_iter().map(|toast| toast.0).collect();
        assert_eq!(
            (busy_flows(cx, &rig), kinds),
            ((false, false, false), vec![ToastKind::Error])
        );
    }

    #[gpui::test]
    fn every_transfer_start_is_refused_with_a_warning_while_another_transfer_runs(
        cx: &mut TestAppContext,
    ) {
        let mut outcomes = Vec::new();
        let mut expected = Vec::new();

        for (name, start, started) in starts() {
            let rig = stalled_rig(cx);
            rig.flows.update(cx, |flows, cx| {
                flows.route_import(
                    PathBuf::from("picked.forgebundle"),
                    Ok(BundleKind::Pack),
                    cx,
                )
            });

            rig.flows.update(cx, start);

            let opened = rig.flows.read_with(cx, started);
            outcomes.push((name, opened, toasts(cx)));
            expected.push((
                name,
                false,
                vec![(ToastKind::Warn, tr!("transfer_busy_already"))],
            ));
        }

        assert_eq!(outcomes, expected);
    }

    #[gpui::test]
    fn an_idle_transfer_start_opens_its_flow_without_a_warning(cx: &mut TestAppContext) {
        let mut outcomes = Vec::new();

        for (name, start, started) in starts()
            .into_iter()
            .filter(|(name, _, _)| *name != "start_restore")
        {
            let rig = stalled_rig(cx);

            rig.flows.update(cx, start);

            let opened = rig.flows.read_with(cx, started);
            outcomes.push((name, opened, toasts(cx).len()));
        }

        assert_eq!(
            outcomes,
            vec![
                ("backup", true, 0),
                ("export_pack", true, 0),
                ("export_pack_of", true, 0),
                ("export_setup", true, 0),
            ]
        );
    }

    #[gpui::test]
    fn a_running_import_of_any_kind_marks_the_transfers_busy(cx: &mut TestAppContext) {
        let mut busy = Vec::new();

        for kind in [BundleKind::Full, BundleKind::Pack, BundleKind::Setup] {
            let rig = stalled_rig(cx);
            let idle = rig.flows.read_with(cx, |flows, cx| flows.is_busy(cx));
            rig.flows.update(cx, |flows, cx| {
                flows.route_import(PathBuf::from("picked.forgebundle"), Ok(kind), cx)
            });
            let running = rig.flows.read_with(cx, |flows, cx| flows.is_busy(cx));
            busy.push((idle, running));
        }

        assert_eq!(busy, vec![(false, true); 3]);
    }

    #[gpui::test]
    fn an_import_result_lands_with_its_toast_and_cleanup_after_home_is_dropped(
        cx: &mut TestAppContext,
    ) {
        let rig = rig_with(cx, true);
        cx.update(|cx| cx.set_global(TransferRoute(rig.flows.clone())));
        let stats = cx.new(|_| HomeStats::new());
        let event_loss = cx.new(|_| EventLoss::new());
        let services = rig.services.clone();
        let home = cx.new(|cx| HomeView::new(stats, event_loss, services, cx));
        let queue = queue_named("Alerts");
        let action = seed_queued_action(&rig.rt, &rig.world, "Greeter", &queue);
        let pack_bundle = export_pack(&rig.rt, &rig.world, &[action]);
        let route = cx.update(|cx| TransferFlows::route(cx).expect("the route is installed"));

        route.update(cx, |flows, cx| {
            flows.route_import(pack_bundle, Ok(BundleKind::Setup), cx)
        });
        cx.update(move |_| drop(home));
        let deadline = std::time::Instant::now() + SETTLE_DEADLINE;
        while toasts(cx).is_empty() && std::time::Instant::now() < deadline {
            rig.rt
                .block_on(async { tokio::time::sleep(Duration::from_millis(1)).await });
            cx.run_until_parked();
        }

        let staging = setup_staging_root(&rig.world.data_dir());
        let shown = toasts(cx);
        assert!(
            matches!(
                shown.as_slice(),
                [(ToastKind::Error, message)]
                    if message.contains("This file is a forge pack, not a setup.")
            ) && entries(&staging) == 0
                && !rig.flows.read_with(cx, |flows, cx| flows.is_busy(cx)),
            "toasts {shown:?}, {} staged setups left",
            entries(&staging)
        );
    }
}
