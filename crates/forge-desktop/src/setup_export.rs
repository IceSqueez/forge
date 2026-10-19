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

use crate::async_bridge::{self, DIALOG_CANCELLED, DialogFilter};
use crate::pack_services::PackServices;
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
        async_bridge::spawn_dialog(
            &self.services.rt_handle,
            async_bridge::save_file(Some(filter), Some(setup_file_name())),
            |this, result, cx| this.file_chosen(result, cx),
            cx,
        );
        cx.notify();
    }

    fn file_chosen(&mut self, result: Result<PathBuf, String>, cx: &mut Context<Self>) {
        match result {
            Ok(destination) => self.write(destination, cx),
            Err(message) => {
                self.step = SetupExportStep::Idle;
                if message != DIALOG_CANCELLED {
                    report(Err(message), cx);
                }
            }
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
                .map_err(|error| error.to_string())
            },
            |this, result, cx| this.written(result, cx),
            cx,
        );
    }

    fn written(&mut self, result: Result<PathBuf, String>, cx: &mut Context<Self>) {
        self.step = SetupExportStep::Idle;
        report(result, cx);
        cx.notify();
    }
}

fn report(result: Result<PathBuf, String>, cx: &mut Context<SetupExportFlow>) {
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
                tr!("setup_export_failed", error = message.as_str()),
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
