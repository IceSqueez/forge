use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    ActivePresentation, OverlayPosition, PushToast, ToastAction, ToastKind, overlay, tr,
};
use forge_transfer::{
    ClosureRegistries, SetupImportOutcome, SetupImportRequest, StagedSetup, TransferError,
    apply_setup_import, discard_staged_setup, stage_setup,
};
use gpui::{App, ClickEvent, Context, EventEmitter, Window, div, prelude::*};

use crate::async_bridge;
use crate::import_busy::{busy_overlay, refuse_while_busy};
use crate::pack_import_live::{LiveRefresh, SetupLiveHandles, refresh_setup_live, voice_baseline};
use crate::pack_services::PackServices;
use crate::restore_close::RestoreClose;
use crate::restore_problem::RestoreProblem;
use crate::setup_confirm::{SetupConfirmCard, SetupCount, setup_counts};

const SETUP_TOAST_DURATION: Duration = Duration::from_secs(10);
const STICKY: Duration = Duration::ZERO;
const DISCARD_CONTEXT: &str = "staged setup discard";

pub(crate) enum SetupApplied {
    Done {
        outcome: SetupImportOutcome,
        live: LiveRefresh,
    },
    Failed {
        staged: Box<StagedSetup>,
        message: String,
    },
    RollbackIncomplete {
        message: String,
        snapshot: PathBuf,
    },
}

struct Confirming {
    counts: Vec<SetupCount>,
    staged: Option<StagedSetup>,
    failure: Option<String>,
}

impl Confirming {
    fn is_applying(&self) -> bool {
        self.staged.is_none()
    }
}

enum SetupImportStep {
    Idle,
    Staging,
    Confirming(Box<Confirming>),
}

pub struct SetupImported;

pub struct SetupImportFlow {
    services: PackServices,
    live: SetupLiveHandles,
    data_dir: PathBuf,
    step: SetupImportStep,
}

impl EventEmitter<SetupImported> for SetupImportFlow {}

pub(crate) fn needs_restart(outcome: &SetupImportOutcome, live: LiveRefresh) -> bool {
    outcome.restart_recommended || live == LiveRefresh::NeedsRestart
}

impl SetupImportFlow {
    pub fn new(services: PackServices, data_dir: PathBuf) -> Self {
        Self {
            services,
            live: SetupLiveHandles::default(),
            data_dir,
            step: SetupImportStep::Idle,
        }
    }

    pub fn set_live(&mut self, live: SetupLiveHandles) {
        self.live = live;
    }

    pub fn is_busy(&self) -> bool {
        !matches!(self.step, SetupImportStep::Idle)
    }

    pub fn open(&mut self, bundle: PathBuf, cx: &mut Context<Self>) {
        if self.is_busy() {
            refuse_while_busy(cx);
            return;
        }
        self.step = SetupImportStep::Staging;
        let data_dir = self.data_dir.clone();
        let services = self.services.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move {
                let mut staged = stage_setup(bundle, data_dir)
                    .await
                    .map_err(|error| RestoreProblem::of(&error))?;
                staged.upgrade_retired_ids(ClosureRegistries {
                    sub_actions: &services.sub_actions,
                    triggers: &services.triggers,
                    schema: None,
                });
                Ok::<_, RestoreProblem>(staged)
            },
            |this, result, cx| this.staged(result.map_err(|problem| problem.message()), cx),
            cx,
        );
        cx.notify();
    }

    fn staged(&mut self, result: Result<StagedSetup, String>, cx: &mut Context<Self>) {
        match result {
            Ok(staged) => {
                self.step = SetupImportStep::Confirming(Box::new(Confirming {
                    counts: setup_counts(staged.documents()),
                    staged: Some(staged),
                    failure: None,
                }));
            }
            Err(message) => {
                self.step = SetupImportStep::Idle;
                tracing::warn!(error = %message, "setup could not be staged");
                cx.push_toast(
                    ToastKind::Error,
                    tr!("home_import_failed", error = message.as_str()),
                );
            }
        }
        cx.notify();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        let SetupImportStep::Confirming(confirming) = &mut self.step else {
            return;
        };
        let Some(staged) = confirming.staged.take() else {
            return;
        };
        confirming.failure = None;
        let services = self.services.clone();
        let live = self.live.clone();
        let data_dir = self.data_dir.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move { apply_staged(services, live, data_dir, staged).await },
            |this, result, cx| this.applied(result, cx),
            cx,
        );
        cx.notify();
    }

    fn applied(&mut self, result: SetupApplied, cx: &mut Context<Self>) {
        match result {
            SetupApplied::Done { outcome, live } => {
                self.step = SetupImportStep::Idle;
                self.report_success(&outcome, live, cx);
                cx.emit(SetupImported);
            }
            SetupApplied::Failed { staged, message } => {
                tracing::warn!(error = %message, "setup import failed and was undone");
                match &mut self.step {
                    SetupImportStep::Confirming(confirming) => {
                        confirming.staged = Some(*staged);
                        confirming.failure =
                            Some(tr!("setup_import_failed", error = message.as_str()));
                    }
                    SetupImportStep::Idle | SetupImportStep::Staging => self.discard(*staged),
                }
            }
            SetupApplied::RollbackIncomplete { message, snapshot } => {
                self.step = SetupImportStep::Idle;
                report_rollback_incomplete(&message, snapshot, cx);
            }
        }
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if let SetupImportStep::Confirming(confirming) = &self.step
            && confirming.is_applying()
        {
            return;
        }
        let step = std::mem::replace(&mut self.step, SetupImportStep::Idle);
        if let SetupImportStep::Confirming(mut confirming) = step
            && let Some(staged) = confirming.staged.take()
        {
            self.discard(staged);
        }
        cx.notify();
    }

    fn discard(&self, staged: StagedSetup) {
        async_bridge::detached(
            &self.services.rt_handle,
            DISCARD_CONTEXT,
            discard_staged_setup(staged),
        );
    }

    fn report_success(
        &mut self,
        outcome: &SetupImportOutcome,
        live: LiveRefresh,
        cx: &mut Context<Self>,
    ) {
        let restart = needs_restart(outcome, live);
        tracing::info!(
            ?live,
            restart_recommended = outcome.restart_recommended,
            cleanup_failures = outcome.cleanup_failures.len(),
            "setup imported"
        );
        if restart {
            let action = ToastAction::new(tr!("setup_import_restart"), |_window, app| {
                request_restart(app)
            });
            cx.push_toast_full(
                ToastKind::Success,
                tr!("setup_import_done_restart"),
                None,
                Some(action),
                STICKY,
            );
        } else {
            let snapshot = outcome.safety_snapshot.clone();
            let action =
                ToastAction::new(tr!("pack_import_reveal_snapshot"), move |_window, app| {
                    app.reveal_path(&snapshot)
                });
            cx.push_toast_full(
                ToastKind::Success,
                tr!("setup_import_done"),
                None,
                Some(action),
                SETUP_TOAST_DURATION,
            );
        }
        if !outcome.cleanup_failures.is_empty() {
            for failure in &outcome.cleanup_failures {
                tracing::warn!(%failure, "setup import left a leftover behind");
            }
            let count = outcome.cleanup_failures.len().to_string();
            cx.push_toast(
                ToastKind::Warn,
                tr!("setup_import_cleanup_failed", count = count.as_str()),
            );
        }
    }
}

fn request_restart(app: &mut App) {
    match RestoreClose::route(app) {
        Some(close) => close.update(app, |close, cx| close.restart(cx)),
        None => tracing::warn!("restart route is missing; restart forge by hand"),
    }
}

fn report_rollback_incomplete(message: &str, snapshot: PathBuf, cx: &mut Context<SetupImportFlow>) {
    tracing::error!(error = %message, "setup import could not be fully undone");
    let shown = snapshot.display().to_string();
    let text = tr!(
        "setup_import_rollback_incomplete",
        error = message,
        snapshot = shown.as_str()
    );
    let action = ToastAction::new(tr!("pack_import_reveal_snapshot"), move |_window, app| {
        app.reveal_path(&snapshot)
    });
    cx.push_toast_full(ToastKind::Error, text, None, Some(action), STICKY);
}

impl Drop for SetupImportFlow {
    fn drop(&mut self) {
        if let SetupImportStep::Confirming(confirming) = &mut self.step
            && let Some(staged) = confirming.staged.take()
        {
            self.discard(staged);
        }
    }
}

async fn apply_staged(
    services: PackServices,
    live: SetupLiveHandles,
    data_dir: PathBuf,
    staged: StagedSetup,
) -> SetupApplied {
    let source = match services.load_source().await {
        Ok(source) => source,
        Err(message) => {
            return SetupApplied::Failed {
                staged: Box::new(staged),
                message,
            };
        }
    };
    let baseline = voice_baseline(services.backend.as_ref()).await;
    let overlay_root = services.overlays.root().await;
    let applied = apply_setup_import(SetupImportRequest {
        provider: Arc::clone(&services.backend),
        registries: services.registries(&source),
        staged: &staged,
        data_dir,
        overlay_root,
    })
    .await;
    match applied {
        Ok(outcome) => {
            discard_quietly(staged).await;
            let live = refresh_setup_live(&services, &live, &outcome, baseline.as_ref()).await;
            SetupApplied::Done { outcome, live }
        }
        Err(TransferError::SetupRollbackIncomplete {
            cause,
            failure,
            snapshot,
        }) => {
            discard_quietly(staged).await;
            SetupApplied::RollbackIncomplete {
                message: format!("{cause}; {failure}"),
                snapshot,
            }
        }
        Err(error) => SetupApplied::Failed {
            staged: Box::new(staged),
            message: error.to_string(),
        },
    }
}

async fn discard_quietly(staged: StagedSetup) {
    if let Err(error) = discard_staged_setup(staged).await {
        tracing::warn!(%error, "staged setup could not be discarded");
    }
}

impl Render for SetupImportFlow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let palette = cx.palette();
        let confirming = match &self.step {
            SetupImportStep::Confirming(confirming) => confirming,
            SetupImportStep::Staging => {
                return busy_overlay(
                    "setup-import-busy",
                    tr!("import_busy_setup_title"),
                    tr!("import_busy_setup_detail"),
                    &palette,
                    density,
                );
            }
            SetupImportStep::Idle => return div().into_any_element(),
        };
        let density = cx.density();
        let card = SetupConfirmCard {
            counts: &confirming.counts,
            applying: confirming.is_applying(),
            failure: confirming.failure.as_deref(),
        }
        .render(
            &palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm(cx)),
        );
        let view = cx.entity();
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(
                overlay(card, &palette)
                    .position(OverlayPosition::Center)
                    .on_dismiss("setup-confirm-scrim", move |_window, cx| {
                        view.update(cx, |this, cx| this.cancel(cx));
                    }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::BTreeSet;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::Duration;

    use forge_components::{ToastKind, Toasts};
    use forge_speech_types::{AliasId, VoiceAlias};
    use forge_speech_types::{AliasState, EngineId, VoiceId};
    use forge_storage::Language;
    use forge_transfer::{
        PackItemKind, SetupExportRequest, SetupImportOutcome, SetupQueueChanges, StagedSetup,
        write_setup,
    };
    use gpui::{AppContext, Entity, TestAppContext};
    use ulid::Ulid;

    use super::{
        SETUP_TOAST_DURATION, SetupApplied, SetupImportFlow, SetupImportStep, SetupImported,
    };
    use crate::i18n::install_language;
    use crate::pack_import_live::LiveRefresh;
    use crate::restore_problem::RestoreProblem;
    use crate::setup_confirm::{SetupCount, SetupPart};
    use crate::test_support::pack_flow::{PackWorld, pack_world, queue_named, seed_queued_action};
    use crate::test_support::{
        alive_tasks, install_presentation, runtime, wait_until_worker_drains_to, worker_runtime,
    };
    use forge_components::tr;

    const SETTLE_DEADLINE: Duration = Duration::from_secs(30);
    const PAST_ANY_TOAST_TIMER: Duration = Duration::from_secs(120);
    const SNAPSHOT_NAME: &str = "forge-pre-setup-snapshot.forgebundle";

    struct Rig {
        rt: tokio::runtime::Runtime,
        world: PackWorld,
        flow: Entity<SetupImportFlow>,
    }

    fn rig(cx: &mut TestAppContext) -> Rig {
        rig_on(cx, runtime())
    }

    fn rig_on(cx: &mut TestAppContext, rt: tokio::runtime::Runtime) -> Rig {
        install_language(Language::En);
        install_presentation(cx);
        cx.update(|cx| cx.set_global(Toasts::new()));
        let world = pack_world(&rt);
        let services = world.services.clone();
        let data_dir = world.data_dir();
        let flow = cx.new(|_| SetupImportFlow::new(services, data_dir));
        Rig { rt, world, flow }
    }

    fn exported_setup(rig: &Rig) -> PathBuf {
        let queue = queue_named("Alerts");
        seed_queued_action(&rig.rt, &rig.world, "Greeter", &queue);
        seed_queued_action(&rig.rt, &rig.world, "Farewell", &queue);
        seed_queued_action(&rig.rt, &rig.world, "Shoutout", &queue);
        rig.rt.block_on(async {
            rig.world
                .provider()
                .voice_alias_repo()
                .upsert(&VoiceAlias {
                    id: AliasId::new(),
                    viewer_id: "twitch:viewer".to_owned(),
                    viewer_name: "viewer".to_owned(),
                    engine_id: EngineId("piper".to_owned()),
                    voice_id: VoiceId("amy".to_owned()),
                    pitch_semitones: None,
                    rate_multiplier: None,
                    state: AliasState::Active,
                })
                .await
                .unwrap();
            write_setup(SetupExportRequest {
                provider: Arc::clone(rig.world.provider()),
                overlay_root: rig.world.overlay_root(),
                destination: rig
                    .world
                    .data_dir()
                    .join(format!("{}.forgebundle", Ulid::generate())),
            })
            .await
            .unwrap()
        })
    }

    fn settle_until(
        cx: &mut TestAppContext,
        rig: &Rig,
        done: impl Fn(&mut TestAppContext) -> bool,
    ) {
        let deadline = std::time::Instant::now() + SETTLE_DEADLINE;
        while std::time::Instant::now() < deadline {
            rig.rt
                .block_on(async { tokio::time::sleep(Duration::from_millis(1)).await });
            cx.run_until_parked();
            if done(cx) {
                return;
            }
        }
        panic!("the setup import never settled");
    }

    fn step(cx: &mut TestAppContext, rig: &Rig) -> &'static str {
        rig.flow.read_with(cx, |flow, _| match &flow.step {
            SetupImportStep::Idle => "idle",
            SetupImportStep::Staging => "staging",
            SetupImportStep::Confirming(confirming) if confirming.is_applying() => "applying",
            SetupImportStep::Confirming(_) => "confirming",
        })
    }

    fn staging_dir(cx: &mut TestAppContext, rig: &Rig) -> PathBuf {
        rig.flow.read_with(cx, |flow, _| {
            let SetupImportStep::Confirming(confirming) = &flow.step else {
                panic!("the flow is not confirming");
            };
            confirming
                .staged
                .as_ref()
                .expect("the confirmation holds the staged setup")
                .staging_dir()
                .to_path_buf()
        })
    }

    fn confirming(cx: &mut TestAppContext, rig: &Rig) -> PathBuf {
        let bundle = exported_setup(rig);
        rig.flow.update(cx, |flow, cx| flow.open(bundle, cx));
        settle_until(cx, rig, |cx| step(cx, rig) == "confirming");
        staging_dir(cx, rig)
    }

    fn take_staged_as_confirm_does(cx: &mut TestAppContext, rig: &Rig) -> StagedSetup {
        rig.flow.update(cx, |flow, _| {
            let SetupImportStep::Confirming(confirming) = &mut flow.step else {
                panic!("the flow is not confirming");
            };
            confirming
                .staged
                .take()
                .expect("the confirmation holds the staged setup")
        })
    }

    fn toasts(cx: &mut TestAppContext) -> Vec<(ToastKind, String, bool)> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| {
                    (
                        toast.kind,
                        toast.message.to_string(),
                        toast.action.is_some(),
                    )
                })
                .collect()
        })
    }

    fn toast_kinds(cx: &mut TestAppContext) -> Vec<ToastKind> {
        toasts(cx).into_iter().map(|toast| toast.0).collect()
    }

    fn outcome(restart_recommended: bool, cleanup_failures: Vec<String>) -> SetupImportOutcome {
        SetupImportOutcome {
            safety_snapshot: PathBuf::from(SNAPSHOT_NAME),
            refresh: BTreeSet::new(),
            queues: SetupQueueChanges::default(),
            restart_recommended,
            cleanup_failures,
        }
    }

    fn count_of(counts: &[SetupCount], part: SetupPart) -> usize {
        counts
            .iter()
            .find(|count| count.part == part)
            .map(|count| count.count)
            .expect("every setup part is counted")
    }

    #[gpui::test]
    fn opening_a_setup_stages_it_and_counts_what_it_replaces(cx: &mut TestAppContext) {
        let rig = rig(cx);
        confirming(cx, &rig);
        let stored_queues = rig
            .rt
            .block_on(rig.world.provider().queue_repo().list())
            .unwrap()
            .len();

        let counted = rig.flow.read_with(cx, |flow, _| {
            let SetupImportStep::Confirming(confirming) = &flow.step else {
                panic!("the flow is not confirming");
            };
            [
                SetupPart::Item(PackItemKind::Action),
                SetupPart::Item(PackItemKind::Queue),
                SetupPart::Item(PackItemKind::Trigger),
                SetupPart::VoiceAliases,
                SetupPart::ClipHotkeys,
            ]
            .map(|part| count_of(&confirming.counts, part))
        });

        assert_eq!(counted, [3, stored_queues, 0, 1, 0]);
    }

    #[gpui::test]
    fn a_file_that_cannot_be_staged_returns_to_idle_with_an_error_toast(cx: &mut TestAppContext) {
        let rig = rig(cx);
        let junk = rig.world.data_dir().join("junk.forgebundle");
        std::fs::write(&junk, b"not a bundle").unwrap();

        rig.flow.update(cx, |flow, cx| flow.open(junk, cx));
        settle_until(cx, &rig, |cx| step(cx, &rig) == "idle");

        assert_eq!(toast_kinds(cx), vec![ToastKind::Error]);
    }

    #[gpui::test]
    fn cancelling_the_confirmation_discards_the_staged_setup(cx: &mut TestAppContext) {
        let rig = rig(cx);
        let staging = confirming(cx, &rig);

        rig.flow.update(cx, |flow, cx| flow.cancel(cx));
        settle_until(cx, &rig, |_| !staging.exists());

        assert_eq!(step(cx, &rig), "idle");
    }

    #[gpui::test]
    fn cancelling_while_the_import_applies_keeps_the_confirmation_open(cx: &mut TestAppContext) {
        let rig = rig(cx);
        confirming(cx, &rig);
        rig.flow.update(cx, |flow, cx| flow.confirm(cx));

        rig.flow.update(cx, |flow, cx| flow.cancel(cx));

        assert_eq!(step(cx, &rig), "applying");
    }

    #[gpui::test]
    fn dropping_the_flow_mid_confirmation_discards_the_staged_setup(cx: &mut TestAppContext) {
        let rig = rig(cx);
        let staging = confirming(cx, &rig);
        let Rig { rt, world, flow } = rig;

        cx.update(move |_| drop(flow));
        let deadline = std::time::Instant::now() + SETTLE_DEADLINE;
        while staging.exists() && std::time::Instant::now() < deadline {
            rt.block_on(async { tokio::time::sleep(Duration::from_millis(1)).await });
        }
        drop(world);

        assert!(!staging.exists(), "{staging:?} survived the flow");
    }

    fn queue_exists(rig: &Rig, name: &str) -> bool {
        rig.rt
            .block_on(rig.world.provider().queue_repo().list())
            .unwrap()
            .iter()
            .any(|queue| queue.name == name)
    }

    #[gpui::test]
    fn confirming_replaces_the_configuration_emits_imported_and_discards_staging(
        cx: &mut TestAppContext,
    ) {
        let rig = rig(cx);
        let staging = confirming(cx, &rig);
        seed_queued_action(&rig.rt, &rig.world, "Added later", &queue_named("Later"));
        let imported = Rc::new(Cell::new(false));
        let seen = Rc::clone(&imported);
        cx.update(|cx| {
            cx.subscribe(&rig.flow, move |_, _: &SetupImported, _| seen.set(true))
                .detach();
        });

        rig.flow.update(cx, |flow, cx| flow.confirm(cx));
        settle_until(cx, &rig, |cx| step(cx, &rig) == "idle" && !staging.exists());

        assert_eq!(
            (queue_exists(&rig, "Later"), imported.get(), toast_kinds(cx)),
            (false, true, vec![ToastKind::Success])
        );
    }

    #[gpui::test]
    fn a_failed_import_shows_the_error_in_the_card_and_keeps_staging(cx: &mut TestAppContext) {
        let rig = rig(cx);
        let staging = confirming(cx, &rig);
        let staged = take_staged_as_confirm_does(cx, &rig);

        rig.flow.update(cx, |flow, cx| {
            flow.applied(
                SetupApplied::Failed {
                    staged: Box::new(staged),
                    message: "disk full".to_owned(),
                },
                cx,
            )
        });

        let failure = rig.flow.read_with(cx, |flow, _| match &flow.step {
            SetupImportStep::Confirming(confirming) => confirming.failure.clone(),
            SetupImportStep::Idle | SetupImportStep::Staging => None,
        });
        assert_eq!(
            (
                step(cx, &rig),
                failure.is_some_and(|shown| shown.contains("disk full")),
                staging.exists()
            ),
            ("confirming", true, true)
        );
    }

    #[gpui::test]
    fn the_success_toast_stays_with_a_restart_offer_only_when_a_restart_is_needed(
        cx: &mut TestAppContext,
    ) {
        let rig = rig(cx);
        let mut survived = Vec::new();

        for (restart_recommended, live) in [
            (false, LiveRefresh::Applied),
            (true, LiveRefresh::Applied),
            (false, LiveRefresh::NeedsRestart),
        ] {
            cx.update(|cx| cx.set_global(Toasts::new()));
            rig.flow.update(cx, |flow, cx| {
                flow.applied(
                    SetupApplied::Done {
                        outcome: outcome(restart_recommended, Vec::new()),
                        live,
                    },
                    cx,
                )
            });
            cx.executor()
                .advance_clock(SETUP_TOAST_DURATION + PAST_ANY_TOAST_TIMER);
            cx.run_until_parked();
            let left: Vec<(ToastKind, bool)> = toasts(cx)
                .into_iter()
                .map(|(kind, _, action)| (kind, action))
                .collect();
            survived.push(left);
        }

        assert_eq!(
            survived,
            vec![
                vec![],
                vec![(ToastKind::Success, true)],
                vec![(ToastKind::Success, true)],
            ]
        );
    }

    #[gpui::test]
    fn leftover_files_after_an_import_add_a_warning(cx: &mut TestAppContext) {
        let rig = rig(cx);

        rig.flow.update(cx, |flow, cx| {
            flow.applied(
                SetupApplied::Done {
                    outcome: outcome(false, vec!["staging/leftover".to_owned()]),
                    live: LiveRefresh::Applied,
                },
                cx,
            )
        });

        assert_eq!(toast_kinds(cx), vec![ToastKind::Success, ToastKind::Warn]);
    }

    #[gpui::test]
    fn an_incomplete_rollback_leaves_a_sticky_error_naming_the_snapshot(cx: &mut TestAppContext) {
        let rig = rig(cx);
        confirming(cx, &rig);
        let staged = take_staged_as_confirm_does(cx, &rig);
        rig.rt
            .block_on(super::discard_staged_setup(staged))
            .unwrap();
        let snapshot = rig.world.data_dir().join(SNAPSHOT_NAME);

        rig.flow.update(cx, |flow, cx| {
            flow.applied(
                SetupApplied::RollbackIncomplete {
                    message: "rollback failed".to_owned(),
                    snapshot: snapshot.clone(),
                },
                cx,
            )
        });
        cx.executor().advance_clock(PAST_ANY_TOAST_TIMER);
        cx.run_until_parked();

        let shown = snapshot.display().to_string();
        let toasts: Vec<(ToastKind, bool, bool)> = toasts(cx)
            .into_iter()
            .map(|(kind, message, action)| (kind, message.contains(&shown), action))
            .collect();
        assert_eq!(
            (step(cx, &rig), toasts),
            ("idle", vec![(ToastKind::Error, true, true)])
        );
    }

    #[gpui::test]
    fn opening_another_setup_while_confirming_warns_and_keeps_the_confirmation(
        cx: &mut TestAppContext,
    ) {
        let rig = rig(cx);
        let staging = confirming(cx, &rig);

        rig.flow.update(cx, |flow, cx| {
            flow.open(PathBuf::from("second.forgebundle"), cx)
        });

        assert_eq!(
            (toast_kinds(cx), step(cx, &rig), staging_dir(cx, &rig)),
            (vec![ToastKind::Warn], "confirming", staging)
        );
    }

    #[gpui::test]
    fn an_unreadable_setup_staged_on_a_worker_thread_toasts_the_refusal_in_the_ui_language(
        cx: &mut TestAppContext,
    ) {
        let rig = rig_on(cx, worker_runtime());
        install_language(Language::Uk);
        let junk = rig.world.data_dir().join("junk.forgebundle");
        std::fs::write(&junk, b"not a bundle").unwrap();
        let background_tasks = alive_tasks(&rig.rt);

        rig.flow.update(cx, |flow, cx| flow.open(junk, cx));
        wait_until_worker_drains_to(&rig.rt, background_tasks);
        cx.run_until_parked();

        let messages: Vec<String> = toasts(cx).into_iter().map(|toast| toast.1).collect();
        assert_eq!(
            (step(cx, &rig), messages),
            (
                "idle",
                vec![tr!(
                    "home_import_failed",
                    error = RestoreProblem::NotABundle.message()
                )]
            )
        );
    }
}
