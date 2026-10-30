use std::sync::Arc;

use forge_components::{ToastAction, ToastData, ToastKind, Toasts, tr};
use forge_runtime::sub_action_runners::NotifyPort;
use forge_storage::settings::DEFAULT_NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE;
use forge_storage::{SettingsRepo, read_setting_or_default};
use forge_transfer::{FailureStreak, ScheduledBackupHandle, ScheduledBackupStatus};
use gpui::{App, AsyncApp, Global};
use time::OffsetDateTime;
use tokio::runtime::Handle;
use tokio::sync::watch;

use crate::settings_catalog::SCHEDULED_BACKUP_FAILURE_OS_NOTICE;

use crate::backup_schedule::failure_reason;
use crate::navigation::{Navigator, Screen, SettingsSection};
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
    navigator: Navigator,
) {
    let status = scheduled.status();
    cx.spawn(async move |cx| {
        let enabled = load_preference(&rt, settings).await;
        cx.update(|cx| cx.set_global(BackupFailureNoticePreference(enabled)));
        drain_failures(status, port, rt, navigator, cx).await;
    })
    .detach();
}

async fn load_preference(rt: &Handle, settings: Arc<dyn SettingsRepo>) -> bool {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt.spawn(async move {
        let enabled =
            read_setting_or_default(settings.as_ref(), &SCHEDULED_BACKUP_FAILURE_OS_NOTICE).await;
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the view may close before the result arrives"
        )]
        let _ = tx.send(enabled);
    });
    rx.await
        .unwrap_or(DEFAULT_NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE)
}

pub async fn drain_failures(
    mut status: watch::Receiver<ScheduledBackupStatus>,
    port: Arc<dyn NotifyPort>,
    rt: Handle,
    navigator: Navigator,
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
                    toast = raise_failure_toast(&text, &navigator, cx);
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

fn raise_failure_toast(text: &NoticeText, navigator: &Navigator, cx: &mut App) -> Option<u64> {
    if !cx.has_global::<Toasts>() {
        return None;
    }
    let navigator = navigator.clone();
    let action = ToastAction::new(
        tr!("system_notice_backup_failed_open"),
        move |_window, cx| {
            navigator.open(Screen::Settings(Some(SettingsSection::Storage)), cx);
        },
    );
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Mutex;

    use forge_components::{ToastKind, Toasts};
    use forge_runtime::sub_action_runners::{DesktopNotice, OsPortError};
    use forge_storage::Language;
    use forge_transfer::{FailureCategory, ScheduleState};
    use gpui::TestAppContext;
    use time::Duration;

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{pump, runtime};

    const SETTLE_ROUNDS: usize = 2000;

    fn streak(since_offset_mins: i64, attempts: u32) -> FailureStreak {
        FailureStreak {
            category: FailureCategory::KeyMissing,
            since: OffsetDateTime::UNIX_EPOCH + Duration::minutes(since_offset_mins),
            attempts,
        }
    }

    fn status(failure: Option<FailureStreak>) -> ScheduledBackupStatus {
        ScheduledBackupStatus {
            state: ScheduleState::Idle,
            last_success_at: None,
            failure,
            next_due: None,
            prune_failures: 0,
            storage_used: None,
        }
    }

    #[test]
    fn a_streak_is_announced_once_however_many_attempts_and_republishes_follow() {
        let mut watch = FailureStreakWatch::default();
        let first = streak(0, 1);

        let changes: Vec<StreakChange> = [None, Some(first), Some(first), Some(streak(0, 2))]
            .into_iter()
            .chain([Some(streak(0, 3))])
            .map(|failure| watch.observe(failure))
            .collect();

        assert_eq!(
            changes,
            vec![
                StreakChange::Unchanged,
                StreakChange::Started(first),
                StreakChange::Unchanged,
                StreakChange::Unchanged,
                StreakChange::Unchanged,
            ]
        );
    }

    #[test]
    fn a_cleared_streak_ends_once_and_the_next_streak_is_announced_again() {
        let mut watch = FailureStreakWatch::default();
        let next = streak(90, 1);

        let changes: Vec<StreakChange> = [Some(streak(0, 1)), Some(streak(0, 2)), None, None]
            .into_iter()
            .chain([Some(next)])
            .map(|failure| watch.observe(failure))
            .collect();

        assert_eq!(
            changes,
            vec![
                StreakChange::Started(streak(0, 1)),
                StreakChange::Unchanged,
                StreakChange::Ended,
                StreakChange::Unchanged,
                StreakChange::Started(next),
            ]
        );
    }

    #[derive(Default)]
    struct RecordingPort {
        shown: Mutex<Vec<String>>,
    }

    impl RecordingPort {
        fn titles(&self) -> Vec<String> {
            self.shown.lock().unwrap().clone()
        }
    }

    impl NotifyPort for RecordingPort {
        fn show(&self, notice: DesktopNotice) -> Result<(), OsPortError> {
            self.shown.lock().unwrap().push(notice.title);
            Ok(())
        }
    }

    struct Drain {
        rt: tokio::runtime::Runtime,
        tx: watch::Sender<ScheduledBackupStatus>,
        port: Arc<RecordingPort>,
        opened: Rc<RefCell<Vec<Screen>>>,
    }

    type ShownToast = (ToastKind, Option<String>);

    fn drain(cx: &mut TestAppContext, os_notice: bool) -> Drain {
        install_language(Language::En);
        cx.executor().allow_parking();
        cx.update(|cx| {
            cx.set_global(Toasts::new());
            cx.set_global(BackupFailureNoticePreference(os_notice));
        });
        let rt = runtime();
        let (tx, rx) = watch::channel(status(None));
        let port = Arc::new(RecordingPort::default());
        let notify = Arc::clone(&port) as Arc<dyn NotifyPort>;
        let handle = rt.handle().clone();
        let opened = Rc::new(RefCell::new(Vec::new()));
        let recorded = Rc::clone(&opened);
        let navigator = Navigator::new(move |screen, _| recorded.borrow_mut().push(screen));
        cx.update(|cx| {
            cx.spawn(async move |cx| drain_failures(rx, notify, handle, navigator, cx).await)
                .detach();
        });
        settle(cx, &rt, |_, _| true);
        Drain {
            rt,
            tx,
            port,
            opened,
        }
    }

    fn toasts(cx: &mut TestAppContext) -> Vec<ShownToast> {
        cx.update(|cx| {
            cx.global::<Toasts>()
                .items()
                .iter()
                .map(|toast| {
                    (
                        toast.kind,
                        toast.action.as_ref().map(|action| action.label.to_string()),
                    )
                })
                .collect()
        })
    }

    fn settle(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        done: impl Fn(&mut TestAppContext, &tokio::runtime::Runtime) -> bool,
    ) {
        for _ in 0..SETTLE_ROUNDS {
            pump(rt);
            cx.run_until_parked();
            if done(cx, rt) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        pump(rt);
        cx.run_until_parked();
    }

    fn publish(
        cx: &mut TestAppContext,
        rig: &Drain,
        failure: Option<FailureStreak>,
        settled: (usize, usize),
    ) {
        rig.tx.send_replace(status(failure));
        let port = Arc::clone(&rig.port);
        settle(cx, &rig.rt, move |cx, _| {
            (port.titles().len(), toasts(cx).len()) == settled
        });
    }

    fn open_settings_error() -> ShownToast {
        (
            ToastKind::Error,
            Some(tr!("system_notice_backup_failed_open")),
        )
    }

    #[gpui::test]
    fn a_streak_of_many_attempts_raises_one_sticky_error_toast_and_one_os_notice(
        cx: &mut TestAppContext,
    ) {
        let rig = drain(cx, true);

        for attempts in 1..=3 {
            publish(cx, &rig, Some(streak(0, attempts)), (1, 1));
        }

        assert_eq!(
            (toasts(cx), rig.port.titles()),
            (
                vec![open_settings_error()],
                vec![tr!("system_notice_backup_failed_title")]
            )
        );
    }

    #[gpui::test]
    fn the_end_of_a_streak_dismisses_its_toast(cx: &mut TestAppContext) {
        let rig = drain(cx, true);
        publish(cx, &rig, Some(streak(0, 1)), (1, 1));

        publish(cx, &rig, None, (1, 0));

        assert_eq!(toasts(cx), Vec::new());
    }

    #[gpui::test]
    fn a_new_streak_after_a_success_raises_a_fresh_toast_and_os_notice(cx: &mut TestAppContext) {
        let rig = drain(cx, true);
        publish(cx, &rig, Some(streak(0, 1)), (1, 1));
        publish(cx, &rig, None, (1, 0));

        publish(cx, &rig, Some(streak(90, 1)), (2, 1));

        assert_eq!(
            (toasts(cx), rig.port.titles().len()),
            (vec![open_settings_error()], 2)
        );
    }

    #[gpui::test]
    fn with_the_os_notice_toggle_off_a_streak_raises_the_toast_but_no_os_notice(
        cx: &mut TestAppContext,
    ) {
        let rig = drain(cx, false);

        publish(cx, &rig, Some(streak(0, 1)), (0, 1));

        assert_eq!(
            (toasts(cx), rig.port.titles()),
            (vec![open_settings_error()], Vec::<String>::new())
        );
    }

    #[gpui::test]
    fn the_failure_toast_open_action_opens_the_storage_settings_pane(cx: &mut TestAppContext) {
        let rig = drain(cx, false);
        publish(cx, &rig, Some(streak(0, 1)), (0, 1));
        let toast = cx.update(|cx| {
            let id = cx.global::<Toasts>().items()[0].id;
            cx.global_mut::<Toasts>().take(id)
        });
        let action = toast.and_then(|toast| toast.action).unwrap();

        cx.add_empty_window()
            .update(|window, cx| (action.on_action)(window, cx));

        assert_eq!(
            *rig.opened.borrow(),
            vec![Screen::Settings(Some(SettingsSection::Storage))]
        );
    }
}
