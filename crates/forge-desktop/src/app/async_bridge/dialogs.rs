use std::path::PathBuf;

use gpui::{Context, SharedString};
use tokio::runtime::Handle;

use super::errors::{ErrorSink, report_failure};
use crate::app::error::DesktopError;

pub(crate) async fn open_path(
    target: impl AsRef<std::ffi::OsStr> + Send + 'static,
) -> Result<(), DesktopError> {
    tokio::task::spawn_blocking(move || open::that(target))
        .await
        .map_err(|_| DesktopError::TaskStopped)?
        .map_err(DesktopError::from)
}

pub(crate) fn open_external<V>(
    handle: &Handle,
    target: impl Into<String>,
    sink: ErrorSink,
    context: impl Into<SharedString>,
    cx: &mut Context<V>,
) where
    V: 'static,
{
    let target = target.into();
    report_failure(handle, open_path(target), sink, context, cx);
}

pub(crate) struct DialogFilter {
    pub name: String,
    pub extensions: Vec<&'static str>,
}

fn file_dialog(filter: Option<DialogFilter>) -> rfd::AsyncFileDialog {
    let dialog = rfd::AsyncFileDialog::new();
    match filter {
        Some(filter) => dialog.add_filter(filter.name, &filter.extensions),
        None => dialog,
    }
}

async fn pick_with(dialog: rfd::AsyncFileDialog) -> Option<PathBuf> {
    dialog
        .pick_file()
        .await
        .map(|handle| handle.path().to_path_buf())
}

pub(crate) async fn pick_file(filter: Option<DialogFilter>) -> Option<PathBuf> {
    pick_with(file_dialog(filter)).await
}

pub(crate) async fn pick_file_in(
    filter: Option<DialogFilter>,
    directory: PathBuf,
) -> Option<PathBuf> {
    pick_with(file_dialog(filter).set_directory(directory)).await
}

pub(crate) async fn pick_folder() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .pick_folder()
        .await
        .map(|handle| handle.path().to_path_buf())
}

pub(crate) async fn save_file(
    filter: Option<DialogFilter>,
    default_name: Option<String>,
) -> Option<PathBuf> {
    let mut dialog = file_dialog(filter);
    if let Some(name) = default_name {
        dialog = dialog.set_file_name(name);
    }
    dialog
        .save_file()
        .await
        .map(|handle| handle.path().to_path_buf())
}
