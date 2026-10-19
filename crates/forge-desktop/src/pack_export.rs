use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{PushToast, ToastAction, ToastKind, tr};
use forge_transfer::{BUNDLE_EXTENSION, PackExportRequest, write_pack};
use forge_types::ActionId;
use gpui::{Context, Entity, Subscription, Window, div, prelude::*};

use crate::async_bridge::{self, DIALOG_CANCELLED, DialogFilter};
use crate::pack_export_modal::{PackDraft, PackExportModal, PackExportModalEvent};
use crate::pack_services::PackServices;

const PACK_TOAST_DURATION: Duration = Duration::from_secs(10);
const FALLBACK_FILE_STEM: &str = "pack";

struct EditingStep {
    modal: Entity<PackExportModal>,
    _sub: Subscription,
}

enum PackExportStep {
    Idle,
    Editing(EditingStep),
    ChoosingFile,
    Writing,
}

pub struct PackExportFlow {
    services: PackServices,
    step: PackExportStep,
}

pub(crate) fn pack_file_name(pack_name: &str) -> String {
    let mut stem = String::new();
    let mut pending_dash = false;
    for ch in pack_name.chars() {
        if ch.is_alphanumeric() {
            if pending_dash && !stem.is_empty() {
                stem.push('-');
            }
            stem.push(ch);
            pending_dash = false;
        } else {
            pending_dash = true;
        }
    }
    if stem.is_empty() {
        stem.push_str(FALLBACK_FILE_STEM);
    }
    format!("{stem}.{BUNDLE_EXTENSION}")
}

impl PackExportFlow {
    pub fn new(services: PackServices) -> Self {
        Self {
            services,
            step: PackExportStep::Idle,
        }
    }

    pub fn is_busy(&self) -> bool {
        !matches!(self.step, PackExportStep::Idle)
    }

    pub fn start(&mut self, preselected: Vec<ActionId>, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let services = self.services.clone();
        let modal = cx.new(|cx| PackExportModal::new(services, preselected, cx));
        let sub = cx.subscribe(&modal, |this, modal, event: &PackExportModalEvent, cx| {
            match event {
                PackExportModalEvent::Confirmed => {
                    let draft = modal.update(cx, |modal, cx| modal.take_draft(cx));
                    if let Some(draft) = draft {
                        this.choose_file(draft, cx);
                    }
                }
                PackExportModalEvent::Cancelled => this.step = PackExportStep::Idle,
            }
            cx.notify();
        });
        self.step = PackExportStep::Editing(EditingStep { modal, _sub: sub });
        cx.notify();
    }

    fn choose_file(&mut self, draft: PackDraft, cx: &mut Context<Self>) {
        self.step = PackExportStep::ChoosingFile;
        let filter = DialogFilter {
            name: tr!("pack_export_dialog_filter"),
            extensions: vec![BUNDLE_EXTENSION],
        };
        let default_name = pack_file_name(&draft.metadata.name);
        async_bridge::spawn_dialog(
            &self.services.rt_handle,
            async_bridge::save_file(Some(filter), Some(default_name)),
            move |this, result, cx| this.file_chosen(result, draft, cx),
            cx,
        );
        cx.notify();
    }

    fn file_chosen(
        &mut self,
        result: Result<PathBuf, String>,
        draft: PackDraft,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(destination) => self.write(destination, draft, cx),
            Err(message) => {
                self.step = PackExportStep::Idle;
                if message != DIALOG_CANCELLED {
                    report(Err(message), cx);
                }
            }
        }
        cx.notify();
    }

    fn write(&mut self, destination: PathBuf, draft: PackDraft, cx: &mut Context<Self>) {
        self.step = PackExportStep::Writing;
        let provider = Arc::clone(&self.services.backend);
        let overlays = self.services.overlays.clone();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move {
                let overlay_root = overlays.root().await;
                write_pack(PackExportRequest {
                    provider,
                    inventory: draft.inventory,
                    selection: draft.selection,
                    metadata: draft.metadata,
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
        self.step = PackExportStep::Idle;
        report(result, cx);
        cx.notify();
    }
}

fn report(result: Result<PathBuf, String>, cx: &mut Context<PackExportFlow>) {
    match result {
        Ok(path) => {
            tracing::info!(path = %path.display(), "pack exported");
            let shown = path.display().to_string();
            cx.push_toast_full(
                ToastKind::Success,
                tr!("pack_export_done", path = shown.as_str()),
                None,
                Some(ToastAction::new(
                    tr!("settings_storage_backup_reveal"),
                    move |_window, app| app.reveal_path(&path),
                )),
                PACK_TOAST_DURATION,
            );
        }
        Err(message) => {
            tracing::warn!(error = %message, "pack export failed");
            cx.push_toast(
                ToastKind::Error,
                tr!("pack_export_failed", error = message.as_str()),
            );
        }
    }
}

impl Render for PackExportFlow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        match &self.step {
            PackExportStep::Editing(step) => div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(step.modal.clone())
                .into_any_element(),
            PackExportStep::Idle | PackExportStep::ChoosingFile | PackExportStep::Writing => {
                div().into_any_element()
            }
        }
    }
}
