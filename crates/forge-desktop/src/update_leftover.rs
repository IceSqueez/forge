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
    leftover_notice(Some(leftover), noticed)
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
    let detection_backend = Arc::clone(&backend);
    rt_handle.spawn(async move {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the view may close before the result arrives"
        )]
        let _ = tx.send(unnoticed_leftover(detection_backend).await);
    });
    cx.spawn(async move |cx| {
        let Ok(Some(notice)) = rx.await else {
            return;
        };
        tracing::info!(path = %notice.install_dir.display(), "leftover all-users install found");
        cx.update(|cx| raise_leftover_toast(backend, rt_handle, cx));
    })
    .detach();
}

async fn remember_leftover_noticed(repo: &dyn SettingsRepo) {
    if let Err(error) = write_setting(repo, &LEFTOVER_INSTALL_NOTICED, &true).await {
        tracing::warn!(error = %error, "leftover install notice could not be remembered");
    }
}

async fn open_installed_apps(repo: Arc<dyn SettingsRepo>) -> Result<(), String> {
    remember_leftover_noticed(repo.as_ref()).await;
    open_path(INSTALLED_APPS_SETTINGS).await
}

fn raise_leftover_toast(backend: Arc<dyn DataProvider>, rt_handle: Handle, cx: &mut App) {
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
                    open_installed_apps(Arc::clone(&backend) as Arc<dyn SettingsRepo>),
                );
            },
        )),
        Duration::ZERO,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{pump, runtime, test_backend, writes_to};

    const INSTALL_DIR: &str = "C:\\Program Files\\forge";
    const NOTICED_KEY: &str = "updates.leftover_install_noticed";

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

    #[gpui::test]
    fn raising_the_notice_records_nothing_until_the_user_acts(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let rt = runtime();
        let (backend, mut writes) = test_backend();
        let provider = Arc::clone(&backend) as Arc<dyn DataProvider>;

        cx.update(|cx| raise_leftover_toast(provider, rt.handle().clone(), cx));
        pump(&rt);
        cx.run_until_parked();

        let shown = cx.update(|cx| cx.global::<forge_components::Toasts>().items().len());
        assert_eq!(
            (shown, writes_to(&mut writes, NOTICED_KEY)),
            (1, Vec::<String>::new())
        );
    }

    #[test]
    fn remembering_the_notice_silences_it_on_the_next_launch() {
        let rt = runtime();
        let (backend, _writes) = test_backend();

        rt.block_on(remember_leftover_noticed(backend.as_ref()));

        let noticed = rt.block_on(read_setting_or_default(
            backend.as_ref(),
            &LEFTOVER_INSTALL_NOTICED,
        ));
        assert_eq!(
            leftover_notice(Some(PathBuf::from(INSTALL_DIR)), noticed),
            None
        );
    }

    #[test]
    fn a_notice_that_could_not_be_remembered_keeps_showing_on_the_next_launch() {
        let rt = runtime();
        let (backend, _writes) = test_backend();
        backend.fail_writes(NOTICED_KEY);

        rt.block_on(remember_leftover_noticed(backend.as_ref()));

        let noticed = rt.block_on(read_setting_or_default(
            backend.as_ref(),
            &LEFTOVER_INSTALL_NOTICED,
        ));
        assert!(leftover_notice(Some(PathBuf::from(INSTALL_DIR)), noticed).is_some());
    }
}
