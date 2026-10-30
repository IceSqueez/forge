use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_components::{ToastAction, ToastKind, tr};
use forge_storage::SettingDisclosure::Verbatim;
use forge_storage::SettingPortability::MachineLocal;
use forge_storage::{
    DataProvider, Flag, Setting, SettingsRepo, read_setting_or_default, write_setting,
};
use forge_update::{InstallEnvironment, leftover_machine_install};
use gpui::{App, AsyncApp};
use tokio::runtime::Handle;

use crate::async_bridge::{detached, open_path};
use forge_components::PushToast;

pub const INSTALLED_APPS_SETTINGS: &str = "ms-settings:appsfeatures";

pub const LEFTOVER_INSTALL_NOTICED: Setting<Flag> = Setting::new(
    "updates.leftover_install_noticed",
    Flag { default: false },
    MachineLocal,
    Verbatim,
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftoverNotice {
    pub install_dir: PathBuf,
}

pub fn leftover_notice(leftover: Option<PathBuf>, noticed: bool) -> Option<LeftoverNotice> {
    let install_dir = leftover?;
    (!noticed).then_some(LeftoverNotice { install_dir })
}

async fn unnoticed_leftover(backend: Arc<dyn DataProvider>) -> Option<LeftoverNotice> {
    let leftover = tokio::task::spawn_blocking(|| {
        InstallEnvironment::current().and_then(|env| leftover_machine_install(&env))
    })
    .await
    .ok()
    .flatten()?;
    let repo = backend as Arc<dyn SettingsRepo>;
    let noticed = read_setting_or_default(repo.as_ref(), &LEFTOVER_INSTALL_NOTICED).await;
    let notice = leftover_notice(Some(leftover), noticed)?;
    if let Err(error) = write_setting(repo.as_ref(), &LEFTOVER_INSTALL_NOTICED, &true).await {
        tracing::warn!(error = %error, "leftover install notice could not be remembered");
    }
    Some(notice)
}

pub fn start_leftover_install_notice(
    cx: &mut AsyncApp,
    backend: Arc<dyn DataProvider>,
    rt_handle: Handle,
) {
    if !cfg!(target_os = "windows") {
        return;
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt_handle.spawn(async move {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the view may close before the result arrives"
        )]
        let _ = tx.send(unnoticed_leftover(backend).await);
    });
    cx.spawn(async move |cx| {
        let Ok(Some(notice)) = rx.await else {
            return;
        };
        tracing::info!(path = %notice.install_dir.display(), "leftover all-users install found");
        cx.update(|cx| raise_leftover_toast(rt_handle, cx));
    })
    .detach();
}

fn raise_leftover_toast(rt_handle: Handle, cx: &mut App) {
    cx.push_toast_full(
        ToastKind::Warn,
        tr!("update_leftover_install"),
        None,
        Some(ToastAction::new(
            tr!("update_leftover_open_action"),
            move |_window, _app: &mut App| {
                detached(
                    &rt_handle,
                    "open the installed apps settings",
                    open_path(INSTALLED_APPS_SETTINGS),
                );
            },
        )),
        Duration::ZERO,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTALL_DIR: &str = "C:\\Program Files\\forge";

    #[test]
    fn leftover_notice_is_raised_only_for_an_unnoticed_leftover_install() {
        for (leftover, noticed, expected) in [
            (None, false, None),
            (None, true, None),
            (Some(PathBuf::from(INSTALL_DIR)), true, None),
            (
                Some(PathBuf::from(INSTALL_DIR)),
                false,
                Some(LeftoverNotice {
                    install_dir: PathBuf::from(INSTALL_DIR),
                }),
            ),
        ] {
            assert_eq!(
                leftover_notice(leftover.clone(), noticed),
                expected,
                "{leftover:?} noticed={noticed}"
            );
        }
    }
}
