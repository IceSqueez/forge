use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{
    ActivePresentation, Density, ForgePalette, Icon, ModalSize, OverlayPosition, PushToast,
    Spacing, ToastAction, ToastKind, ghost_button, modal, modal_width, overlay, primary_button,
    spacing, tr,
};
use forge_transfer::{BUNDLE_EXTENSION, SetupExportRequest, write_setup};
use gpui::{AnyElement, App, ClickEvent, Context, Window, div, prelude::*};

use crate::async_bridge::{self, DialogFilter};
use crate::pack_services::PackServices;
use crate::repo_failure::RepoFailure;
use crate::restore_confirm::statement;

const SETUP_TOAST_DURATION: Duration = Duration::from_secs(10);
const SETUP_FILE_STEM: &str = "forge-setup";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SetupExportStep {
    Idle,
    Explaining,
    ChoosingFile,
    Writing,
}

pub struct SetupExportFlow {
    services: PackServices,
    step: SetupExportStep,
}

pub(crate) fn setup_file_name() -> String {
    format!("{SETUP_FILE_STEM}.{BUNDLE_EXTENSION}")
}

impl SetupExportFlow {
    pub fn new(services: PackServices) -> Self {
        Self {
            services,
            step: SetupExportStep::Idle,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.step != SetupExportStep::Idle
    }

    pub fn start(&mut self, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        self.step = SetupExportStep::Explaining;
        cx.notify();
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if self.step == SetupExportStep::Explaining {
            self.step = SetupExportStep::Idle;
        }
        cx.notify();
    }

    fn choose_file(&mut self, cx: &mut Context<Self>) {
        if self.step != SetupExportStep::Explaining {
            return;
        }
        self.step = SetupExportStep::ChoosingFile;
        let filter = DialogFilter {
            name: tr!("setup_export_dialog_filter"),
            extensions: vec![BUNDLE_EXTENSION],
        };
        async_bridge::run_async(
            &self.services.rt_handle,
            async_bridge::save_file(Some(filter), Some(setup_file_name())),
            |this, result, cx| this.file_chosen(result, cx),
            cx,
        );
        cx.notify();
    }

    fn file_chosen(&mut self, chosen: Option<PathBuf>, cx: &mut Context<Self>) {
        match chosen {
            Some(destination) => self.write(destination, cx),
            None => self.step = SetupExportStep::Idle,
        }
        cx.notify();
    }

    fn write(&mut self, destination: PathBuf, cx: &mut Context<Self>) {
        self.step = SetupExportStep::Writing;
        let provider = Arc::clone(&self.services.backend);
        let overlays = self.services.overlays.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move {
                let overlay_root = overlays.root().await;
                write_setup(SetupExportRequest {
                    provider,
                    overlay_root,
                    destination,
                })
                .await
                .map_err(RepoFailure::from)
            },
            |this, result, cx| this.written(result, cx),
            cx,
        );
    }

    fn written(&mut self, result: Result<PathBuf, RepoFailure>, cx: &mut Context<Self>) {
        self.step = SetupExportStep::Idle;
        report(result, cx);
        cx.notify();
    }
}

fn report(result: Result<PathBuf, RepoFailure>, cx: &mut Context<SetupExportFlow>) {
    match result {
        Ok(path) => {
            tracing::info!(path = %path.display(), "setup exported");
            let shown = path.display().to_string();
            cx.push_toast_full(
                ToastKind::Success,
                tr!("setup_export_done", path = shown.as_str()),
                None,
                Some(ToastAction::new(
                    tr!("settings_storage_backup_reveal"),
                    move |_window, app| app.reveal_path(&path),
                )),
                SETUP_TOAST_DURATION,
            );
        }
        Err(message) => {
            tracing::warn!(error = %message, "setup export failed");
            cx.push_toast(
                ToastKind::Error,
                tr!("setup_export_failed", error = message.text()),
            );
        }
    }
}

fn explain_card(
    palette: &ForgePalette,
    density: Density,
    on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_continue: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let on_cancel = Rc::new(on_cancel);
    let cancel_from_footer = Rc::clone(&on_cancel);
    let body = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xs, density))
        .child(statement(
            Icon::Check,
            palette.success,
            tr!("setup_export_carries"),
            palette,
            density,
        ))
        .child(statement(
            Icon::Key,
            palette.info,
            tr!("setup_export_never_logins"),
            palette,
            density,
        ))
        .child(statement(
            Icon::History,
            palette.info,
            tr!("setup_export_never_history"),
            palette,
            density,
        ))
        .child(statement(
            Icon::AlertTriangle,
            palette.warning,
            tr!("setup_export_never_machine"),
            palette,
            density,
        ));
    let footer = div()
        .flex()
        .items_center()
        .justify_end()
        .gap(spacing(Spacing::Xs, density))
        .child(
            ghost_button(tr!("common_cancel"), palette)
                .on_click("setup-export-cancel", move |event, window, cx| {
                    cancel_from_footer(event, window, cx)
                }),
        )
        .child(
            primary_button(tr!("setup_export_continue"), palette)
                .on_click("setup-export-continue", on_continue),
        );
    modal(tr!("setup_export_title"), body, palette)
        .header_icon(Icon::Settings, palette.brand)
        .subtitle(tr!("setup_export_subtitle"))
        .width(modal_width(ModalSize::Sm))
        .footer(footer)
        .on_close("setup-export-close", move |event, window, cx| {
            on_cancel(event, window, cx)
        })
        .into_any_element()
}

impl Render for SetupExportFlow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.step != SetupExportStep::Explaining {
            return div().into_any_element();
        }
        let palette = cx.palette();
        let density = cx.density();
        let card = explain_card(
            &palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.dismiss(cx)),
            cx.listener(|this, _: &ClickEvent, _, cx| this.choose_file(cx)),
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
                    .on_dismiss("setup-export-scrim", move |_window, cx| {
                        view.update(cx, |this, cx| this.dismiss(cx));
                    }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use forge_components::{ToastKind, Toasts};
    use forge_storage::Language;
    use forge_transfer::{BundleKind, read_bundle_kind};
    use gpui::{AppContext, Entity, TestAppContext};

    use super::{SetupExportFlow, SetupExportStep, setup_file_name};
    use crate::i18n::install_language;
    use crate::test_support::pack_flow::{PackWorld, pack_world};
    use crate::test_support::{install_presentation, runtime};

    const SETTLE_DEADLINE: Duration = Duration::from_secs(30);

    struct Rig {
        rt: tokio::runtime::Runtime,
        world: PackWorld,
        flow: Entity<SetupExportFlow>,
    }

    fn rig(cx: &mut TestAppContext) -> Rig {
        install_language(Language::En);
        install_presentation(cx);
        cx.update(|cx| cx.set_global(Toasts::new()));
        let rt = runtime();
        let world = pack_world(&rt);
        let services = world.services.clone();
        let flow = cx.new(|_| SetupExportFlow::new(services));
        Rig { rt, world, flow }
    }

    fn step(cx: &mut TestAppContext, rig: &Rig) -> SetupExportStep {
        rig.flow.read_with(cx, |flow, _| flow.step)
    }

    fn toasts(cx: &mut TestAppContext) -> Vec<(ToastKind, bool)> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| (toast.kind, toast.action.is_some()))
                .collect()
        })
    }

    #[gpui::test]
    fn a_closed_save_dialog_returns_to_idle_without_a_toast(cx: &mut TestAppContext) {
        let rig = rig(cx);

        rig.flow.update(cx, |flow, cx| {
            flow.step = SetupExportStep::ChoosingFile;
            flow.file_chosen(None, cx);
        });

        assert_eq!(
            (step(cx, &rig), toasts(cx)),
            (SetupExportStep::Idle, vec![])
        );
    }

    #[gpui::test]
    fn a_chosen_destination_receives_a_setup_and_a_toast_offers_to_reveal_it(
        cx: &mut TestAppContext,
    ) {
        let rig = rig(cx);
        let destination = rig.world.data_dir().join(setup_file_name());

        rig.flow.update(cx, |flow, cx| {
            flow.step = SetupExportStep::ChoosingFile;
            flow.file_chosen(Some(destination.clone()), cx);
        });
        let deadline = std::time::Instant::now() + SETTLE_DEADLINE;
        while step(cx, &rig) != SetupExportStep::Idle && std::time::Instant::now() < deadline {
            rig.rt
                .block_on(async { tokio::time::sleep(Duration::from_millis(1)).await });
            cx.run_until_parked();
        }

        let kind = rig.rt.block_on(read_bundle_kind(destination)).unwrap();
        assert_eq!(
            (kind, toasts(cx)),
            (BundleKind::Setup, vec![(ToastKind::Success, true)])
        );
    }

    #[gpui::test]
    fn a_failed_write_reports_an_error_and_frees_the_flow(cx: &mut TestAppContext) {
        let rig = rig(cx);
        rig.flow.update(cx, |flow, cx| {
            flow.step = SetupExportStep::Writing;
            flow.written(
                Err(crate::repo_failure::RepoFailure::operation("disk full")),
                cx,
            );
        });

        assert_eq!(
            (step(cx, &rig), toasts(cx)),
            (SetupExportStep::Idle, vec![(ToastKind::Error, false)])
        );
    }

    #[gpui::test]
    fn while_writing_neither_a_new_start_nor_a_dismiss_interrupts_the_export(
        cx: &mut TestAppContext,
    ) {
        let rig = rig(cx);
        rig.flow
            .update(cx, |flow, _| flow.step = SetupExportStep::Writing);

        rig.flow.update(cx, |flow, cx| {
            flow.start(cx);
            flow.dismiss(cx);
        });

        assert_eq!(step(cx, &rig), SetupExportStep::Writing);
    }
}
