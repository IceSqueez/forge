use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{PushToast, ToastAction, ToastKind, tr};
use forge_storage::DataProvider;
use forge_transfer::{
    BUNDLE_EXTENSION, BackupDestination, FullBackupRequest, Passphrase, backup_file_stem,
    unique_suffix, write_full_backup,
};
use gpui::{Context, Entity, Subscription, Window, div, prelude::*};
use time::OffsetDateTime;
use tokio::runtime::Handle;

use crate::async_bridge::{self, DIALOG_CANCELLED, DialogFilter};
use crate::backup_passphrase_modal::{BackupPassphraseEvent, BackupPassphraseModal};

const BACKUP_TOAST_DURATION: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackupTarget {
    BackupsDir,
    ChosenFile,
}

struct PassphrasePrompt {
    modal: Entity<BackupPassphraseModal>,
    _sub: Subscription,
}

enum BackupStep {
    Idle,
    Passphrase(PassphrasePrompt),
    ChoosingFile,
    Writing,
}

pub struct BackupFlow {
    backend: Arc<dyn DataProvider>,
    rt_handle: Handle,
    data_dir: PathBuf,
    target: BackupTarget,
    step: BackupStep,
}

pub(crate) fn default_bundle_name(now: OffsetDateTime, suffix: &str) -> String {
    format!("{}.{BUNDLE_EXTENSION}", backup_file_stem(now, suffix))
}

impl BackupFlow {
    pub fn new(backend: Arc<dyn DataProvider>, rt_handle: Handle, data_dir: PathBuf) -> Self {
        Self {
            backend,
            rt_handle,
            data_dir,
            target: BackupTarget::ChosenFile,
            step: BackupStep::Idle,
        }
    }

    pub fn is_busy(&self) -> bool {
        !matches!(self.step, BackupStep::Idle)
    }

    pub fn is_writing(&self) -> bool {
        matches!(self.step, BackupStep::ChoosingFile | BackupStep::Writing)
    }

    pub fn start(&mut self, target: BackupTarget, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        self.target = target;
        let modal = cx.new(BackupPassphraseModal::new);
        let sub = cx.subscribe(&modal, |this, modal, event: &BackupPassphraseEvent, cx| {
            match event {
                BackupPassphraseEvent::Confirmed => {
                    let passphrase = modal.update(cx, |modal, cx| modal.take_passphrase(cx));
                    this.step = BackupStep::Idle;
                    if let Some(passphrase) = passphrase {
                        this.passphrase_entered(passphrase, cx);
                    }
                }
                BackupPassphraseEvent::Cancelled => {
                    this.step = BackupStep::Idle;
                }
            }
            cx.notify();
        });
        self.step = BackupStep::Passphrase(PassphrasePrompt { modal, _sub: sub });
        cx.notify();
    }

    fn passphrase_entered(&mut self, passphrase: Passphrase, cx: &mut Context<Self>) {
        match self.target {
            BackupTarget::BackupsDir => self.write(BackupDestination::BackupsDir, passphrase, cx),
            BackupTarget::ChosenFile => self.choose_file(passphrase, cx),
        }
    }

    fn choose_file(&mut self, passphrase: Passphrase, cx: &mut Context<Self>) {
        self.step = BackupStep::ChoosingFile;
        let filter = DialogFilter {
            name: tr!("restore_dialog_filter"),
            extensions: vec![BUNDLE_EXTENSION],
        };
        let default_name = default_bundle_name(OffsetDateTime::now_utc(), &unique_suffix());
        async_bridge::spawn_dialog(
            &self.rt_handle,
            async_bridge::save_file(Some(filter), Some(default_name)),
            move |this, result, cx| this.file_chosen(result, passphrase, cx),
            cx,
        );
        cx.notify();
    }

    fn file_chosen(
        &mut self,
        result: Result<PathBuf, String>,
        passphrase: Passphrase,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(path) => self.write(BackupDestination::File(path), passphrase, cx),
            Err(message) => {
                self.step = BackupStep::Idle;
                if message != DIALOG_CANCELLED {
                    self.report(Err(message), cx);
                }
            }
        }
        cx.notify();
    }

    fn write(
        &mut self,
        destination: BackupDestination,
        passphrase: Passphrase,
        cx: &mut Context<Self>,
    ) {
        self.step = BackupStep::Writing;
        let backup = write_full_backup(FullBackupRequest {
            provider: Arc::clone(&self.backend),
            data_dir: self.data_dir.clone(),
            destination,
            passphrase,
        });
        async_bridge::run_async(
            &self.rt_handle,
            async move { backup.await.map_err(|e| e.to_string()) },
            |this, result, cx| this.written(result, cx),
            cx,
        );
        cx.notify();
    }

    fn written(&mut self, result: Result<PathBuf, String>, cx: &mut Context<Self>) {
        self.step = BackupStep::Idle;
        self.report(result, cx);
        cx.notify();
    }

    fn report(&self, result: Result<PathBuf, String>, cx: &mut Context<Self>) {
        match result {
            Ok(path) => {
                tracing::info!(path = %path.display(), "data backup created");
                let shown = path.display().to_string();
                cx.push_toast_full(
                    ToastKind::Success,
                    tr!("settings_storage_backup_done", path = shown.as_str()),
                    None,
                    Some(ToastAction::new(
                        tr!("settings_storage_backup_reveal"),
                        move |_window, app| app.reveal_path(&path),
                    )),
                    BACKUP_TOAST_DURATION,
                );
            }
            Err(message) => {
                tracing::warn!(error = %message, "data backup failed");
                cx.push_toast(
                    ToastKind::Error,
                    tr!("settings_storage_backup_failed", error = message.as_str()),
                );
            }
        }
    }
}

impl Render for BackupFlow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        match &self.step {
            BackupStep::Passphrase(prompt) => div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(prompt.modal.clone())
                .into_any_element(),
            BackupStep::Idle | BackupStep::ChoosingFile | BackupStep::Writing => {
                div().into_any_element()
            }
        }
    }
}
