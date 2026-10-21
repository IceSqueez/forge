use std::sync::Arc;

use forge_components::{ToastAction, ToastData, ToastKind, Toasts, tr};
use forge_runtime::sub_action_runners::NotifyPort;
use forge_storage::settings::DEFAULT_NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE;
use forge_storage::{SettingsRepo, get_bool_setting, reserved_keys};
use forge_transfer::{FailureStreak, ScheduledBackupHandle, ScheduledBackupStatus};
use gpui::{App, AsyncApp, Global};
use time::OffsetDateTime;
use tokio::runtime::Handle;
use tokio::sync::watch;

use crate::backup_schedule::failure_reason;
use crate::screen::Screen;
use crate::settings::SettingsSection;
use crate::system_notices::{FallbackLog, NoticeText, show_off_thread};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupFailureNoticePreference(pub bool);

impl Default for BackupFailureNoticePreference {
    fn default() -> Self {
        Self(DEFAULT_NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE)
    }
}

impl Global for BackupFailureNoticePreference {}

impl BackupFailureNoticePreference {
    pub fn current(cx: &App) -> bool {
        cx.try_global::<Self>().copied().unwrap_or_default().0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreakChange {
    Started(FailureStreak),
    Ended,
    Unchanged,
}

#[derive(Debug, Default)]
pub struct FailureStreakWatch {
    announced: Option<OffsetDateTime>,
}

impl FailureStreakWatch {
    pub fn observe(&mut self, failure: Option<FailureStreak>) -> StreakChange {
        match failure {
            Some(streak) if streak.attempts == 1 && self.announced != Some(streak.since) => {
                self.announced = Some(streak.since);
                StreakChange::Started(streak)
            }
            Some(_) => StreakChange::Unchanged,
            None if self.announced.take().is_some() => StreakChange::Ended,
            None => StreakChange::Unchanged,
        }
    }
}

pub fn failure_notice_text(streak: &FailureStreak) -> NoticeText {
    NoticeText {
        title: tr!("system_notice_backup_failed_title"),
        body: tr!(
            "system_notice_backup_failed_body",
            reason = failure_reason(streak.category)
        ),
    }
}

pub fn start_scheduled_backup_notices(
    cx: &mut AsyncApp,
    scheduled: &ScheduledBackupHandle,
    settings: Arc<dyn SettingsRepo>,
    port: Arc<dyn NotifyPort>,
    rt: Handle,
) {
    let status = scheduled.status();
    cx.spawn(async move |cx| {
        let enabled = load_preference(&rt, settings).await;
        cx.update(|cx| cx.set_global(BackupFailureNoticePreference(enabled)));
        drain_failures(status, port, rt, cx).await;
    })
    .detach();
}

async fn load_preference(rt: &Handle, settings: Arc<dyn SettingsRepo>) -> bool {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt.spawn(async move {
        let enabled = get_bool_setting(
            settings.as_ref(),
            reserved_keys::NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE,
            DEFAULT_NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE,
        )
        .await;
        let _ = tx.send(enabled);
    });
    rx.await
        .unwrap_or(DEFAULT_NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE)
}

pub async fn drain_failures(
    mut status: watch::Receiver<ScheduledBackupStatus>,
    port: Arc<dyn NotifyPort>,
    rt: Handle,
    cx: &mut AsyncApp,
) {
    let mut streaks = FailureStreakWatch::default();
    let mut fallback_log = FallbackLog::default();
    let mut toast: Option<u64> = None;
    loop {
        let failure = status.borrow_and_update().failure;
        match streaks.observe(failure) {
            StreakChange::Started(streak) => {
                let text = cx.update(|_| failure_notice_text(&streak));
                let os_notice = cx.update(|cx| {
                    if let Some(previous) = toast.take() {
                        dismiss_toast(previous, cx);
                    }
                    toast = raise_failure_toast(&text, cx);
                    BackupFailureNoticePreference::current(cx)
                });
                if os_notice
                    && let Err(error) =
                        show_off_thread(&rt, Arc::clone(&port), text.desktop_notice()).await
                    && fallback_log.first_failure()
                {
                    tracing::warn!(
                        error = %error,
                        "desktop notifications unavailable; backup failures stay in-app"
                    );
                }
            }
            StreakChange::Ended => {
                if let Some(previous) = toast.take() {
                    cx.update(|cx| dismiss_toast(previous, cx));
                }
            }
            StreakChange::Unchanged => {}
        }
        if status.changed().await.is_err() {
            break;
        }
    }
}

fn raise_failure_toast(text: &NoticeText, cx: &mut App) -> Option<u64> {
    if !cx.has_global::<Toasts>() {
        return None;
    }
    let action = ToastAction::new(tr!("system_notice_backup_failed_open"), |_window, cx| {
        crate::shell::open_screen(Screen::Settings(Some(SettingsSection::Storage)), cx);
    });
    let id = cx.global_mut::<Toasts>().show(ToastData {
        id: 0,
        kind: ToastKind::Error,
        message: text.toast_message().into(),
        icon: None,
        action: Some(action),
        secondary_action: None,
        progress: None,
    });
    Some(id)
}

fn dismiss_toast(id: u64, cx: &mut App) {
    if cx.has_global::<Toasts>() {
        cx.global_mut::<Toasts>().dismiss(id);
    }
}
